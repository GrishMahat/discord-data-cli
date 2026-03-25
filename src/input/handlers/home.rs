use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, MouseEvent};
use ratatui::layout::Rect;

use crate::app::{AppState, HOME_MENU_ITEMS, execute_home_selection, home_item_disabled_reason};
pub(crate) fn handle_home_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    let disabled_reason = |idx: usize| home_item_disabled_reason(app, idx);

    match key.code {
        KeyCode::Up
        | KeyCode::Char('w')
        | KeyCode::Char('W')
        | KeyCode::Char('k')
        | KeyCode::Char('K') => {
            app.home_cursor = app.home_cursor.saturating_sub(1);
        }
        KeyCode::Down
        | KeyCode::Char('s')
        | KeyCode::Char('S')
        | KeyCode::Char('j')
        | KeyCode::Char('J') => {
            if app.home_cursor + 1 < HOME_MENU_ITEMS.len() {
                app.home_cursor += 1;
            }
        }
        KeyCode::Enter => {
            execute_home_selection(app)?;
        }
        KeyCode::Char(c) if ('1'..='9').contains(&c) => {
            let idx = (c as u8 - b'1') as usize;
            if idx < HOME_MENU_ITEMS.len() {
                if let Some(reason) = disabled_reason(idx) {
                    app.home_cursor = idx; // move cursor so user can see disabled item
                    app.status = reason;
                    app.error = None;
                } else {
                    app.home_cursor = idx;
                }
            }
        }
        _ => {}
    }

    Ok(())
}

pub(crate) fn handle_home_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) -> Result<()> {
    let _ = (app, mouse, area);
    Ok(())
}
