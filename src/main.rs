mod analyzer;
mod app_state;
mod config;
mod downloader;
mod input;
mod support_activity;
mod ui;

use std::{
    env,
    io::{self, IsTerminal},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use app_state::AppState;
use crossterm::{
    cursor,
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode().with_context(|| "failed to enable raw mode".to_owned())?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            cursor::Hide,
            event::EnableMouseCapture
        )
        .with_context(|| "failed to enter alternate screen".to_owned())?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            cursor::Show,
            event::DisableMouseCapture,
            LeaveAlternateScreen
        );
    }
}

fn main() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("Discord Analyzer TTY requires an interactive terminal session.");
    }

    let config_path = env::current_dir()
        .with_context(|| "failed to read current working directory".to_owned())?
        .join("interactive.session.toml");

    let mut app = AppState::new(config_path)?;
    run_tui(&mut app)
}

fn run_tui(app: &mut AppState) -> Result<()> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal =
        Terminal::new(backend).with_context(|| "failed to create terminal".to_owned())?;
    terminal
        .clear()
        .with_context(|| "failed to clear terminal".to_owned())?;

    while !app.should_quit {
        app.animation_tick = app.animation_tick.wrapping_add(1);
        app_state::poll_analysis(app);
        app_state::poll_download(app);

        terminal
            .draw(|frame| ui::draw_ui(frame, app))
            .with_context(|| "failed to draw frame".to_owned())?;

        if event::poll(Duration::from_millis(50)).with_context(|| "event poll failed".to_owned())? {
            match event::read().with_context(|| "event read failed".to_owned())? {
                Event::Key(key) if key.kind == KeyEventKind::Press => input::handle_key(app, key)?,
                Event::Mouse(mouse) => input::handle_mouse(app, mouse)?,
                Event::Paste(text) => input::handle_paste(app, &text),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    Ok(())
}
