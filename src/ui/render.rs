use std::time::Duration;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Alignment, Color, Modifier, Position, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    analyzer,
    app::{
        ActivityFilterField, AppState, ChannelFilter, ChannelKind, HOME_MENU_ITEMS, Screen,
        SetupStep, filtered_activity_events, filtered_channels, filtered_gallery_files, fmt_num,
        format_duration, home_item_disabled_reason, key_help, ratio, screen_disabled_reason,
        top_counts,
    },
    data::{utils::truncate_text, SupportTicketView},
};

pub(crate) fn draw_ui(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    if app.screen == Screen::Setup {
        draw_setup(frame, app);
        return;
    }

    if app.screen == Screen::Analyzing {
        draw_analyzing(frame, app);
        return;
    }

    if app.screen == Screen::Downloading {
        draw_downloading(frame, app);
        return;
    }

    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);
    draw_tabs(frame, app, chunks[1]);

    match app.screen {
        Screen::Home => draw_home(frame, app, chunks[2]),
        Screen::Overview => draw_overview(frame, app, chunks[2]),
        Screen::SupportActivity => draw_support_activity(frame, app, chunks[2]),
        Screen::SupportTicketDetail => draw_support_ticket_detail(frame, app, chunks[2]),
        Screen::Activity => draw_activity(frame, app, chunks[2]),
        Screen::ActivityDetail => draw_activity_detail(frame, app, chunks[2]),
        Screen::ChannelList => draw_channels(frame, app, chunks[2]),
        Screen::MessageView => draw_message_view(frame, app, chunks[2]),
        Screen::Gallery => draw_gallery(frame, app, chunks[2]),
        Screen::Settings => draw_settings(frame, app, chunks[2]),
        _ => {}
    }

    draw_statusbar(frame, app, chunks[3]);
}

