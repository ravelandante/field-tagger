use crate::app::{App, AppState};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Gauge, Paragraph},
    Frame,
};

pub fn ui(f: &mut Frame<>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // progress bar
            Constraint::Length(3), // input
            Constraint::Min(0),    // instructions + info
        ])
        .split(f.area());

    // progress bar
    let playback_bar = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Playback Progress"))
        .gauge_style(ratatui::style::Style::default().fg(ratatui::style::Color::Cyan))
        .ratio(app.progress.clamp(0.0, 1.0))
        .label(format!("{:02}:{:02} / {:02}:{:02}",
            app.current_duration.as_secs() / 60,
            app.current_duration.as_secs() % 60,
            app.total_duration.as_secs() / 60,
            app.total_duration.as_secs() % 60));
    f.render_widget(playback_bar, chunks[0]);

    // input
    let input_title = match app.state {
        AppState::AskingForTags => "Enter Tags",
        AppState::AskingForLocation => "Enter Location",
        _ => "",
    };

    let input_panel = Paragraph::new(app.input.as_str())
        .block(Block::default().borders(Borders::ALL).title(input_title));
    f.render_widget(input_panel, chunks[1]);

    // instructions + info
    let help_text = Paragraph::new("ESC: Quit | Enter: Save & Next | Arrows: Seek | Del: Delete File")
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    f.render_widget(help_text, chunks[2]);
}