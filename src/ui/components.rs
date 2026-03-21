use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Alignment, Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{AppState, Screen, key_help, screen_disabled_reason};

pub(crate) fn draw_header(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
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
                ratatui::text::Span::styled("msgs ", Style::default().fg(Color::DarkGray)),
                ratatui::text::Span::styled(
                    format!("{}", data.messages.total),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                ratatui::text::Span::raw("  "),
                ratatui::text::Span::styled("servers ", Style::default().fg(Color::DarkGray)),
                ratatui::text::Span::styled(
                    format!("{}", data.servers.count),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                ratatui::text::Span::styled("channels ", Style::default().fg(Color::DarkGray)),
                ratatui::text::Span::styled(
                    format!("{}", data.messages.channels),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                ratatui::text::Span::raw("  "),
                ratatui::text::Span::styled("avg len ", Style::default().fg(Color::DarkGray)),
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

pub(crate) fn draw_tabs(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
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

pub(crate) fn draw_statusbar(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let help = key_help(app.screen);
    let mut spans = vec![
        ratatui::text::Span::styled(" ? ", Style::default().fg(Color::Black).bg(Color::Cyan)),
        ratatui::text::Span::raw(" "),
    ];

    let mut current = help;
    while let Some(start) = current.find('[') {
        if let Some(end_offset) = current[start..].find(']') {
            let end = start + end_offset;
            if start > 0 {
                spans.push(ratatui::text::Span::styled(
                    &current[..start],
                    Style::default().fg(Color::DarkGray),
                ));
            }
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

pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
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

pub(crate) fn fit_input_for_box(input: &str, width: usize) -> (String, usize) {
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

pub(crate) fn stat_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        ratatui::text::Span::styled(
            format!("  {label:<22}"),
            Style::default().fg(Color::DarkGray),
        ),
        ratatui::text::Span::styled(value.to_owned(), Style::default().fg(Color::White)),
    ])
}
