use crate::app::{self, App, InputField};
use crate::audio::play_next_file;
use crate::ui::ui;
use lofty::flac::FlacFile;
use lofty::{config::{ParseOptions, WriteOptions}, ogg::VorbisComments, prelude::*};
use ratatui::{backend::CrosstermBackend, Terminal};
use rodio::Sink;
use std::path::Path;
use std::{fs, fs::File, io, process::Command, time::Duration};
use walkdir::WalkDir;

pub fn build_initial_metadata(file_count: usize) -> Vec<app::FileMetadata> {
    (0..file_count)
        .map(|_| app::FileMetadata {
            tags: Vec::new(),
            location: None,
            trim_from: None,
            trim_to: None,
            marked_for_deletion: false,
        })
        .collect()
}

pub fn get_wav_files_in_current_directory() -> Vec<String> {
    WalkDir::new(".")
        .into_iter()
        .filter_map(|entry| {
            entry.ok().and_then(|e| {
                let path = e.path();
                if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("wav") {
                    path.to_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
        })
        .collect()
}

pub fn load_location_suggestions() -> io::Result<Vec<String>> {
    let path = Path::new("suggestions.txt");
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(path)?;
    let mut suggestions = Vec::new();
    for line in contents.lines() {
        let suggestion = line.trim();
        if suggestion.is_empty() {
            continue;
        }
        if !suggestions
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(suggestion))
        {
            suggestions.push(suggestion.to_string());
        }
    }
    Ok(suggestions)
}

pub fn save_current_file_inputs(app: &mut App) {
    let location = app.location_input.trim().to_string();
    app.metadata[app.current_file_index].location = if location.is_empty() {
        None
    } else {
        add_location_suggestion(app, &location);
        Some(location)
    };

    app.metadata[app.current_file_index].tags.extend(
        app.tags_input
            .split(',')
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty()),
    );
}

fn add_location_suggestion(app: &mut App, location: &str) {
    let exists = app
        .location_suggestions
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(location));
    if !exists {
        app.location_suggestions.push(location.to_string());
    }
}

pub fn process_all_files(
    sink: &Sink,
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error + 'static>> {
    sink.stop();
    app.state = app::AppState::Processing;
    terminal.draw(|f| ui(f, app))?;
    convert_all_to_flac(app, app.auto_compute_filename)?;
    write_all_metadata(app, app.auto_compute_filename)?;
    if app.keep_original_files {
        remove_marked_wav_files(app)?;
    } else {
        remove_original_wav_files(app)?;
    }
    persist_location_suggestions(app)?;
    app.should_quit = true;
    Ok(())
}

fn persist_location_suggestions(app: &App) -> io::Result<()> {
    if app.location_suggestions.is_empty() {
        fs::write("suggestions.txt", "")?;
        return Ok(());
    }
    let mut body = app.location_suggestions.join("\n");
    body.push('\n');
    fs::write("suggestions.txt", body)
}

pub fn delete_file(sink: &Sink, app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    app.metadata[app.current_file_index].marked_for_deletion = true;

    if app.current_file_index + 1 >= app.available_files.len() {
        sink.stop();
        app.state = app::AppState::ReviewOutputNaming;
    } else {
        app.current_file_index += 1;
        play_next_file(sink, app)?;
        app.state = app::AppState::EditingMetadata;
        app.location_input.clear();
        app.tags_input.clear();
        app.active_input_field = InputField::Location;
    }

    Ok(())
}

fn convert_to_flac(input: &str, output: &str, trim_from: Option<Duration>, trim_to: Option<Duration>) -> anyhow::Result<()> {
    let mut args: Vec<String> = Vec::new();
    if let Some(from) = trim_from {
        args.push("-ss".to_string());
        args.push(format_duration_for_ffmpeg(from));
    }
    if let Some(to) = trim_to {
        args.push("-to".to_string());
        args.push(format_duration_for_ffmpeg(to));
    }
    args.extend([
        "-i".to_string(),
        input.to_string(),
        "-compression_level".to_string(),
        "8".to_string(),
        "-y".to_string(),
        output.to_string(),
    ]);

    let status = Command::new("ffmpeg")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("FFmpeg conversion failed"))
    }
}

