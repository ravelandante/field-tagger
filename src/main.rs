use app::{App, InputField, InteractionMode, ReviewOption, TrimSide};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{CrosstermBackend},
    Terminal,
};
use rodio::{Decoder, OutputStream, Sink, Source};
use std::{fs::File, io, io::BufReader, path::Path, time::{Duration, Instant}};
use walkdir::WalkDir;
use std::process::Command;
use lofty::{config::{ParseOptions, WriteOptions}, ogg::VorbisComments, prelude::*};
use lofty::flac::FlacFile;
use hound::WavReader;

mod app;
mod ui;

use ui::ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let available_files = get_wav_files_in_current_directory();
    if available_files.is_empty() {
        clean_up_terminal(&mut terminal)?;
        println!("No .wav files found in the current directory.");
        return Ok(());
    }

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;

    let file = File::open(&available_files[0])?;

    let source = Decoder::new(BufReader::new(file))?;

    let duration = source.total_duration().unwrap_or(Duration::from_secs(0));

    let waveform_data = extract_waveform(&available_files[0], 200)?;

    let metadata: Vec<app::FileMetadata> = available_files
        .iter()
        .map(|_| app::FileMetadata {
            tags: Vec::new(),
            location: None,
            trim_from: None,
            trim_to: None,
        })
        .collect();

    let mut app = App {
        metadata,
        location_input: String::new(),
        tags_input: String::new(),
        total_duration: duration,
        current_duration: Duration::from_secs(0),
        progress: 0.0,
        current_file_index: 0,
        should_quit: false,
        available_files,
        state: app::AppState::EditingMetadata,
        active_input_field: InputField::Location,
        waveform_data,
        interaction_mode: InteractionMode::Normal,
        active_trim_side: TrimSide::From,
        trim_warning: None,
        is_paused: false,
        seek_repeat_count: 0,
        last_seek_direction: 0,
        last_seek_at: None,
        auto_compute_filename: true,
        keep_original_files: true,
        review_selected_option: ReviewOption::AutoComputeFilename,
    };
    
    sink.append(source);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        app.current_duration = sink.get_pos();
        enforce_playback_bounds(&sink, &mut app)?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                handle_key_event(key, &sink, &mut app, &mut terminal)?;
            }
        }

        app.progress = if app.total_duration.as_secs() > 0 {
            app.current_duration.as_secs_f64() / app.total_duration.as_secs_f64()
        } else {
            0.0
        };

        if app.should_quit { break; }
    }

    clean_up_terminal(&mut terminal)?;
    Ok(())
}

