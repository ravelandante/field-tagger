use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Gauge, Paragraph},
    Terminal,
};
use rodio::{Decoder, OutputStream, Sink, Source};
use std::{fs::File, io, io::BufReader, time::Duration};

struct App {
    input: String,
    total_duration: Duration,
    current_duration: Duration,
    progress: f64,
    should_quit: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;

    let file = File::open("../serenity.wav")?;

    let source = Decoder::new(BufReader::new(file))?;

    let duration = source.total_duration().unwrap_or(Duration::from_secs(0));

    sink.append(source);
    
    let mut app = App {
        input: String::new(),
        total_duration: duration,
        current_duration: Duration::from_secs(0),
        progress: 0.0,
        should_quit: false,
    };

    loop {
        terminal.draw(|f| ui(f, &app))?;

        app.current_duration = sink.get_pos();

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => app.should_quit = true,
                    KeyCode::Enter => {
                        // save tags and go to next file
                        app.input.clear();
                    }
                    KeyCode::Char(c) => {
                        app.input.push(c);
                    }
                    KeyCode::Backspace => {
                        app.input.pop();
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

    // clean up terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // progress bar
            Constraint::Length(3), // input
            Constraint::Min(0),    // instructions + info
        ])
        .split(f.size());

    // progress bar
    let playback_bar = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Playback Progress"))
        .gauge_style(ratatui::style::Style::default().fg(ratatui::style::Color::Cyan))
        .ratio(app.progress.clamp(0.0, 1.0))
        .label(format!("{}s / {}s", app.current_duration.as_secs(), app.total_duration.as_secs()));
    f.render_widget(playback_bar, chunks[0]);

    // input
    let input_panel = Paragraph::new(app.input.as_str())
        .block(Block::default().borders(Borders::ALL).title("Enter Tags / Location"));
    f.render_widget(input_panel, chunks[1]);

    // instructions + info
    let help_text = Paragraph::new("ESC: Quit | Enter: Save & Next | Arrows: Seek");
    f.render_widget(help_text, chunks[2]);
}