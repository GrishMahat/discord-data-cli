use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{
    AppState, SetupStep, is_printable_input, list_browse_entries, setup_prev_step,
    setup_submit_step,
};

pub(crate) fn handle_setup_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.should_quit = true;
        }
        // Tab toggles between input and browse panel (only on path steps)
        KeyCode::Tab
            if matches!(
                app.setup.step,
                SetupStep::ExportPath | SetupStep::ResultsPath
            ) =>
        {
            app.setup.browse_focus = !app.setup.browse_focus;
            if app.setup.browse_focus {
                // Refresh browse entries when switching focus to browse
                app.setup.browse_entries = list_browse_entries(&app.setup.input);
                if app.setup.browse_cursor >= app.setup.browse_entries.len() {
                    app.setup.browse_cursor = 0;
                }
            }
        }
        // When browse panel is focused, handle navigation
        KeyCode::Up if app.setup.browse_focus => {
            if app.setup.browse_cursor > 0 {
                app.setup.browse_cursor -= 1;
            } else if !app.setup.browse_entries.is_empty() {
                app.setup.browse_cursor = app.setup.browse_entries.len() - 1;
            }
        }
        KeyCode::Down if app.setup.browse_focus => {
            if !app.setup.browse_entries.is_empty() {
                app.setup.browse_cursor =
                    (app.setup.browse_cursor + 1) % app.setup.browse_entries.len();
            }
        }
        // Enter in browse panel: select the directory entry
        KeyCode::Enter if app.setup.browse_focus => {
            if let Some(entry) = app.setup.browse_entries.get(app.setup.browse_cursor) {
                let new_path = entry.path.display().to_string();
                app.setup.input = new_path.clone();
                // Refresh browse entries to show contents of selected dir
                app.setup.browse_entries = list_browse_entries(&new_path);
                app.setup.browse_cursor = 0;
            }
        }
        // Backspace
        KeyCode::Backspace => {
            if app.setup.browse_focus {
                // Switch back to input mode on backspace
                app.setup.browse_focus = false;
            } else if app.setup.step == SetupStep::Confirm {
                setup_prev_step(app);
                refresh_browse_state(app);
            } else {
                app.setup.input.pop();
            }
        }
        // Navigate back
        KeyCode::Left | KeyCode::BackTab if !app.setup.browse_focus => {
            setup_prev_step(app);
            refresh_browse_state(app);
        }
        // Navigate forward (Enter, Right, Down when not browsing)
        KeyCode::Enter | KeyCode::Right | KeyCode::Down if !app.setup.browse_focus => {
            if let Err(err) = setup_submit_step(app) {
                app.setup.notice = err.to_string();
            } else {
                refresh_browse_state(app);
            }
        }
        // Ctrl+U to clear
        KeyCode::Char('u') | KeyCode::Char('U')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            if app.setup.step != SetupStep::Confirm && !app.setup.browse_focus {
                app.setup.input.clear();
            }
        }
        // Regular character input (only when not browsing)
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if is_printable_input(c)
                && app.setup.step != SetupStep::Confirm
                && !app.setup.browse_focus
            {
                app.setup.input.push(c);
            }
        }
        _ => {}
    }

    Ok(())
}

/// Refresh browse entries after a step change
fn refresh_browse_state(app: &mut AppState) {
    if matches!(
        app.setup.step,
        SetupStep::ExportPath | SetupStep::ResultsPath
    ) {
        app.setup.browse_entries = list_browse_entries(&app.setup.input);
        app.setup.browse_cursor = 0;
        app.setup.browse_focus = false;
    } else {
        app.setup.browse_entries.clear();
        app.setup.browse_cursor = 0;
        app.setup.browse_focus = false;
    }
}