fn handle_key_event(
    key: KeyEvent,
    sink: &Sink,
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error + 'static>> {
    if matches!(key.code, KeyCode::Esc) {
        reset_seek_acceleration(app);
        app.should_quit = true;
        return Ok(());
    }

    if matches!(app.state, app::AppState::ReviewOutputNaming) {
        match key.code {
            KeyCode::Up | KeyCode::Down | KeyCode::Tab => {
                app.review_selected_option = match app.review_selected_option {
                    ReviewOption::AutoComputeFilename => ReviewOption::KeepOriginalFiles,
                    ReviewOption::KeepOriginalFiles => ReviewOption::AutoComputeFilename,
                };
            }
            KeyCode::Char(' ') => match app.review_selected_option {
                ReviewOption::AutoComputeFilename => {
                    app.auto_compute_filename = !app.auto_compute_filename
                }
                ReviewOption::KeepOriginalFiles => {
                    app.keep_original_files = !app.keep_original_files
                }
            },
            KeyCode::Enter => process_all_files(sink, app, terminal)?,
            _ => {}
        }
        return Ok(());
    }

    if matches!(key.code, KeyCode::F(6)) {
        toggle_interaction_mode(sink, app);
        return Ok(());
    }

    match key.code {
        KeyCode::Enter => {
            reset_seek_acceleration(app);
            handle_enter_key(sink, app)?;
        }
        KeyCode::Delete => {
            reset_seek_acceleration(app);
            delete_file(sink, app)?;
        }
        KeyCode::Right => seek_with_acceleration(sink, app, 1)?,
        KeyCode::Left => seek_with_acceleration(sink, app, -1)?,
        KeyCode::Up | KeyCode::Down if app.interaction_mode == InteractionMode::Normal => {
            reset_seek_acceleration(app);
            app.active_input_field = match app.active_input_field {
                InputField::Location => InputField::Tags,
                InputField::Tags => InputField::Location,
            };
        }
        KeyCode::Char(' ') if app.interaction_mode == InteractionMode::Trim => {
            reset_seek_acceleration(app);
            toggle_pause(sink, app)
        }
        KeyCode::Char('t') | KeyCode::Char('T') if app.interaction_mode == InteractionMode::Trim => {
            reset_seek_acceleration(app);
            app.active_trim_side = match app.active_trim_side {
                TrimSide::From => TrimSide::To,
                TrimSide::To => TrimSide::From,
            };
            app.trim_warning = None;
        }
        KeyCode::Backspace if app.interaction_mode == InteractionMode::Trim => {
            reset_seek_acceleration(app);
            clear_active_trim_marker(app);
        }
        KeyCode::Backspace if app.interaction_mode == InteractionMode::Normal => {
            reset_seek_acceleration(app);
            active_input_buffer_mut(app).pop();
        }
        KeyCode::Char(c) if app.interaction_mode == InteractionMode::Normal => {
            reset_seek_acceleration(app);
            active_input_buffer_mut(app).push(c);
        }
        KeyCode::Tab if app.interaction_mode == InteractionMode::Normal => {
            reset_seek_acceleration(app);
            app.active_input_field = match app.active_input_field {
                InputField::Location => InputField::Tags,
                InputField::Tags => InputField::Location,
            };
        }
        _ => reset_seek_acceleration(app),
    }

    Ok(())
}

fn toggle_interaction_mode(sink: &Sink, app: &mut App) {
    app.trim_warning = None;
    match app.interaction_mode {
        InteractionMode::Normal => {
            app.interaction_mode = InteractionMode::Trim;
            app.active_trim_side = TrimSide::From;
        }
        InteractionMode::Trim => {
            app.interaction_mode = InteractionMode::Normal;
            if app.is_paused {
                sink.play();
                app.is_paused = false;
            }
        }
    }
}

fn toggle_pause(sink: &Sink, app: &mut App) {
    if app.is_paused {
        sink.play();
        app.is_paused = false;
    } else {
        sink.pause();
        app.is_paused = true;
    }
}

fn seek_by(
    sink: &Sink,
    app: &App,
    delta_ms: i64,
) -> Result<(), Box<dyn std::error::Error + 'static>> {
    let (start_bound, end_bound) = playback_bounds(app);
    let current_ms = app.current_duration.as_millis() as i64;
    let target_ms = (current_ms + delta_ms).clamp(start_bound, end_bound) as u64;
    sink.try_seek(Duration::from_millis(target_ms))?;
    Ok(())
}

fn seek_with_acceleration(
    sink: &Sink,
    app: &mut App,
    direction: i8,
) -> Result<(), Box<dyn std::error::Error + 'static>> {
    let now = Instant::now();
    let is_continuing_hold = app.last_seek_direction == direction
        && app
            .last_seek_at
            .map(|last| now.duration_since(last) <= Duration::from_millis(250))
            .unwrap_or(false);

    if is_continuing_hold {
        app.seek_repeat_count = app.seek_repeat_count.saturating_add(1);
    } else {
        app.seek_repeat_count = 0;
    }

    app.last_seek_direction = direction;
    app.last_seek_at = Some(now);

    let step_ms = match app.seek_repeat_count {
        0..=2 => 500,
        3..=6 => 1_500,
        7..=12 => 3_000,
        _ => 5_000,
    };

    seek_by(sink, app, i64::from(direction) * step_ms)
}

