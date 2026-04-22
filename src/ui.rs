use crate::app::{App, AppState, InputField, InteractionMode, ReviewOption, TrimSide};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    widgets::{Axis, Block, Borders, Chart, Dataset, Gauge, GraphType, Paragraph, Wrap},
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

    if let AppState::ReviewOutputNaming = app.state {
        let filename_checkbox = if app.auto_compute_filename { "[X]" } else { "[ ]" };
        let originals_checkbox = if app.keep_original_files { "[X]" } else { "[ ]" };
        let filename_suffix = if app.review_selected_option == ReviewOption::AutoComputeFilename {
            "<"
        } else {
            " "
        };
        let originals_suffix = if app.review_selected_option == ReviewOption::KeepOriginalFiles {
            "<"
        } else {
            " "
        };
        let review_panel = Paragraph::new(format!(
            "{} Rename files with computed location and tags? {}\n{} Keep original files? {}\n\nSpace: Toggle selected option | Enter: Start processing | Esc: Quit",
            filename_checkbox,
            filename_suffix,
            originals_checkbox,
            originals_suffix,
        ))
        .block(Block::default().borders(Borders::ALL).title("Output Options"))
        .wrap(Wrap { trim: true });
        f.render_widget(review_panel, f.area());
        return;
    }

    let input_height = 6;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(2),  // file info
            Constraint::Length(3),  // progress bar
            Constraint::Length(12), // waveform
            Constraint::Length(input_height), // input / trim
            Constraint::Min(3),  // controls (adaptive height)
        ])
        .split(f.area());

    // file info
    let file_info = Paragraph::new(app.available_files[app.current_file_index].as_str())
        .style(Style::default().add_modifier(Modifier::BOLD));
    f.render_widget(file_info, chunks[0]);

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
    f.render_widget(playback_bar, chunks[1]);


    // waveform
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
    let file_metadata = &app.metadata[app.current_file_index];
    let playback_marker_data = if app.total_duration.is_zero() {
        None
    } else {
        Some(vec![
            (app.progress.clamp(0.0, 1.0) * data_len, 0.0),
            (app.progress.clamp(0.0, 1.0) * data_len, max_val * 1.1),
        ])
    };
    let from_marker_data = trim_time_to_waveform_x(file_metadata.trim_from, app.total_duration, data_len)
        .map(|x| vec![(x, 0.0), (x, max_val * 1.1)]);
    let to_marker_data = trim_time_to_waveform_x(file_metadata.trim_to, app.total_duration, data_len)
        .map(|x| vec![(x, 0.0), (x, max_val * 1.1)]);
    
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

    if let Some(playback_marker) = &playback_marker_data {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Green))
                .data(playback_marker),
        );
    }

    if let Some(from_marker) = &from_marker_data {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Yellow))
                .data(from_marker),
        );
    }

    if let Some(to_marker) = &to_marker_data {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Magenta))
                .data(to_marker),
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
    
    f.render_widget(waveform_chart, chunks[2]);

    if app.interaction_mode == InteractionMode::Trim {
        let active_side = match app.active_trim_side {
            TrimSide::From => "From",
            TrimSide::To => "To",
        };
        let warning_line = app.trim_warning.clone().unwrap_or_default();
        let trim_panel = Paragraph::new(format!(
            "From: {}\nTo: {}\nActive Side: {}\nPlayback: {}\n{}",
            format_duration(file_metadata.trim_from),
            format_duration(file_metadata.trim_to),
            active_side,
            if app.is_paused { "Paused" } else { "Playing" },
            warning_line
        ))
        .block(Block::default().borders(Borders::ALL).title("Trim"));
        f.render_widget(trim_panel, chunks[3]);
    } else {
        let input_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Length(3)])
            .split(chunks[3]);

        let location_block = if app.active_input_field == InputField::Location {
            Block::default()
                .borders(Borders::ALL)
                .title("Location")
                .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        } else {
            Block::default().borders(Borders::ALL).title("Location")
        };

        let tags_block = if app.active_input_field == InputField::Tags {
            Block::default()
                .borders(Borders::ALL)
                .title("Tags (comma separated)")
                .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        } else {
            Block::default()
                .borders(Borders::ALL)
                .title("Tags (comma separated)")
        };

        let location_panel = Paragraph::new(app.location_input.as_str()).block(location_block);
        let tags_panel = Paragraph::new(app.tags_input.as_str()).block(tags_block);
        f.render_widget(location_panel, input_chunks[0]);
        f.render_widget(tags_panel, input_chunks[1]);
    }

    // instructions
    let controls = if app.interaction_mode == InteractionMode::Trim {
        "ESC: Quit | F6: Tag Mode | Arrows: Seek | Space: Play/Pause | T: Switch Side | Enter: Set Trim | Backspace: Clear Trim"
    } else {
        "ESC: Quit | F6: Trim Mode | Arrows: Seek | Enter: Next File | Ctrl+Enter: Process To Current | Del: Mark for Deletion"
    };
    let help_text = Paragraph::new(controls)
        .block(Block::default().borders(Borders::ALL).title("Controls"))
        .wrap(Wrap { trim: true });
    f.render_widget(help_text, chunks[4]);
}

fn format_duration(duration: Option<std::time::Duration>) -> String {
    match duration {
        Some(value) => format!(
            "{:02}:{:02}.{:03}",
            value.as_secs() / 60,
            value.as_secs() % 60,
            value.subsec_millis()
        ),
        None => "--:--.---".to_string(),
    }
}

fn trim_time_to_waveform_x(
    marker: Option<std::time::Duration>,
    total_duration: std::time::Duration,
    data_len: f64,
) -> Option<f64> {
    let marker = marker?;
    if total_duration.is_zero() {
        return None;
    }

    let ratio = (marker.as_secs_f64() / total_duration.as_secs_f64()).clamp(0.0, 1.0);
    Some(ratio * data_len)
}