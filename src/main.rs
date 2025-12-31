use app::App;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{CrosstermBackend},
    Terminal,
};
use rodio::{Decoder, OutputStream, Sink, Source};
use std::{fs::File, io, io::BufReader, time::Duration};
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

    let metadata: Vec<app::FileMetadata> = available_files.iter().map(|_| app::FileMetadata {
        tags: Vec::new(),
        location: None,
    }).collect();

    let mut app = App {
        metadata,
        input: String::new(),
        total_duration: duration,
        current_duration: Duration::from_secs(0),
        progress: 0.0,
        current_file_index: 0,
        should_quit: false,
        available_files,
        state: app::AppState::AskingForLocation,
        waveform_data,
    };
    
    sink.append(source);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        app.current_duration = sink.get_pos();

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => app.should_quit = true,
                    KeyCode::Enter => {
                        handle_enter_key(&sink, &mut app, &mut terminal)?;
                    }
                    KeyCode::Char(c) => {
                        app.input.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
                    }
                    KeyCode::Delete => {
                        delete_file(&sink, &mut app)?;
                    }
                    KeyCode::Right => {
                        sink.try_seek(Duration::from_millis((app.current_duration.as_millis() + 5000) as u64))?;
                    }
                    KeyCode::Left => {
                        let seeked_position = if app.current_duration.as_millis() < 5000 { 0 } else { app.current_duration.as_millis() - 5000 };
                        sink.try_seek(Duration::from_millis(seeked_position as u64))?;
                    }
                    _ => {}
                }
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

fn handle_enter_key(sink: &Sink, app: &mut App, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<(), Box<dyn std::error::Error + 'static>> {
    Ok(match app.state {
        app::AppState::AskingForLocation => {
            app.state = app::AppState::AskingForTags;
            app.metadata[app.current_file_index].location = Some(app.input.trim().to_string());
            app.input.clear();
        }
        app::AppState::AskingForTags => {
            app.state = app::AppState::Processing;
            // conversion below blocks update, so update state + ui explicitly before blocking work
            terminal.draw(|f| ui(f, app))?;
            
            app.metadata[app.current_file_index].tags.extend(
                app.input.split(',')
                .map(|tag| tag.trim().to_string())
                .filter(|tag| !tag.is_empty())
            );
        
            app.current_file_index += 1;
            if app.current_file_index >= app.available_files.len() {
                sink.stop();

                convert_all_to_flac(&app)?;
                write_metadata_to_file(
                    &*format!("{}.flac", app.available_files[app.current_file_index - 1].trim_end_matches(".wav")),
                    &app.metadata[app.current_file_index - 1]
                )?;
                app.should_quit = true;
            } else {
                play_next_file(sink, app)?;
                app.state = app::AppState::AskingForLocation;
            }
            app.input.clear();
        }
        _ => {}
    })
}

fn delete_file(sink: &Sink, app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    sink.stop();
    std::fs::remove_file(&*app.available_files[app.current_file_index])?;
    app.available_files.remove(app.current_file_index);

    if app.current_file_index >= app.available_files.len() {
        app.should_quit = true;
    } else {
        play_next_file(sink, app)?;
        app.input.clear();
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
    sink.stop();
    let next_file = File::open(&*app.available_files[app.current_file_index])?;
    let next_source = Decoder::new(BufReader::new(next_file))?;
    app.total_duration = next_source.total_duration().unwrap_or(Duration::from_secs(0));
    app.waveform_data = extract_waveform(&app.available_files[app.current_file_index], 200)?;
    sink.append(next_source);
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

fn convert_to_flac(input: &str, output: &str) -> anyhow::Result<()> {
    let status = Command::new("ffmpeg")
        .args([
            "-i", input,
            "-compression_level", "8",
            "-y",
            output,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("FFmpeg conversion failed"))
    }
}

fn convert_all_to_flac(app: &App) -> anyhow::Result<()> {
    for file in &app.available_files {
        let output = format!("{}.flac", file.trim_end_matches(".wav"));
        convert_to_flac(file, &output)?;
    }
    Ok(())
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