fn reset_seek_acceleration(app: &mut App) {
    app.seek_repeat_count = 0;
    app.last_seek_direction = 0;
    app.last_seek_at = None;
}

fn handle_enter_key(sink: &Sink, app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    if app.interaction_mode == InteractionMode::Trim {
        return set_active_trim_marker(app);
    }

    let location = app.location_input.trim().to_string();
    app.metadata[app.current_file_index].location = if location.is_empty() {
        None
    } else {
        Some(location)
    };
    
    app.metadata[app.current_file_index].tags.extend(
        app.tags_input
            .split(',')
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
    );

    if app.current_file_index + 1 >= app.available_files.len() {
        sink.stop();
        app.state = app::AppState::ReviewOutputNaming;
    } else {
        app.current_file_index += 1;
        play_next_file(sink, app)?;
        app.state = app::AppState::EditingMetadata;
    }
    app.location_input.clear();
    app.tags_input.clear();
    app.active_input_field = InputField::Location;

    Ok(())
}

fn process_all_files(
    sink: &Sink,
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error + 'static>> {
    sink.stop();
    app.state = app::AppState::Processing;
    // conversion below blocks update, so update state + ui explicitly before blocking work
    terminal.draw(|f| ui(f, app))?;
    convert_all_to_flac(app, app.auto_compute_filename)?;
    write_all_metadata(app, app.auto_compute_filename)?;
    if !app.keep_original_files {
        remove_original_wav_files(app)?;
    }
    app.should_quit = true;
    Ok(())
}

fn set_active_trim_marker(app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    let current = app.current_duration.min(app.total_duration);
    let metadata = &mut app.metadata[app.current_file_index];
    app.trim_warning = None;

    match app.active_trim_side {
        TrimSide::From => {
            if let Some(to) = metadata.trim_to {
                if current > to {
                    app.trim_warning = Some("From cannot be greater than To".to_string());
                    return Ok(());
                }
            }
            metadata.trim_from = Some(current);
        }
        TrimSide::To => {
            if let Some(from) = metadata.trim_from {
                if current < from {
                    app.trim_warning = Some("To cannot be less than From".to_string());
                    return Ok(());
                }
            }
            metadata.trim_to = Some(current);
        }
    }

    Ok(())
}

fn clear_active_trim_marker(app: &mut App) {
    let metadata = &mut app.metadata[app.current_file_index];
    app.trim_warning = None;

    match app.active_trim_side {
        TrimSide::From => metadata.trim_from = None,
        TrimSide::To => metadata.trim_to = None,
    }
}

fn delete_file(sink: &Sink, app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    sink.clear();
    std::fs::remove_file(&*app.available_files[app.current_file_index])?;
    app.available_files.remove(app.current_file_index);
    app.metadata.remove(app.current_file_index);

    if app.current_file_index >= app.available_files.len() {
        app.should_quit = true;
    } else {
        play_next_file(sink, app)?;
        app.location_input.clear();
        app.tags_input.clear();
        app.active_input_field = InputField::Location;
    }

    Ok(())
}

fn get_wav_files_in_current_directory() -> Vec<String> {
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

fn play_next_file(sink: &Sink, app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    sink.clear();
    let next_file = File::open(&*app.available_files[app.current_file_index])?;
    let next_source = Decoder::new(BufReader::new(next_file))?;
    app.total_duration = next_source.total_duration().unwrap_or(Duration::from_secs(0));
    app.waveform_data = extract_waveform(&app.available_files[app.current_file_index], 200)?;
    app.current_duration = Duration::from_secs(0);
    app.interaction_mode = InteractionMode::Normal;
    app.active_trim_side = TrimSide::From;
    app.active_input_field = InputField::Location;
    app.trim_warning = None;
    app.is_paused = false;
    reset_seek_acceleration(app);
    sink.append(next_source);
    sink.play();
    Ok(())
}

fn clean_up_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
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
        let output = flac_output_path(file, &app.metadata[index], auto_compute_filename);
        let metadata = &app.metadata[index];
        convert_to_flac(file, output.as_str(), metadata.trim_from, metadata.trim_to)?;
    }
    Ok(())
}

