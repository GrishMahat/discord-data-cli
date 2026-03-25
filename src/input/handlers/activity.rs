use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::{
    filtered_activity_events, is_printable_input, open_selected_activity_event,
    refresh_support_activity_data, ActivityFilterField, AppState, Screen,
};
use crate::input::rect_contains;

pub(crate) fn handle_activity_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    if let Some(field) = app.activity_filter_edit {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                app.activity_filter_edit = None;
            }
            KeyCode::Backspace => {
                active_filter_mut(app, field).pop();
                app.activity_cursor = 0;
            }
            KeyCode::Char('u') | KeyCode::Char('U')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                active_filter_mut(app, field).clear();
                app.activity_cursor = 0;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if is_printable_input(c) {
                    active_filter_mut(app, field).push(c);
                    app.activity_cursor = 0;
                }
            }
            _ => {}
        }
        return Ok(());
    }

    let event_count = filtered_activity_events(app).len();
    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.activity_cursor = app.activity_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            if app.activity_cursor + 1 < event_count {
                app.activity_cursor += 1;
            }
        }
        KeyCode::PageUp => {
            app.activity_cursor = app.activity_cursor.saturating_sub(15);
        }
        KeyCode::PageDown => {
            app.activity_cursor = (app.activity_cursor + 15).min(event_count.saturating_sub(1));
        }
        KeyCode::Char('/') => app.activity_filter_edit = Some(ActivityFilterField::Query),
        KeyCode::Char('t') | KeyCode::Char('T') => {
            app.activity_filter_edit = Some(ActivityFilterField::EventType)
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.activity_filter_edit = Some(ActivityFilterField::SourceFile)
        }
        KeyCode::Char('[') => app.activity_filter_edit = Some(ActivityFilterField::FromDate),
        KeyCode::Char(']') => app.activity_filter_edit = Some(ActivityFilterField::ToDate),
        KeyCode::Char('o') | KeyCode::Char('O') => {
            app.activity_sort = app.activity_sort.next();
            app.activity_cursor = 0;
        }
        KeyCode::Enter => open_selected_activity_event(app),
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.activity_filters.query.clear();
            app.activity_filters.event_type.clear();
            app.activity_filters.source_file.clear();
            app.activity_filters.from_date.clear();
            app.activity_filters.to_date.clear();
            app.activity_cursor = 0;
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            refresh_support_activity_data(app)?;
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn handle_activity_detail_key(app: &mut AppState, key: KeyEvent) {
    let filtered = filtered_activity_events(app);
    let max_scroll = filtered
        .get(app.activity_cursor.min(filtered.len().saturating_sub(1)))
        .map(|event| event.detail.lines().count().saturating_sub(1))
        .unwrap_or(0);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.activity_detail_scroll = app.activity_detail_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            app.activity_detail_scroll = (app.activity_detail_scroll + 1).min(max_scroll);
        }
        KeyCode::PageUp => {
            app.activity_detail_scroll = app.activity_detail_scroll.saturating_sub(15);
        }
        KeyCode::PageDown => {
            app.activity_detail_scroll = (app.activity_detail_scroll + 15).min(max_scroll);
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Activity;
        }
        _ => {}
    }
}

pub(crate) fn handle_activity_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    if area.width <= 2 || area.height <= 2 {
        return;
    }

    let inner = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(rows[1]);
    let list_area = cols[0];
    if !rect_contains(list_area, mouse.column, mouse.row) {
        return;
    }

    let filtered = filtered_activity_events(app);
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if filtered.is_empty() || list_area.height <= 2 {
                return;
            }
            let visible = list_area.height.saturating_sub(2) as usize;
            let page_size = visible.max(1);
            let cursor = app.activity_cursor.min(filtered.len().saturating_sub(1));
            let start = cursor
                .saturating_sub(page_size / 2)
                .min(filtered.len().saturating_sub(page_size));
            let end = (start + page_size).min(filtered.len());
            let row = mouse.row.saturating_sub(list_area.y + 1) as usize;
            if row < end.saturating_sub(start) {
                app.activity_cursor = start + row;
                open_selected_activity_event(app);
            }
        }
        MouseEventKind::ScrollUp => {
            app.activity_cursor = app.activity_cursor.saturating_sub(1);
        }
        MouseEventKind::ScrollDown => {
            let max_idx = filtered.len().saturating_sub(1);
            app.activity_cursor = (app.activity_cursor + 1).min(max_idx);
        }
        _ => {}
    }
}

pub(crate) fn handle_activity_detail_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    if !rect_contains(area, mouse.column, mouse.row) {
        return;
    }
    let filtered = filtered_activity_events(app);
    let max_scroll = filtered
        .get(app.activity_cursor.min(filtered.len().saturating_sub(1)))
        .map(|event| event.detail.lines().count().saturating_sub(1))
        .unwrap_or(0);
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.activity_detail_scroll = app.activity_detail_scroll.saturating_sub(3);
        }
        MouseEventKind::ScrollDown => {
            app.activity_detail_scroll = (app.activity_detail_scroll + 3).min(max_scroll);
        }
        _ => {}
    }
}

fn active_filter_mut(app: &mut AppState, field: ActivityFilterField) -> &mut String {
    match field {
        ActivityFilterField::Query => &mut app.activity_filters.query,
        ActivityFilterField::EventType => &mut app.activity_filters.event_type,
        ActivityFilterField::SourceFile => &mut app.activity_filters.source_file,
        ActivityFilterField::FromDate => &mut app.activity_filters.from_date,
        ActivityFilterField::ToDate => &mut app.activity_filters.to_date,
    }
}
