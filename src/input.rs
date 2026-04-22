use crate::app::{App, InputField, InteractionMode, ReviewOption};
use crate::autocomplete;
use crate::audio::{
    clear_active_trim_marker, reset_seek_acceleration, seek_with_acceleration, set_active_trim_marker,
    toggle_interaction_mode, toggle_pause,
};
use crate::processing::{delete_file, process_all_files, save_current_file_inputs};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{backend::CrosstermBackend, Terminal};
use rodio::Sink;
use std::io;

pub fn handle_key_event(
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

    if matches!(app.state, crate::app::AppState::ReviewOutputNaming) {
        match key.code {
            KeyCode::Up | KeyCode::Down | KeyCode::Tab => {
                app.review_selected_option = match app.review_selected_option {
                    ReviewOption::AutoComputeFilename => ReviewOption::KeepOriginalFiles,
                    ReviewOption::KeepOriginalFiles => ReviewOption::AutoComputeFilename,
                };
            }
            KeyCode::Char(' ') => match app.review_selected_option {
                ReviewOption::AutoComputeFilename => app.auto_compute_filename = !app.auto_compute_filename,
                ReviewOption::KeepOriginalFiles => app.keep_original_files = !app.keep_original_files,
            },
            KeyCode::Enter => process_all_files(sink, app, terminal)?,
            _ => {}
        }
        return Ok(());
    }

    if key.code == KeyCode::F(5) {
        reset_seek_acceleration(app);
        handle_process_to_current_key(sink, app);
        return Ok(());
    }

    if matches!(key.code, KeyCode::F(6)) {
        toggle_interaction_mode(sink, app);
        return Ok(());
    }

    match key.code {
        KeyCode::Enter => {
            reset_seek_acceleration(app);
            if app.active_input_field == InputField::Location && autocomplete::apply_selection(app) {
                app.active_input_field = InputField::Tags;
                return Ok(());
            }
            handle_enter_key(sink, app)?;
        }
        KeyCode::Delete => {
            reset_seek_acceleration(app);
            delete_file(sink, app)?;
        }
        KeyCode::Right => seek_with_acceleration(sink, app, 1)?,
        KeyCode::Left => seek_with_acceleration(sink, app, -1)?,
        KeyCode::Up if app.interaction_mode == InteractionMode::Normal => {
            reset_seek_acceleration(app);
            if app.active_input_field == InputField::Location && autocomplete::move_selection(app, -1) {
                return Ok(());
            }
            app.active_input_field = InputField::Location;
        }
        KeyCode::Down if app.interaction_mode == InteractionMode::Normal => {
            reset_seek_acceleration(app);
            if app.active_input_field == InputField::Location && autocomplete::move_selection(app, 1) {
                return Ok(());
            }
            app.active_input_field = InputField::Tags;
        }
        KeyCode::Char(' ') if app.interaction_mode == InteractionMode::Trim => {
            reset_seek_acceleration(app);
            toggle_pause(sink, app)
        }
        KeyCode::Char('t') | KeyCode::Char('T') if app.interaction_mode == InteractionMode::Trim => {
            reset_seek_acceleration(app);
            app.active_trim_side = match app.active_trim_side {
                crate::app::TrimSide::From => crate::app::TrimSide::To,
                crate::app::TrimSide::To => crate::app::TrimSide::From,
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
            if app.active_input_field == InputField::Location {
                autocomplete::reset_selection(app);
            }
        }
        KeyCode::Char(c) if app.interaction_mode == InteractionMode::Normal => {
            reset_seek_acceleration(app);
            active_input_buffer_mut(app).push(c);
            if app.active_input_field == InputField::Location {
                autocomplete::reset_selection(app);
            }
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

fn handle_enter_key(sink: &Sink, app: &mut App) -> Result<(), Box<dyn std::error::Error + 'static>> {
    if app.interaction_mode == InteractionMode::Trim {
        return set_active_trim_marker(app);
    }

    save_current_file_inputs(app);

    if app.current_file_index + 1 >= app.available_files.len() {
        sink.stop();
        app.processing_end_index = Some(app.current_file_index);
        app.state = crate::app::AppState::ReviewOutputNaming;
    } else {
        app.current_file_index += 1;
        crate::audio::play_next_file(sink, app)?;
        app.state = crate::app::AppState::EditingMetadata;
    }
    app.location_input.clear();
    app.tags_input.clear();
    app.active_input_field = InputField::Location;

    Ok(())
}

fn handle_process_to_current_key(sink: &Sink, app: &mut App) {
    save_current_file_inputs(app);
    sink.stop();
    app.processing_end_index = Some(app.current_file_index);
    app.state = crate::app::AppState::ReviewOutputNaming;
    app.location_input.clear();
    app.tags_input.clear();
    app.active_input_field = InputField::Location;
}

fn active_input_buffer_mut(app: &mut App) -> &mut String {
    match app.active_input_field {
        InputField::Location => &mut app.location_input,
        InputField::Tags => &mut app.tags_input,
    }
}