fn write_all_metadata(app: &App, auto_compute_filename: bool) -> Result<(), Box<dyn std::error::Error + 'static>> {
    for (index, file) in app.available_files.iter().enumerate() {
        let path = flac_output_path(file, &app.metadata[index], auto_compute_filename);
        write_metadata_to_file(path.as_str(), &app.metadata[index])?;
    }
    Ok(())
}

fn flac_output_path(
    input_wav_path: &str,
    metadata: &app::FileMetadata,
    auto_compute_filename: bool,
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

    let tags = if metadata.tags.is_empty() {
        "no tags".to_string()
    } else {
        metadata.tags.join(" ")
    };

    let raw_name = format!("{location} {tags}");
    let sanitized_name = sanitize_for_filename(raw_name.trim());
    let file_name = format!("{sanitized_name}.flac");

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
    for file in &app.available_files {
        std::fs::remove_file(file)?;
    }
    Ok(())
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

fn extract_waveform(file_path: &str, num_points: usize) -> Result<Vec<u64>, Box<dyn std::error::Error + 'static>> {
    let mut reader = WavReader::open(file_path)?;
    let samples: Vec<i16> = reader.samples::<i16>().filter_map(Result::ok).collect();
    
    if samples.is_empty() {
        return Ok(vec![0; num_points]);
    }
    
    let samples_per_point = samples.len() / num_points;
    let mut waveform = Vec::with_capacity(num_points);
    
    for i in 0..num_points {
        let start = i * samples_per_point;
        let end = ((i + 1) * samples_per_point).min(samples.len());
        
        if start >= samples.len() {
            waveform.push(0);
            continue;
        }
        
        // Calculate RMS (root mean square) for this chunk
        let chunk = &samples[start..end];
        let sum_squares: f64 = chunk.iter().map(|&s| (s as f64).powi(2)).sum();
        let rms = (sum_squares / chunk.len() as f64).sqrt();
        
        // Normalize to 0-100 range for display
        let normalized = (rms / i16::MAX as f64 * 100.0) as u64;
        waveform.push(normalized);
    }
    
    Ok(waveform)
}

fn format_duration_for_ffmpeg(duration: Duration) -> String {
    format!("{:.3}", duration.as_secs_f64())
}

fn playback_bounds(app: &App) -> (i64, i64) {
    let metadata = &app.metadata[app.current_file_index];
    let total_ms = app.total_duration.as_millis() as i64;
    let start_ms = metadata
        .trim_from
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
        .clamp(0, total_ms);
    let end_ms = metadata
        .trim_to
        .map(|d| d.as_millis() as i64)
        .unwrap_or(total_ms)
        .clamp(0, total_ms);

    if start_ms <= end_ms {
        (start_ms, end_ms)
    } else {
        (end_ms, start_ms)
    }
}

fn enforce_playback_bounds(sink: &Sink, app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    let (start_bound, end_bound) = playback_bounds(app);
    let current_ms = app.current_duration.as_millis() as i64;

    if current_ms < start_bound {
        sink.try_seek(Duration::from_millis(start_bound as u64))?;
        app.current_duration = Duration::from_millis(start_bound as u64);
        return Ok(());
    }

    if current_ms > end_bound {
        sink.try_seek(Duration::from_millis(end_bound as u64))?;
        sink.pause();
        app.is_paused = true;
        app.current_duration = Duration::from_millis(end_bound as u64);
    }

    Ok(())
}

fn active_input_buffer_mut(app: &mut App) -> &mut String {
    match app.active_input_field {
        InputField::Location => &mut app.location_input,
        InputField::Tags => &mut app.tags_input,
    }
}
