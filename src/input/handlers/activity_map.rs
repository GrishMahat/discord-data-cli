use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};

use crate::app::{AppState, Screen};

pub(crate) fn handle_activity_map_key(app: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            app.map_page = app.map_page.saturating_sub(1);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            app.map_page += 1;
        }
        KeyCode::Home | KeyCode::Char('g') => app.map_page = 0,
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        _ => {}
    }
}

pub(crate) fn handle_activity_map_mouse(app: &mut AppState, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => app.map_page = app.map_page.saturating_sub(1),
        MouseEventKind::ScrollDown => app.map_page = app.map_page.saturating_add(1),
        _ => {}
    }
}
