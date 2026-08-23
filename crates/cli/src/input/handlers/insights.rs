use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};

use crate::app::{AppState, Screen};

pub(crate) fn handle_insights_key(app: &mut AppState, key: KeyEvent) {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.insights_scroll = app.insights_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            app.insights_scroll += 1;
        }
        KeyCode::PageUp => app.insights_scroll = app.insights_scroll.saturating_sub(15),
        KeyCode::PageDown => app.insights_scroll += 15,
        KeyCode::Home => app.insights_scroll = 0,
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
            app.insights_scroll = 0;
        }
        _ => {}
    }
}

pub(crate) fn handle_insights_mouse(app: &mut AppState, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => app.insights_scroll = app.insights_scroll.saturating_sub(3),
        MouseEventKind::ScrollDown => app.insights_scroll += 3,
        _ => {}
    }
}