fn convert_all_to_flac(app: &App, auto_compute_filename: bool) -> anyhow::Result<()> {
    for (index, file) in app.available_files.iter().enumerate() {
        if !is_index_in_processing_range(app, index) || app.metadata[index].marked_for_deletion {
            continue;
        }
        let output = flac_output_path(file, &app.metadata[index], auto_compute_filename, index);
        let metadata = &app.metadata[index];
        convert_to_flac(file, output.as_str(), metadata.trim_from, metadata.trim_to)?;
    }
    Ok(())
}

fn write_all_metadata(app: &App, auto_compute_filename: bool) -> Result<(), Box<dyn std::error::Error + 'static>> {
    for (index, file) in app.available_files.iter().enumerate() {
        if !is_index_in_processing_range(app, index) || app.metadata[index].marked_for_deletion {
            continue;
        }
        let path = flac_output_path(file, &app.metadata[index], auto_compute_filename, index);
        write_metadata_to_file(path.as_str(), &app.metadata[index])?;
    }
    Ok(())
}

fn flac_output_path(
    input_wav_path: &str,
    metadata: &app::FileMetadata,
    auto_compute_filename: bool,
    index: usize,
) -> String {
    if !auto_compute_filename {
        return original_flac_output_path(input_wav_path);
    }

    let location = metadata
        .location
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");

    let tags_missing = metadata.tags.is_empty();
    let tags = if tags_missing {
        "no tags".to_string()
    } else {
        metadata.tags.join(" ")
    };
    let location_missing = metadata
        .location
        .as_ref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true);
    let has_incomplete_naming_fields = location_missing || tags_missing;

    let raw_name = format!("{location} {tags}");
    let sanitized_name = sanitize_for_filename(raw_name.trim());
    let file_name = if has_incomplete_naming_fields {
        format!("{sanitized_name} {}.flac", index + 1)
    } else {
        format!("{sanitized_name}.flac")
    };

    Path::new(input_wav_path)
        .parent()
        .map(|parent| parent.join(&file_name).to_string_lossy().into_owned())
        .unwrap_or(file_name)
}

fn original_flac_output_path(input_wav_path: &str) -> String {
    let input_path = Path::new(input_wav_path);
    let file_stem = input_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");
    let file_name = format!("{file_stem}.flac");
    input_path
        .parent()
        .map(|parent| parent.join(&file_name).to_string_lossy().into_owned())
        .unwrap_or(file_name)
}

fn remove_original_wav_files(app: &App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    for (index, file) in app.available_files.iter().enumerate() {
        if !is_index_in_processing_range(app, index) {
            continue;
        }
        std::fs::remove_file(file)?;
    }
    Ok(())
}

fn remove_marked_wav_files(app: &App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    for (index, file) in app.available_files.iter().enumerate() {
        if !is_index_in_processing_range(app, index) {
            continue;
        }
        if app.metadata[index].marked_for_deletion {
            std::fs::remove_file(file)?;
        }
    }
    Ok(())
}

fn is_index_in_processing_range(app: &App, index: usize) -> bool {
    let end_index = app
        .processing_end_index
        .unwrap_or_else(|| app.available_files.len().saturating_sub(1));
    index <= end_index
}

fn sanitize_for_filename(input: &str) -> String {
    let collapsed = input.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = collapsed.replace('/', "_");
    if cleaned.is_empty() {
        "unknown no tags".to_string()
    } else {
        cleaned
    }
}

fn write_metadata_to_file(path: &str, metadata: &app::FileMetadata) -> Result<(), Box<dyn std::error::Error + 'static>> {
    let mut file = File::open(path)?;
    let mut flac_file = FlacFile::read_from(&mut file, ParseOptions::new())?;
    let mut tag = VorbisComments::default();

    if !metadata.tags.is_empty() {
        tag.insert(String::from("TAGS"), metadata.tags.join(", "));
    }
    if let Some(location) = &metadata.location {
        tag.insert(String::from("LOCATION"), location.to_string());
    }

    flac_file.set_vorbis_comments(tag);
    flac_file.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

fn format_duration_for_ffmpeg(duration: Duration) -> String {
    format!("{:.3}", duration.as_secs_f64())
}
