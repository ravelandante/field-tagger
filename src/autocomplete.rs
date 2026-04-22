use crate::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn filtered_location_suggestions(app: &App) -> Vec<String> {
    let query = app.location_input.trim().to_lowercase();
    let mut filtered: Vec<String> = if query.is_empty() {
        app.location_suggestions.clone()
    } else {
        app.location_suggestions
            .iter()
            .filter(|value| value.to_lowercase().contains(&query))
            .cloned()
            .collect()
    };
    filtered.truncate(5);
    filtered
}

pub fn move_selection(app: &mut App, delta: i8) -> bool {
    let suggestions = filtered_location_suggestions(app);
    if suggestions.is_empty() {
        return false;
    }
    let max = suggestions.len().saturating_sub(1);
    if delta < 0 {
        app.location_dropdown_index = app.location_dropdown_index.saturating_sub(1);
    } else if delta > 0 {
        app.location_dropdown_index = (app.location_dropdown_index + 1).min(max);
    }
    true
}

pub fn apply_selection(app: &mut App) -> bool {
    let suggestions = filtered_location_suggestions(app);
    if suggestions.is_empty() {
        return false;
    }
    let index = app.location_dropdown_index.min(suggestions.len().saturating_sub(1));
    let selected = &suggestions[index];
    if app.location_input == *selected {
        return false;
    }
    app.location_input = selected.clone();
    true
}

pub fn reset_selection(app: &mut App) {
    app.location_dropdown_index = 0;
}

pub fn render_location_autocomplete(
    f: &mut Frame<>,
    area: Rect,
    app: &App,
    is_focused: bool,
) {
    let block = if is_focused {
        Block::default()
            .borders(Borders::ALL)
            .title("Location")
            .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
    } else {
        Block::default().borders(Borders::ALL).title("Location")
    };
    let inner = block.inner(area);
    f.render_widget(block, area);

    f.render_widget(Paragraph::new(app.location_input.as_str()), inner);
}

pub fn render_location_dropdown_overlay(
    f: &mut Frame<>,
    app: &App,
    anchor_area: Rect,
    is_focused: bool,
) {
    if !is_focused {
        return;
    }

    let suggestions = filtered_location_suggestions(app);
    if suggestions.is_empty() {
        return;
    }

    let show_count = suggestions.len().min(4);
    let popup_height = show_count as u16 + 1;
    let frame = f.area();
    let preferred_below_y = anchor_area.y.saturating_add(anchor_area.height);
    let render_below = preferred_below_y.saturating_add(popup_height) <= frame.y.saturating_add(frame.height);

    let popup_y = if render_below {
        // overlap one row so dropdown and input share a border
        preferred_below_y.saturating_sub(1)
    } else {
        // overlap one row when rendering upward as well
        anchor_area.y.saturating_sub(popup_height).saturating_add(1)
    };

    let popup = Rect {
        x: anchor_area.x,
        y: popup_y,
        width: anchor_area.width,
        height: popup_height.min(frame.height),
    };

    let items = suggestions
        .into_iter()
        .take(show_count)
        .map(ListItem::new)
        .collect::<Vec<_>>();
    let mut list_state = ListState::default();
    list_state.select(Some(app.location_dropdown_index.min(items.len().saturating_sub(1))));
    let dropdown_borders = if render_below {
        Borders::LEFT | Borders::RIGHT | Borders::BOTTOM
    } else {
        Borders::LEFT | Borders::RIGHT | Borders::TOP
    };
    let list = List::new(items)
        .block(Block::default().borders(dropdown_borders))
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    f.render_widget(Clear, popup);
    f.render_stateful_widget(list, popup, &mut list_state);
}
