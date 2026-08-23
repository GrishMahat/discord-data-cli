mod analyzer;
mod app;
mod compare;
mod config;
mod data;
mod downloader;
mod input;
mod insights;
mod ui;

// Somewhere in the void, a Discord server stores 5 years of your embarrassing messages.
// This program is here to remind you of all your life choices.

use std::{
    env,
    fs::{self, File},
    io::{self, IsTerminal, Write},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use app::AppState;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

// When you ctrl+C, the terminal goes "ow" and puts its clothes back on.
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
    // Headless analysis mode: `discord-analyzer --analyze`
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--analyze") {
        return run_headless_analysis();
    }

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

/// Non-interactive analysis pass; prints progress and an insight digest.
fn run_headless_analysis() -> Result<()> {
    let config_path = env::current_dir()
        .with_context(|| "failed to read current working directory".to_owned())?
        .join("interactive.session.toml");

    let mut app = AppState::new(config_path)?;
    if !app.config.package_path(&app.config_path, &app.id).exists() {
        bail!(
            "No export configured. Run the TUI once to set up interactive.session.toml."
        );
    }

    println!("Headless analysis starting…");
    app::start_analysis(&mut app);
    let mut last_label = String::new();
    while app.analysis_running {
        app::poll_analysis(&mut app);
        if app.status != last_label {
            last_label = app.status.clone();
            println!("  {}", app.status);
        }
        thread::sleep(Duration::from_millis(150));
    }
    if let Some(err) = &app.error {
        bail!("Analysis failed: {err}");
    }

    let Some(data) = &app.last_data else {
        bail!("Analysis produced no data.");
    };
    let ins = &data.insights;
    println!("\nAnalysis complete:");
    println!(
        "  messages: {} across {} channels",
        data.messages.total, data.messages.channels
    );
    println!(
        "  billing: {} payments · entitlements {} · coin tx {}",
        ins.billing.payments_total, ins.billing.entitlements_count, ins.billing.coin_transactions
    );
    println!(
        "  privacy: mfa={} verified={} payment_sources={}",
        ins.privacy.mfa_enabled, ins.privacy.email_verified, ins.privacy.payment_sources
    );
    println!(
        "  social: {} DM channels ({} msgs) · {} groups ({} msgs)",
        ins.social.dm_channels,
        ins.social.dm_messages,
        ins.social.group_dm_channels,
        ins.social.group_dm_messages
    );
    println!(
        "  links: {} links in {} msgs · questions {} · top domain {:?}",
        ins.links.total_links,
        ins.links.messages_with_links,
        ins.links.question_messages,
        ins.links.top_domains.first().map(|(d, c)| format!("{d} x{c}")),
    );
    println!(
        "  voice: {} conns · {} disc · ping {:.0}ms · MOS {:.2} · {:.1}h connected",
        ins.voice.connections,
        ins.voice.disconnects,
        ins.voice.avg_ping_ms,
        ins.voice.avg_mos,
        ins.voice.connected_minutes / 60.0
    );
    println!(
        "  sessions: {} active days · streak {} · first {:?} · last {:?} ({} duplicate events skipped)",
        ins.sessions.active_days,
        ins.sessions.longest_daily_streak,
        ins.sessions.first_active_day,
        ins.sessions.last_active_day,
        data.insights_cache
            .as_ref()
            .map(|c| c.aggregate.duplicates_skipped)
            .unwrap_or(0)
    );
    Ok(())
}

fn log_msg(msg: &str) {
    if let Ok(file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/discord-cli.log")
    {
        let mut file: File = file;
        let line = format!(
            "[{}] {}\n",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
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
            log_msg(&format!(
                "Heartbeat, screen: {:?}, tick: {}",
                app.screen, app.animation_tick
            ));
            last_tick = tick_start;
        }

        app.animation_tick = app.animation_tick.wrapping_add(1);
        app::poll_analysis(app);
        app::poll_download(app);
        app::poll_support_activity(app);
        app::poll_gallery(app);
        app::poll_channels(app);
        app::poll_channel_preview(app);
        app::poll_search(app);

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
                    if key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
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
