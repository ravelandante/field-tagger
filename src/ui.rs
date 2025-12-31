use crate::app::{App, AppState};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Block, Borders, Gauge, Paragraph, Chart, Dataset, Axis, GraphType},
    style::{Color, Style, Modifier},
    symbols,
    text::Span,
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
            Constraint::Length(3),  // progress bar
            Constraint::Length(12), // waveform
            Constraint::Length(3),  // input
            Constraint::Min(0),     // instructions + info
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

    let progress_point = (app.progress * app.waveform_data.len() as f64) as usize;
    
    let played_data: Vec<(f64, f64)> = app.waveform_data
        .iter()
        .take(progress_point)
        .enumerate()
        .map(|(i, &val)| (i as f64, val as f64))
        .collect();
    
    let unplayed_data: Vec<(f64, f64)> = app.waveform_data
        .iter()
        .enumerate()
        .skip(progress_point)
        .map(|(i, &val)| (i as f64, val as f64))
        .collect();
    
    let max_val = app.waveform_data.iter().max().copied().unwrap_or(1) as f64;
    let data_len = app.waveform_data.len() as f64;
    
    let mut datasets = vec![];
    
    if !played_data.is_empty() {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Cyan))
                .data(&played_data)
        );
    }
    
    if !unplayed_data.is_empty() {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Gray))
                .data(&unplayed_data)
        );
    }
    
    let waveform_chart = Chart::new(datasets)
        .block(Block::default().borders(Borders::ALL).title(Span::styled(
            "Waveform",
            Style::default().add_modifier(Modifier::BOLD)
        )))
        .x_axis(
            Axis::default()
                .bounds([0.0, data_len])
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, max_val * 1.1])
        );
    
    f.render_widget(waveform_chart, chunks[1]);

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