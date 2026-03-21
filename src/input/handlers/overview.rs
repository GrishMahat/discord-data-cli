use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

use crate::analyzer;
use crate::app::{AppState, Screen};

pub(crate) fn handle_overview_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            let results_dir = app.config.results_path(&app.config_path, &app.id);
            app.last_data = analyzer::read_data(&results_dir)?;
            app.status = "Overview refreshed from data.json".to_owned();
            app.error = None;
        }
        _ => {}
    }
    Ok(())
}
