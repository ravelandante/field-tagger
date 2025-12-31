use crate::app::{App, AppState};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
    style::{Color, Style},
    Frame,
};

pub fn ui(f: &mut Frame<>, app: &App) {
    if let AppState::Processing = app.state {
        let processing_message = Paragraph::new("Processing... Please wait");
        f.render_widget(processing_message, f.area());
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // progress bar
            Constraint::Length(8), // waveform
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

    // waveform with progress indicator
    let progress_point = (app.progress * app.waveform_data.len() as f64) as usize;
    let waveform_before: Vec<u64> = app.waveform_data.iter().take(progress_point).copied().collect();
    let waveform_after: Vec<u64> = app.waveform_data.iter().skip(progress_point).copied().collect();
    
    let waveform_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((app.progress * 100.0) as u16),
            Constraint::Percentage(((1.0 - app.progress) * 100.0) as u16),
        ])
        .split(chunks[1]);
    
    if !waveform_before.is_empty() {
        let sparkline_played = Sparkline::default()
            .block(Block::default().borders(Borders::ALL).title("Waveform"))
            .data(&waveform_before)
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(sparkline_played, waveform_chunks[0]);
    }
    
    if !waveform_after.is_empty() && waveform_chunks.len() > 1 {
        let sparkline_remaining = Sparkline::default()
            .block(Block::default().borders(Borders::ALL))
            .data(&waveform_after)
            .style(Style::default().fg(Color::Gray));
        f.render_widget(sparkline_remaining, waveform_chunks[1]);
    }

    // input
    let input_title = match app.state {
        AppState::AskingForTags => "Enter Tags",
        AppState::AskingForLocation => "Enter Location",
        _ => "",
    };

    let input_panel = Paragraph::new(app.input.as_str())
        .block(Block::default().borders(Borders::ALL).title(input_title));
    f.render_widget(input_panel, chunks[2]);

    // instructions + info
    let help_text = Paragraph::new("ESC: Quit | Enter: Save & Next | Arrows: Seek | Del: Delete File")
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    f.render_widget(help_text, chunks[3]);
}