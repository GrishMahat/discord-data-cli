mod analyzer;
mod app;
mod config;
mod data;
mod downloader;
mod input;
mod ui;

use std::{
    env,
    fs::{self, File},
    io::{self, IsTerminal, Write},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use app::AppState;
use crossterm::{
    cursor,
    event::{self, Event, KeyEventKind, KeyCode, KeyModifiers},
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
    
    // Debug logging
    let _ = fs::remove_file("/tmp/discord-cli.log");
    log_msg("App started");

    run_tui(&mut app)
}

fn log_msg(msg: &str) {
    if let Ok(file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/discord-cli.log")
    {
        let mut file: File = file;
        let line = format!("[{}] {}\n", 
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(), 
            msg
        );
        let _ = file.write_all(line.as_bytes());
    }
}

fn run_tui(app: &mut AppState) -> Result<()> {
    let _guard = TerminalGuard::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal =
        Terminal::new(backend).with_context(|| "failed to create terminal".to_owned())?;
    terminal
        .clear()
        .with_context(|| "failed to clear terminal".to_owned())?;

    let mut last_tick = Instant::now();
    while !app.should_quit {
        let tick_start = Instant::now();
        if tick_start.duration_since(last_tick) > Duration::from_millis(1000) {
            log_msg(&format!("Heartbeat, screen: {:?}, tick: {}", app.screen, app.animation_tick));
            last_tick = tick_start;
        }

        app.animation_tick = app.animation_tick.wrapping_add(1);
        app::poll_analysis(app);
        app::poll_download(app);
        app::poll_support_activity(app);
        app::poll_gallery(app);

        terminal
            .draw(|frame| ui::draw_ui(frame, app))
            .with_context(|| "failed to draw frame".to_owned())?;

        let poll_duration = Duration::from_millis(50);
        if event::poll(poll_duration).with_context(|| "event poll failed".to_owned())? {
            let ev = event::read().with_context(|| "event read failed".to_owned())?;
            log_msg(&format!("Input: {:?}", ev));
            
            match ev {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    // Universal Ctrl+C
                    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                        log_msg("Ctrl+C detected in main loop");
                        app.should_quit = true;
                    } else {
                        input::handle_key(app, key)?;
                    }
                }
                Event::Mouse(mouse) => input::handle_mouse(app, mouse)?,
                Event::Paste(text) => input::handle_paste(app, &text),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        
        let loop_elapsed = tick_start.elapsed();
        if loop_elapsed > Duration::from_millis(200) {
            log_msg(&format!("SLOW LOOP: {:?}", loop_elapsed));
        }
    }

    Ok(())
}
