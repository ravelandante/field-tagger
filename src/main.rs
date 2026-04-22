use app::{App, InputField, InteractionMode, ReviewOption, TrimSide};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{CrosstermBackend},
    Terminal,
};
use rodio::{Decoder, OutputStream, Sink, Source};
use std::{fs::File, io, io::BufReader, time::Duration};

mod app;
mod autocomplete;
mod audio;
mod input;
mod processing;
mod ui;

use audio::{enforce_playback_bounds, extract_waveform};
use input::handle_key_event;
use processing::{build_initial_metadata, get_wav_files_in_current_directory, load_location_suggestions};
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

    let metadata = build_initial_metadata(available_files.len());

    let location_suggestions = load_location_suggestions().unwrap_or_default();

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
        processing_end_index: None,
        location_suggestions,
        location_dropdown_index: 0,
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
