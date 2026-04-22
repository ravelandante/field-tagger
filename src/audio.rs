use crate::app::{App, InteractionMode, TrimSide};
use hound::WavReader;
use rodio::{Decoder, Sink, Source};
use std::{fs::File, io::BufReader, time::{Duration, Instant}};

pub fn toggle_interaction_mode(sink: &Sink, app: &mut App) {
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

pub fn toggle_pause(sink: &Sink, app: &mut App) {
    if app.is_paused {
        sink.play();
        app.is_paused = false;
    } else {
        sink.pause();
        app.is_paused = true;
    }
}

pub fn seek_with_acceleration(
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

pub fn reset_seek_acceleration(app: &mut App) {
    app.seek_repeat_count = 0;
    app.last_seek_direction = 0;
    app.last_seek_at = None;
}

pub fn set_active_trim_marker(app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
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

pub fn clear_active_trim_marker(app: &mut App) {
    let metadata = &mut app.metadata[app.current_file_index];
    app.trim_warning = None;

    match app.active_trim_side {
        TrimSide::From => metadata.trim_from = None,
        TrimSide::To => metadata.trim_to = None,
    }
}

pub fn play_next_file(sink: &Sink, app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    sink.clear();
    let next_file = File::open(&*app.available_files[app.current_file_index])?;
    let next_source = Decoder::new(BufReader::new(next_file))?;
    app.total_duration = next_source.total_duration().unwrap_or(Duration::from_secs(0));
    app.waveform_data = extract_waveform(&app.available_files[app.current_file_index], 200)?;
    app.current_duration = Duration::from_secs(0);
    app.interaction_mode = InteractionMode::Normal;
    app.active_trim_side = TrimSide::From;
    app.active_input_field = crate::app::InputField::Location;
    app.trim_warning = None;
    app.is_paused = false;
    reset_seek_acceleration(app);
    sink.append(next_source);
    sink.play();
    Ok(())
}

pub fn extract_waveform(file_path: &str, num_points: usize) -> Result<Vec<u64>, Box<dyn std::error::Error + 'static>> {
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

        let chunk = &samples[start..end];
        let sum_squares: f64 = chunk.iter().map(|&s| (s as f64).powi(2)).sum();
        let rms = (sum_squares / chunk.len() as f64).sqrt();
        let normalized = (rms / i16::MAX as f64 * 100.0) as u64;
        waveform.push(normalized);
    }

    Ok(waveform)
}

pub fn enforce_playback_bounds(sink: &Sink, app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
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