fn draw_header(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Discord Data Analyzer ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split header into left (user/path) and right (quick stats)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(40)])
        .split(inner);

    let user_str = if let Some(data) = &app.last_data {
        format!(
            "  {}  ({})",
            data.account.username.as_deref().unwrap_or("unknown"),
            data.account.user_id.as_deref().unwrap_or("?")
        )
    } else {
        "  Not analyzed yet".to_owned()
    };

    let status_color = if app.error.is_some() {
        Color::Red
    } else if app.status.contains("complete")
        || app.status.contains("Ready")
        || app.status.contains("loaded")
    {
        Color::Green
    } else {
        Color::Yellow
    };

    let left_lines = vec![
        Line::styled(
            user_str,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            format!("  {}", app.error.as_deref().unwrap_or(app.status.as_str())),
            Style::default().fg(status_color),
        ),
    ];
    frame.render_widget(Paragraph::new(left_lines), cols[0]);

    // Quick stats on the right
    if let Some(data) = &app.last_data {
        let right_lines = vec![
            Line::from(vec![
                ratatui::text::Span::styled("msgs ", Style::default().fg(Color::Gray)),
                ratatui::text::Span::styled(
                    format!("{}", data.messages.total),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                ratatui::text::Span::raw("  "),
                ratatui::text::Span::styled("servers ", Style::default().fg(Color::Gray)),
                ratatui::text::Span::styled(
                    format!("{}", data.servers.count),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                ratatui::text::Span::styled("channels ", Style::default().fg(Color::Gray)),
                ratatui::text::Span::styled(
                    format!("{}", data.messages.channels),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                ratatui::text::Span::raw("  "),
                ratatui::text::Span::styled("avg len ", Style::default().fg(Color::Gray)),
                ratatui::text::Span::styled(
                    format!("{:.0}ch", data.messages.content.avg_length_chars),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        ];
        frame.render_widget(
            Paragraph::new(right_lines).alignment(Alignment::Right),
            cols[1],
        );
    }
}

fn draw_tabs(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let screens = [
        (Screen::Home, "Home"),
        (Screen::Overview, "Overview"),
        (Screen::SupportActivity, "Support"),
        (Screen::Activity, "Activity"),
        (Screen::ChannelList, "Channels"),
        (Screen::Gallery, "Gallery"),
        (Screen::Settings, "Settings"),
    ];

    let mut spans = vec![ratatui::text::Span::raw(" ")];
    let tab_screen = tab_group_screen(app.screen);
    for (screen, label) in &screens {
        let active = tab_screen == *screen;
        let disabled = screen_disabled_reason(app, *screen).is_some();
        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if disabled {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(ratatui::text::Span::styled(format!(" {label} "), style));
        spans.push(ratatui::text::Span::raw("  "));
    }

    let tabs = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(tabs, area);
}

fn tab_group_screen(screen: Screen) -> Screen {
    match screen {
        Screen::SupportTicketDetail => Screen::SupportActivity,
        Screen::ActivityDetail => Screen::Activity,
        Screen::MessageView => Screen::ChannelList,
        other => other,
    }
}

fn draw_statusbar(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let help = key_help(app.screen);
    let mut spans = vec![
        ratatui::text::Span::styled(" ? ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        ratatui::text::Span::raw(" "),
    ];

    let mut current = help;
    while let Some(start) = current.find('[') {
        if let Some(end_offset) = current[start..].find(']') {
            let end = start + end_offset;
            // Text before the bracketed key
            if start > 0 {
                spans.push(ratatui::text::Span::styled(
                    &current[..start],
                    Style::default().fg(Color::DarkGray),
                ));
            }
            // The bracketed key itself, highlighted
            spans.push(ratatui::text::Span::styled(
                &current[start..=end],
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
            current = &current[end + 1..];
        } else {
            break;
        }
    }
    // Remaining text after the last bracketed key
    if !current.is_empty() {
        spans.push(ratatui::text::Span::styled(
            current,
            Style::default().fg(Color::DarkGray),
        ));
    }

    let bar = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(bar, area);
}

fn draw_analyzing(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );
    let card = centered_rect(64, 50, area);
    frame.render_widget(Clear, card);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Analyzing Discord Export  ")
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);

    let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner = spinner_frames[((app.animation_tick / 2) % spinner_frames.len() as u64) as usize];
    let elapsed = app
        .analysis_started_at
        .map(|s| s.elapsed())
        .unwrap_or_default();

    let text_lines = vec![
        Line::from(""),
        Line::styled(
            format!(" {spinner} Analyzing your Discord data..."),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(
            format!("    {}", app.status),
            Style::default().fg(Color::Gray),
        ),
    ];
    frame.render_widget(
        Paragraph::new(text_lines).wrap(Wrap { trim: true }),
        sections[0],
    );

    let pct = app.analysis_progress * 100.0;
    let label = format!("  {pct:>5.1}%  elapsed: {}", format_duration(elapsed));
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(app.analysis_progress as f64)
        .label(label);
    frame.render_widget(gauge, sections[1]);

    let eta_line = if app.analysis_progress > 0.02 {
        let rate = app.analysis_progress as f64 / elapsed.as_secs_f64().max(0.001);
        let remaining = ((1.0 - app.analysis_progress as f64) / rate) as u64;
        Line::styled(
            format!("  ETA ~{}", format_duration(Duration::from_secs(remaining))),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        Line::styled("  Calculating ETA...", Style::default().fg(Color::DarkGray))
    };
    frame.render_widget(Paragraph::new(eta_line), sections[2]);
}

fn draw_downloading(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );
    let card = centered_rect(64, 50, area);
    frame.render_widget(Clear, card);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Downloading Attachments  ")
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(3)])
        .split(inner);

    let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner = spinner_frames[((app.animation_tick / 2) % spinner_frames.len() as u64) as usize];

    let text_lines = vec![
        Line::from(""),
        Line::styled(
            format!(" {spinner} Downloading media and attachments..."),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(
            format!("    {}", app.status),
            Style::default().fg(Color::Gray),
        ),
    ];
    frame.render_widget(
        Paragraph::new(text_lines).wrap(Wrap { trim: true }),
        sections[0],
    );

    let pct = app.download_progress * 100.0;
    let label = format!("  {pct:>5.1}%");
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(app.download_progress as f64)
        .label(label);
    frame.render_widget(gauge, sections[1]);
}

fn draw_setup(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );

    let card = centered_rect(72, 80, area);
    frame.render_widget(Clear, card);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Setup  ")
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Length(1), // spacer
            Constraint::Length(4), // step progress
            Constraint::Length(4), // instruction
            Constraint::Length(3), // preview
            Constraint::Length(3), // input
            Constraint::Min(2),    // status
        ])
        .split(inner);

    // Title
    let title = Paragraph::new(Line::styled(
        "Discord Data Analyzer",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, sections[0]);

    // Step dots
    let steps = ["Export Path", "Results Dir", "Profile ID", "Confirm"];
    let current_step = match app.setup.step {
        SetupStep::ExportPath => 0,
        SetupStep::ResultsPath => 1,
        SetupStep::ProfileId => 2,
        SetupStep::Confirm => 3,
    };
    let mut dot_spans = Vec::new();
    for (i, name) in steps.iter().enumerate() {
        let (dot, style) = if i < current_step {
            ("●", Style::default().fg(Color::Green))
        } else if i == current_step {
            (
                "●",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("○", Style::default().fg(Color::DarkGray))
        };
        dot_spans.push(ratatui::text::Span::styled(format!("{dot} {name}"), style));
        if i < steps.len() - 1 {
            dot_spans.push(ratatui::text::Span::styled(
                "  ──  ",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(dot_spans)).alignment(Alignment::Center),
        sections[2],
    );

    // Instruction
    let instruction = match app.setup.step {
        SetupStep::ExportPath => {
            "Paste the full path to your extracted Discord data folder.\nThe directory must already exist."
        }
        SetupStep::ResultsPath => {
            "Where should results be saved?\nPress Enter to accept the default (inside your export folder)."
        }
        SetupStep::ProfileId => {
            "Optional: enter a profile ID if you have multiple exports.\nLeave empty and press Enter to skip."
        }
        SetupStep::Confirm => {
            "Everything looks good.\nPress Enter to continue, or Left/Up to go back and edit."
        }
    };
    frame.render_widget(
        Paragraph::new(instruction)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true }),
        sections[3],
    );

    // Preview of current values
    let preview = Paragraph::new(vec![
        Line::from(vec![
            ratatui::text::Span::styled("Export:  ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(&app.setup.export_path, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            ratatui::text::Span::styled("Results: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(&app.setup.results_path, Style::default().fg(Color::White)),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(preview, sections[4]);

    // Input box
    if app.setup.step == SetupStep::Confirm {
        let confirm = Paragraph::new(Line::styled(
            "  Press Enter to open Home  ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(confirm, sections[5]);
    } else {
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .border_style(Style::default().fg(Color::Cyan));
        let input_area = input_block.inner(sections[5]);
        frame.render_widget(input_block, sections[5]);

        let max_width = input_area.width.saturating_sub(1) as usize;
        let (display, cursor_offset) = fit_input_for_box(&app.setup.input, max_width);
        frame.render_widget(Paragraph::new(display), input_area);
        frame.set_cursor_position(Position::new(
            input_area.x + cursor_offset as u16,
            input_area.y,
        ));
    }

    // Status / error
    let (notice_text, notice_color) = if app.setup.notice.to_lowercase().contains("error")
        || app.setup.notice.to_lowercase().contains("not found")
        || app.setup.notice.to_lowercase().contains("required")
    {
        (format!("  {}", app.setup.notice), Color::Red)
    } else {
        (format!("  {}", app.setup.notice), Color::Yellow)
    };
    frame.render_widget(
        Paragraph::new(notice_text)
            .style(Style::default().fg(notice_color))
            .wrap(Wrap { trim: true }),
        sections[6],
    );
}

fn draw_home(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    // Split into menu (left) and sidebar (right)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(32), Constraint::Length(36)])
        .split(area);

    // Menu list
    let mut items = Vec::with_capacity(HOME_MENU_ITEMS.len());
    for (idx, (label, _)) in HOME_MENU_ITEMS.iter().enumerate() {
        let key = format!("{}", idx + 1);
        let disabled_reason = home_item_disabled_reason(app, idx);
        if disabled_reason.is_some() {
            items.push(ListItem::new(Line::from(vec![
                ratatui::text::Span::styled(
                    format!(" {key} "),
                    Style::default().fg(Color::DarkGray),
                ),
                ratatui::text::Span::raw(" "),
                ratatui::text::Span::styled(
                    label.to_string(),
                    Style::default().fg(Color::DarkGray),
                ),
                ratatui::text::Span::styled(
                    "  [disabled]",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::DIM),
                ),
            ])));
        } else {
            items.push(ListItem::new(Line::from(vec![
                ratatui::text::Span::styled(
                    format!(" {key} "),
                    Style::default().fg(Color::DarkGray),
                ),
                ratatui::text::Span::raw(" "),
                ratatui::text::Span::styled(label.to_string(), Style::default().fg(Color::White)),
            ])));
        }
    }

    // Show description of selected item at bottom
    let selected_desc = HOME_MENU_ITEMS
        .get(app.home_cursor)
        .map(|(_, d)| *d)
        .unwrap_or("");
    let cursor_disabled_reason = home_item_disabled_reason(app, app.home_cursor);
    let (desc_text, desc_color) = if let Some(reason) = cursor_disabled_reason.as_deref() {
        (format!("  {reason}"), Color::Yellow)
    } else {
        (format!("  {selected_desc}"), Color::Gray)
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .split(cols[0]);

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Menu [↑↓ Select, Enter Open] ")
        )
        .highlight_style(
            Style::default()
                .fg(Color::DarkGray)
                .bg(if cursor_disabled_reason.is_some() {
                    Color::Reset
                } else {
                    Color::Cyan
                })
                .add_modifier(if cursor_disabled_reason.is_some() {
                    Modifier::empty()
                } else {
                    Modifier::BOLD
                }),
        )
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.home_cursor));
    frame.render_stateful_widget(list, rows[0], &mut state);

    // Description bar
    frame.render_widget(
        Paragraph::new(Line::styled(desc_text, Style::default().fg(desc_color))).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if cursor_disabled_reason.is_some() {
                    Color::Yellow
                } else {
                    Color::DarkGray
                })),
        ),
        rows[1],
    );

    // Sidebar
    draw_home_sidebar(frame, app, cols[1]);
}

fn draw_home_sidebar(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Quick Stats ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(data) = &app.last_data else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(
                    "  No data loaded yet.",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::from(""),
                Line::styled(
                    "  Run 'Analyze Now' to",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::styled(
                    "  populate this panel.",
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            inner,
        );
        return;
    };

    let total_emoji = data.messages.content.emoji_unicode + data.messages.content.emoji_custom;

    let mut lines = vec![
        stat_line("Messages", &fmt_num(data.messages.total)),
        stat_line("Channels", &fmt_num(data.messages.channels)),
        stat_line(
            "With text",
            &format!(
                "{:.1}%",
                ratio(data.messages.with_content, data.messages.total) * 100.0
            ),
        ),
        stat_line(
            "Avg length",
            &format!("{:.0} ch", data.messages.content.avg_length_chars),
        ),
        stat_line("Emoji", &fmt_num(total_emoji)),
        stat_line("Attach.", &fmt_num(data.messages.with_attachments)),
        Line::from(""),
        stat_line("Servers", &fmt_num(data.servers.count)),
        stat_line("Tickets", &fmt_num(data.support_tickets.count)),
        stat_line("Activity", &fmt_num(data.activity.total_events)),
        Line::from(""),
    ];

    if let (Some(first), Some(last)) = (
        &data.messages.temporal.first_message_date,
        &data.messages.temporal.last_message_date,
    ) {
        lines.push(Line::styled(
            "  History",
            Style::default().fg(Color::DarkGray),
        ));
        lines.push(Line::styled(
            format!("  {first}"),
            Style::default().fg(Color::Gray),
        ));
        lines.push(Line::styled("  →", Style::default().fg(Color::DarkGray)));
        lines.push(Line::styled(
            format!("  {last}"),
            Style::default().fg(Color::Gray),
        ));
    }

    // Most active hour mini chart
    if !data.messages.temporal.by_hour.is_empty()
        && let Some((&peak_hr, &peak_cnt)) = data
            .messages
            .temporal
            .by_hour
            .iter()
            .max_by_key(|&(_, c)| c)
    {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            format!("  Peak {:02}:00  {} msgs", peak_hr, fmt_num(peak_cnt)),
            Style::default().fg(Color::Cyan),
        ));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn draw_overview(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let Some(data) = &app.last_data else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(
                    "  No analysis data loaded. Run Analyze Now first.",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Overview ")
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            area,
        );
        return;
    };

    // Layout: top row (messages | servers/tickets) + bottom row (hour chart | top words | top channels)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
        .split(area);

    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(rows[0]);

    let bot_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ])
        .split(rows[1]);

    // ── Panel 1: Messages ──────────────────────────────────────────────────
    let total_emoji = data.messages.content.emoji_unicode + data.messages.content.emoji_custom;
    let msg_lines = vec![
        stat_line("Total messages", &fmt_num(data.messages.total)),
        stat_line("Channels", &fmt_num(data.messages.channels)),
        stat_line(
            "With text",
            &format!(
                "{} ({:.1}%)",
                fmt_num(data.messages.with_content),
                ratio(data.messages.with_content, data.messages.total) * 100.0
            ),
        ),
        stat_line(
            "With attachments",
            &format!(
                "{} ({:.1}%)",
                fmt_num(data.messages.with_attachments),
                ratio(data.messages.with_attachments, data.messages.total) * 100.0
            ),
        ),
        stat_line(
            "Avg length",
            &format!("{:.1} chars", data.messages.content.avg_length_chars),
        ),
        stat_line("Total chars", &fmt_num(data.messages.content.total_chars)),
        stat_line(
            "Emoji (unicode)",
            &fmt_num(data.messages.content.emoji_unicode),
        ),
        stat_line(
            "Emoji (custom)",
            &fmt_num(data.messages.content.emoji_custom),
        ),
        stat_line("Total emoji", &fmt_num(total_emoji)),
        stat_line("Line breaks", &fmt_num(data.messages.content.linebreaks)),
        stat_line(
            "Distinct chars",
            &fmt_num(data.messages.content.distinct_characters as u64),
        ),
    ];
    frame.render_widget(
        Paragraph::new(msg_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Messages ")
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: true }),
        top_cols[0],
    );

    // ── Panel 2: Servers / Tickets / Activity ─────────────────────────────
    let mut right_lines = vec![
        Line::styled(
            " Servers",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        stat_line("Count", &fmt_num(data.servers.count)),
        stat_line("Index entries", &fmt_num(data.servers.index_entries)),
        stat_line("Audit logs", &fmt_num(data.servers.audit_log_entries)),
        Line::from(""),
        Line::styled(
            " Support Tickets",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        stat_line("Count", &fmt_num(data.support_tickets.count)),
        stat_line("Comments", &fmt_num(data.support_tickets.comments)),
        stat_line(
            "Tickets w/ comments",
            &format!(
                "{} ({:.1}%)",
                fmt_num(data.support_tickets.tickets_with_comments),
                ratio(
                    data.support_tickets.tickets_with_comments,
                    data.support_tickets.count
                ) * 100.0
            ),
        ),
        stat_line(
            "Avg comments/ticket",
            &format!("{:.2}", data.support_tickets.avg_comments_per_ticket),
        ),
        Line::from(""),
        Line::styled(
            " Activity",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        stat_line("Events", &fmt_num(data.activity.total_events)),
        stat_line(
            "Parse errors",
            &format!(
                "{} ({:.2}%)",
                fmt_num(data.activity.parse_errors),
                ratio(data.activity.parse_errors, data.activity.total_events) * 100.0
            ),
        ),
    ];

    // channel type breakdown
    if !data.messages.by_channel_type.is_empty() {
        right_lines.push(Line::from(""));
        right_lines.push(Line::styled(
            " Channel Types",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        for (name, count) in top_counts(&data.messages.by_channel_type, 5) {
            right_lines.push(stat_line(&name, &fmt_num(count)));
        }
    }

    if !data.support_tickets.by_status.is_empty() {
        right_lines.push(Line::from(""));
        right_lines.push(Line::styled(
            " Ticket Status",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        for (status, count) in top_counts(&data.support_tickets.by_status, 4) {
            right_lines.push(stat_line(&status, &fmt_num(count)));
        }
    }

    frame.render_widget(
        Paragraph::new(right_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Servers & Activity ")
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: true }),
        top_cols[1],
    );

    // ── Panel 3: Hour-of-day bar chart ────────────────────────────────────
    draw_hour_chart(frame, data, bot_cols[0]);

    // ── Panel 4: Top words ─────────────────────────────────────────────────
    let word_lines: Vec<Line> = {
        let mut v = vec![Line::from("")];
        for (word, count) in data.messages.content.top_words.iter().take(15) {
            v.push(Line::from(vec![
                ratatui::text::Span::styled(
                    format!("  {word:<14}"),
                    Style::default().fg(Color::White),
                ),
                ratatui::text::Span::styled(
                    format!("{:>6}", fmt_num(*count)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        v
    };
    frame.render_widget(
        Paragraph::new(word_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Top Words ")
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        bot_cols[1],
    );

    // ── Panel 5: Top channels ──────────────────────────────────────────────
    let ch_lines: Vec<Line> = {
        let mut v = vec![Line::from("")];
        for (name, count) in data.messages.top_channels.iter().take(15) {
            let short = if name.chars().count() > 16 {
                format!("{}…", name.chars().take(15).collect::<String>())
            } else {
                name.clone()
            };
            v.push(Line::from(vec![
                ratatui::text::Span::styled(
                    format!("  {short:<17}"),
                    Style::default().fg(Color::White),
                ),
                ratatui::text::Span::styled(
                    format!("{:>5}", fmt_num(*count)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
        v
    };
    frame.render_widget(
        Paragraph::new(ch_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Top Channels ")
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        bot_cols[2],
    );
}

fn draw_support_activity(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let tickets: &[SupportTicketView] = app.support_tickets.as_deref().unwrap_or(&[]);
    if tickets.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(
                    "  No support tickets found (or not loaded yet).",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::from(""),
                Line::styled(
                    "  Press r to reload from your export.",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::styled(
                    "  Browse with ↑/↓, press Enter to open a ticket.",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Support Tickets ")
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            area,
        );
        return;
    }

    let visible_rows = area.height.saturating_sub(2) as usize;
    let page_size = visible_rows.max(1);
    let start = app
        .support_ticket_cursor
        .saturating_sub(page_size / 2)
        .min(tickets.len().saturating_sub(page_size));
    let end = (start + page_size).min(tickets.len());

    let mut items = Vec::with_capacity(end.saturating_sub(start));
    for (local_idx, ticket) in tickets[start..end].iter().enumerate() {
        let idx = start + local_idx + 1;
        let row = format!(
            "{idx:>4}  [{:<10}] {:<40}  {:<8}  c:{}",
            truncate_text(&ticket.status, 10),
            truncate_text(&ticket.subject, 40),
            truncate_text(&ticket.priority, 8),
            ticket.comment_count
        );
        items.push(ListItem::new(Line::styled(
            row,
            Style::default().fg(Color::White),
        )));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    " Support Tickets: {} [↑↓ Select, Enter View] ",
                    tickets.len()
                ))
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.support_ticket_cursor.saturating_sub(start)));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_activity(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let all_event_count = app.activity_events.as_ref().map(|v| v.len()).unwrap_or(0);
    let filtered = filtered_activity_events(app);
    let filtered_count = filtered.len();
    let event_cursor = app.activity_cursor.min(filtered_count.saturating_sub(1));
    let mut by_type = std::collections::BTreeMap::<String, u64>::new();
    for event in &filtered {
        *by_type.entry(event.event_type.clone()).or_insert(0) += 1;
    }
    let top_types = top_counts(&by_type, 4)
        .into_iter()
        .map(|(name, count)| format!("{}({})", truncate_text(&name, 14), count))
        .collect::<Vec<_>>()
        .join(", ");
    let top_types = if top_types.is_empty() {
        "n/a".to_owned()
    } else {
        top_types
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " Activity Explorer ({}/{}) ",
            filtered_count, all_event_count
        ))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);
    let edit_label = match app.activity_filter_edit {
        Some(field) => format!("editing {}", filter_field_label(field)),
        None => "edit off".to_owned(),
    };
    let filter_line = format!(
        " q:{}  type:{}  src:{}  from:{}  to:{}  sort:{}  {}",
        render_filter_value(&app.activity_filters.query),
        render_filter_value(&app.activity_filters.event_type),
        render_filter_value(&app.activity_filters.source_file),
        render_filter_value(&app.activity_filters.from_date),
        render_filter_value(&app.activity_filters.to_date),
        app.activity_sort.label(),
        edit_label,
    );
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                format!("  Memory-safe tail-read mode | top types: {top_types}"),
                Style::default().fg(Color::DarkGray),
            ),
            Line::styled(
                format!("  {filter_line}"),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        rows[0],
    );

    if filtered.is_empty() {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
            .split(rows[1]);
        draw_activity_quick_stats(
            frame,
            app,
            cols[1],
            filtered_count,
            all_event_count,
            &by_type,
        );
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(
                    "  No activity events match current filters.",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::styled(
                    "  Use / t y [ ] to edit filters, o to change sort, c to clear.",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Activity Feed [No matches] ")
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            cols[0],
        );
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(rows[1]);

    let visible_rows = cols[0].height.saturating_sub(2) as usize;
    let page_size = visible_rows.max(1);
    let start = event_cursor
        .saturating_sub(page_size / 2)
        .min(filtered_count.saturating_sub(page_size));
    let end = (start + page_size).min(filtered_count);

    let mut items = Vec::with_capacity(end.saturating_sub(start));
    for (local_idx, event) in filtered[start..end].iter().enumerate() {
        let idx = start + local_idx + 1;
        let summary = truncate_text(&event.summary, 72);
        items.push(ListItem::new(Line::from(vec![
            ratatui::text::Span::styled(format!("{idx:>4} "), Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                format!("[{}] ", truncate_text(&event.timestamp, 20)),
                Style::default().fg(Color::Blue),
            ),
            ratatui::text::Span::styled(
                format!("{:<16} ", truncate_text(&event.event_type, 16)),
                Style::default().fg(Color::Cyan),
            ),
            ratatui::text::Span::styled(summary, Style::default().fg(Color::White)),
            ratatui::text::Span::styled(
                format!("  ({})", truncate_text(&event.source_file, 20)),
                Style::default().fg(Color::DarkGray),
            ),
        ])));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Activity Feed [↑↓ Browse, Enter Detail, / Search] ")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");
    let mut state = ListState::default();
    state.select(Some(event_cursor.saturating_sub(start)));
    frame.render_stateful_widget(list, cols[0], &mut state);
    draw_activity_quick_stats(
        frame,
        app,
        cols[1],
        filtered_count,
        all_event_count,
        &by_type,
    );
}

fn draw_support_ticket_detail(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let Some(ticket) = app
        .support_tickets
        .as_ref()
        .and_then(|tickets| tickets.get(app.support_ticket_cursor))
    else {
        frame.render_widget(
            Paragraph::new("No support ticket selected.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Ticket Detail "),
            ),
            area,
        );
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(4)])
        .split(area);

    let info = Paragraph::new(vec![
        Line::from(vec![
            ratatui::text::Span::styled("  Ticket ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                format!("#{}", ticket.id),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::raw("  "),
            ratatui::text::Span::styled(
                truncate_text(&ticket.subject, 80),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            ratatui::text::Span::styled("  Status: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(ticket.status.clone(), Style::default().fg(Color::White)),
            ratatui::text::Span::styled("   Priority: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(ticket.priority.clone(), Style::default().fg(Color::White)),
            ratatui::text::Span::styled("   Comments: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                ticket.comment_count.to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Ticket Info ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(info, rows[0]);

    let scroll_indicator = format!(
        " Ticket Content: line {}/{} [↑↓ Scroll, B Back] ",
        app.support_ticket_scroll + 1,
        ticket.detail_lines.len().max(1)
    );
    let detail_lines: Vec<Line> = ticket
        .detail_lines
        .iter()
        .map(|line| Line::from(line.as_str()))
        .collect();
    frame.render_widget(
        Paragraph::new(detail_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(scroll_indicator)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.support_ticket_scroll as u16, 0)),
        rows[1],
    );
}

fn draw_activity_detail(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let filtered = filtered_activity_events(app);
    let Some(event) = filtered.get(app.activity_cursor.min(filtered.len().saturating_sub(1)))
    else {
        frame.render_widget(
            Paragraph::new("No activity event selected.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Activity Detail "),
            ),
            area,
        );
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(4)])
        .split(area);

    let info = Paragraph::new(vec![
        Line::from(vec![
            ratatui::text::Span::styled("  Timestamp: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(event.timestamp.clone(), Style::default().fg(Color::Blue)),
        ]),
        Line::from(vec![
            ratatui::text::Span::styled("  Type: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(event.event_type.clone(), Style::default().fg(Color::Cyan)),
            ratatui::text::Span::styled("   Source: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                event.source_file.clone(),
                Style::default().fg(Color::White),
            ),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Event Info ")
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(info, rows[0]);

    let scroll_indicator = format!(
        " Event Detail: line {}/{} [↑↓ Scroll, B Back] ",
        app.activity_detail_scroll + 1,
        event.detail.lines().count().max(1)
    );
    let mut detail_lines = vec![
        Line::styled(" Summary", Style::default().fg(Color::DarkGray)),
        Line::styled(
            format!(" {}", event.summary),
            Style::default().fg(Color::White),
        ),
        Line::from(""),
        Line::styled(" Raw event", Style::default().fg(Color::DarkGray)),
    ];
    for line in event.detail.lines() {
        detail_lines.push(Line::styled(
            format!(" {}", line),
            Style::default().fg(Color::Gray),
        ));
    }

    frame.render_widget(
        Paragraph::new(detail_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(scroll_indicator)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.activity_detail_scroll as u16, 0)),
        rows[1],
    );
}

fn draw_hour_chart(frame: &mut ratatui::Frame<'_>, data: &analyzer::AnalysisData, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Messages by Hour (UTC) ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if data.messages.temporal.by_hour.is_empty() || inner.height < 2 || inner.width < 24 {
        return;
    }

    let max_count = data
        .messages
        .temporal
        .by_hour
        .values()
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);
    let chart_height = inner.height.saturating_sub(2) as u64;
    let bar_width = (inner.width / 24).max(1);

    let mut lines: Vec<Line> = Vec::new();

    // Build bars line by line (top to bottom)
    for row in (0..inner.height.saturating_sub(1)).rev() {
        let threshold = (row as u64 * max_count) / inner.height.saturating_sub(1) as u64;
        let mut spans = Vec::new();
        for hour in 0u32..24 {
            let count = data
                .messages
                .temporal
                .by_hour
                .get(&hour)
                .copied()
                .unwrap_or(0);
            let bar_h = (count * chart_height) / max_count;
            let fill = bar_h >= (inner.height.saturating_sub(1) - row) as u64;
            let ch = if fill { "█" } else { " " };
            let color = if fill {
                // gradient: low=DarkGray, mid=Blue, high=Cyan
                let frac = count as f32 / max_count as f32;
                if frac > 0.75 {
                    Color::Cyan
                } else if frac > 0.4 {
                    Color::Blue
                } else {
                    Color::DarkGray
                }
            } else {
                Color::Reset
            };
            for _ in 0..bar_width {
                spans.push(ratatui::text::Span::styled(ch, Style::default().fg(color)));
            }
        }
        lines.push(Line::from(spans));
        let _ = threshold;
    }

    // Hour labels (every 6h)
    let mut label_spans = Vec::new();
    for hour in 0u32..24 {
        let label = if hour % 6 == 0 {
            format!("{hour:02}")
        } else {
            " ".repeat(bar_width as usize)
        };
        let s = format!("{label:<width$}", width = bar_width as usize);
        label_spans.push(ratatui::text::Span::styled(
            s,
            Style::default().fg(Color::DarkGray),
        ));
    }
    lines.push(Line::from(label_spans));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_channels(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let channels = filtered_channels(app);

    let filter_tabs = "  1:All  2:DMs  3:Groups  4:Threads  5:Voice";
    let title = format!(" Channels: {} [↑↓ Select, Enter Messages, 1-5 Filter] ", app.current_filter.label());

    if channels.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(
                    "  No channels match this filter.",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::from(""),
                Line::styled(filter_tabs, Style::default().fg(Color::DarkGray)),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Channels: {} [No matches] ", app.current_filter.label()))
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            area,
        );
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(4)])
        .split(area);

    // Filter tabs at top
    let mut tab_spans = Vec::new();
    for (i, (filter, label)) in [
        (ChannelFilter::All, "1:All"),
        (ChannelFilter::Dm, "2:DMs"),
        (ChannelFilter::GroupDm, "3:Groups"),
        (ChannelFilter::PublicThread, "4:Threads"),
        (ChannelFilter::Voice, "5:Voice"),
    ]
    .iter()
    .enumerate()
    {
        let active = app.current_filter == *filter;
        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        tab_spans.push(ratatui::text::Span::styled(format!(" {label} "), style));
        if i < 4 {
            tab_spans.push(ratatui::text::Span::raw("  "));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(tab_spans)).block(Block::default().borders(Borders::NONE)),
        rows[0],
    );

    // Channel list
    let visible_rows = rows[1].height.saturating_sub(2) as usize;
    let page_size = visible_rows.max(1);
    let start = app
        .channel_cursor
        .saturating_sub(page_size / 2)
        .min(channels.len().saturating_sub(page_size));
    let end = (start + page_size).min(channels.len());

    let max_count = channels
        .iter()
        .map(|c| c.message_count)
        .max()
        .unwrap_or(1)
        .max(1);

    let mut items = Vec::new();
    for (local_idx, channel) in channels[start..end].iter().enumerate() {
        let idx = start + local_idx + 1;
        let kind_color = match channel.kind {
            ChannelKind::Dm => Color::Green,
            ChannelKind::GroupDm => Color::LightGreen,
            ChannelKind::PublicThread => Color::Blue,
            ChannelKind::Voice => Color::Magenta,
            ChannelKind::Guild => Color::Yellow,
            ChannelKind::Other => Color::DarkGray,
        };
        // mini bar (up to 8 chars)
        let bar_len = (channel.message_count * 8 / max_count).max(if channel.message_count > 0 {
            1
        } else {
            0
        });
        let bar = format!(
            "{}{}",
            "█".repeat(bar_len),
            "░".repeat(8usize.saturating_sub(bar_len))
        );

        let short_title = if channel.title.chars().count() > 34 {
            format!("{}…", channel.title.chars().take(33).collect::<String>())
        } else {
            channel.title.clone()
        };

        items.push(ListItem::new(Line::from(vec![
            ratatui::text::Span::styled(format!("{idx:>4} "), Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                format!("{:<10} ", channel.kind.label()),
                Style::default().fg(kind_color),
            ),
            ratatui::text::Span::styled(
                format!("{short_title:<35}"),
                Style::default().fg(Color::White),
            ),
            ratatui::text::Span::styled(
                format!("{:>6} ", fmt_num(channel.message_count as u64)),
                Style::default().fg(Color::DarkGray),
            ),
            ratatui::text::Span::styled(bar, Style::default().fg(Color::Cyan)),
        ])));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.channel_cursor.saturating_sub(start)));
    frame.render_stateful_widget(list, rows[1], &mut state);
}

fn draw_message_view(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let Some(channel) = &app.open_channel else {
        frame.render_widget(
            Paragraph::new("No channel selected.")
                .block(Block::default().borders(Borders::ALL).title(" Messages ")),
            area,
        );
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(4)])
        .split(area);

    // Channel info header
    let kind_color = match channel.kind {
        ChannelKind::Dm => Color::Green,
        ChannelKind::GroupDm => Color::LightGreen,
        ChannelKind::PublicThread => Color::Blue,
        ChannelKind::Voice => Color::Magenta,
        ChannelKind::Guild => Color::Yellow,
        ChannelKind::Other => Color::DarkGray,
    };
    let info = Paragraph::new(vec![
        Line::from(vec![
            ratatui::text::Span::styled("  ", Style::default()),
            ratatui::text::Span::styled(
                channel.kind.label(),
                Style::default().fg(kind_color).add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::raw("  "),
            ratatui::text::Span::styled(
                &channel.title,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            ratatui::text::Span::styled("  ID: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(&channel.id, Style::default().fg(Color::Gray)),
            ratatui::text::Span::styled("   Messages: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                fmt_num(channel.message_count as u64),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Channel Info ")
            .border_style(Style::default().fg(kind_color)),
    );
    frame.render_widget(info, rows[0]);

    // Message list
    if app.open_message_lines.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(
                "  No messages found.",
                Style::default().fg(Color::DarkGray),
            ))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Messages [No Content] ")
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            rows[1],
        );
    } else {
        let lines: Vec<Line> = app
            .open_message_lines
            .iter()
            .map(|l| {
                // Color timestamps
                if let Some(rest) = l.strip_prefix("- [")
                    && let Some(close) = rest.find(']')
                {
                    let ts = &rest[..close];
                    let msg = &rest[close + 1..];
                    return Line::from(vec![
                        ratatui::text::Span::styled("  [", Style::default().fg(Color::DarkGray)),
                        ratatui::text::Span::styled(ts, Style::default().fg(Color::Blue)),
                        ratatui::text::Span::styled("]", Style::default().fg(Color::DarkGray)),
                        ratatui::text::Span::styled(msg, Style::default().fg(Color::White)),
                    ]);
                }
                Line::from(l.as_str())
            })
            .collect();

        let scroll_indicator = format!(
            " Messages: line {}/{} [↑↓ Scroll, B Back] ",
            app.open_message_scroll + 1,
            app.open_message_lines.len()
        );
        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(scroll_indicator)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.open_message_scroll as u16, 0));
        frame.render_widget(paragraph, rows[1]);
    }
}

fn draw_settings(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let items = vec![
        ListItem::new(Line::from(vec![
            ratatui::text::Span::styled(
                " Auto-download attachments  ",
                Style::default().fg(Color::White),
            ),
            ratatui::text::Span::styled(
                if app.settings.download_attachments {
                    " ON "
                } else {
                    " OFF"
                },
                if app.settings.download_attachments {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray).bg(Color::Black)
                },
            ),
        ])),
        ListItem::new(Line::from(vec![
            ratatui::text::Span::styled(
                " Preview messages per channel  ",
                Style::default().fg(Color::White),
            ),
            ratatui::text::Span::styled(
                format!(" {} ", app.settings.preview_messages),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::styled("  ← → to adjust", Style::default().fg(Color::DarkGray)),
        ])),
        ListItem::new(Line::styled(
            " Reconfigure export / results / profile",
            Style::default().fg(Color::White),
        )),
        ListItem::new(Line::styled(" Back", Style::default().fg(Color::DarkGray))),
    ];

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Settings [↑↓ Select, ←→ Adjust, Enter Toggle] ")
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.settings_cursor));
    frame.render_stateful_widget(list, area, &mut state);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let px = percent_x.clamp(10, 100);
    let py = percent_y.clamp(10, 100);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - py) / 2),
            Constraint::Percentage(py),
            Constraint::Percentage(100 - py - ((100 - py) / 2)),
        ])
        .split(area);

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - px) / 2),
            Constraint::Percentage(px),
            Constraint::Percentage(100 - px - ((100 - px) / 2)),
        ])
        .split(vertical[1]);

    horizontal[1]
}

fn fit_input_for_box(input: &str, width: usize) -> (String, usize) {
    if width == 0 {
        return (String::new(), 0);
    }
    let count = input.chars().count();
    if count <= width {
        return (input.to_owned(), count);
    }

    let start = count.saturating_sub(width);
    let display = input.chars().skip(start).collect::<String>();
    (display, width)
}

fn stat_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        ratatui::text::Span::styled(
            format!("  {label:<22}"),
            Style::default().fg(Color::DarkGray),
        ),
        ratatui::text::Span::styled(value.to_owned(), Style::default().fg(Color::White)),
    ])
}

fn draw_activity_quick_stats(
    frame: &mut ratatui::Frame<'_>,
    app: &AppState,
    area: Rect,
    filtered_count: usize,
    all_event_count: usize,
    filtered_by_type: &std::collections::BTreeMap<String, u64>,
) {
    let mut lines = vec![
        Line::styled(
            " Quick Stats",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        stat_line("Loaded (tail)", &fmt_num(all_event_count as u64)),
        stat_line("Filtered", &fmt_num(filtered_count as u64)),
        stat_line("Sort", app.activity_sort.label()),
        stat_line(
            "Active filters",
            &activity_active_filter_count(app).to_string(),
        ),
    ];

    if let Some(data) = &app.last_data {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            " Analyzed totals",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(stat_line("Files", &fmt_num(data.activity.files)));
        lines.push(stat_line("Events", &fmt_num(data.activity.total_events)));
        let parse_rate = if data.activity.total_events == 0 {
            0.0
        } else {
            ratio(data.activity.parse_errors, data.activity.total_events) * 100.0
        };
        lines.push(stat_line(
            "Parse errors",
            &format!("{} ({parse_rate:.2}%)", fmt_num(data.activity.parse_errors)),
        ));

        lines.push(Line::from(""));
        lines.push(Line::styled(
            " Top types (all)",
            Style::default().fg(Color::DarkGray),
        ));
        let all_top = top_counts(&data.activity.by_event_type, 5);
        if all_top.is_empty() {
            lines.push(Line::styled("  n/a", Style::default().fg(Color::DarkGray)));
        } else {
            for (name, count) in all_top {
                lines.push(stat_line(&truncate_text(&name, 16), &fmt_num(count)));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::styled(
        " Top types (view)",
        Style::default().fg(Color::DarkGray),
    ));
    let filtered_top = top_counts(filtered_by_type, 5);
    if filtered_top.is_empty() {
        lines.push(Line::styled("  n/a", Style::default().fg(Color::DarkGray)));
    } else {
        for (name, count) in filtered_top {
            lines.push(stat_line(&truncate_text(&name, 16), &fmt_num(count)));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Activity Stats ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn activity_active_filter_count(app: &AppState) -> usize {
    let mut count = 0usize;
    if !app.activity_filters.query.trim().is_empty() {
        count += 1;
    }
    if !app.activity_filters.event_type.trim().is_empty() {
        count += 1;
    }
    if !app.activity_filters.source_file.trim().is_empty() {
        count += 1;
    }
    if !app.activity_filters.from_date.trim().is_empty() {
        count += 1;
    }
    if !app.activity_filters.to_date.trim().is_empty() {
        count += 1;
    }
    count
}

fn filter_field_label(field: ActivityFilterField) -> &'static str {
    match field {
        ActivityFilterField::Query => "query",
        ActivityFilterField::EventType => "type",
        ActivityFilterField::SourceFile => "source",
        ActivityFilterField::FromDate => "from-date",
        ActivityFilterField::ToDate => "to-date",
    }
}

fn render_filter_value(value: &str) -> String {
    if value.trim().is_empty() {
        "∅".to_owned()
    } else {
        truncate_text(value.trim(), 18)
    }
}

fn draw_gallery(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let files = filtered_gallery_files(app);
    let categories = [
        (None, "All"),
        (Some("imgs"), "Images"),
        (Some("vids"), "Videos"),
        (Some("audios"), "Audio"),
        (Some("docs"), "Docs"),
        (Some("txts"), "Text"),
        (Some("codes"), "Code"),
        (Some("zips"), "Archives"),
        (None, "Others"),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(5)])
        .split(area);

    // Filter tabs
    let mut tab_spans = Vec::new();
    for (i, (opt, label)) in categories.iter().enumerate() {
        let active = match (&app.gallery.category_filter, opt) {
            (None, None) => i == 0, // "All" is the first None
            (Some(f), Some(o)) => f == o,
            _ => false,
        };
        let (num_key, label_text) = if i < 9 {
            (format!("{}", i + 1), *label)
        } else {
            ("?".to_owned(), *label)
        };
        
        let style = if active {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        tab_spans.push(ratatui::text::Span::styled(format!(" {num_key}:{label_text} "), style));
        if i < categories.len() - 1 {
            tab_spans.push(ratatui::text::Span::raw(" "));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), chunks[0]);

    if files.is_empty() {
        frame.render_widget(
            Paragraph::new("\n  No files found in this category.\n  Make sure you have downloaded attachments first.")
                .block(Block::default().borders(Borders::ALL).title(" Gallery ")
                .border_style(Style::default().fg(Color::Cyan))),
            chunks[1]
        );
        return;
    }

    let visible_rows = chunks[1].height.saturating_sub(2) as usize;
    let page_size = visible_rows.max(1);
    let start = app.gallery.cursor.saturating_sub(page_size / 2).min(files.len().saturating_sub(page_size));
    let end = (start + page_size).min(files.len());

    let mut items = Vec::new();
    for (local_idx, file) in files[start..end].iter().enumerate() {
        let idx = start + local_idx + 1;
        let cat_color = match file.category.as_str() {
            "imgs" => Color::Green,
            "vids" => Color::Yellow,
            "audios" => Color::Magenta,
            "docs" => Color::Blue,
            "codes" | "txts" => Color::Cyan,
            "zips" | "exes" => Color::Red,
            _ => Color::DarkGray,
        };

        let file_info = Line::from(vec![
            ratatui::text::Span::styled(format!("{idx:>4} "), Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(format!("{:<10} ", file.category), Style::default().fg(cat_color)),
            ratatui::text::Span::styled(truncate_text(&file.name, 45), Style::default().fg(Color::White)),
            ratatui::text::Span::raw("  "),
            ratatui::text::Span::styled(format_size(file.size), Style::default().fg(Color::DarkGray)),
        ]);
        items.push(ListItem::new(file_info));
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(format!(" Gallery: {} files discovered ", files.len()))
        .border_style(Style::default().fg(Color::Cyan)))
        .highlight_style(Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD))
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.gallery.cursor.saturating_sub(start)));
    frame.render_stateful_widget(list, chunks[1], &mut state);
}

fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
