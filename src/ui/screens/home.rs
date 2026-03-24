use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Alignment, Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    app::{AppState, fmt_num, ratio, top_counts},
    ui::components::stat_line,
};

pub(crate) fn draw_home(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    draw_home_dashboard(frame, app, area);
}

fn draw_home_dashboard(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Length(3), Constraint::Min(8)])
        .split(area);

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
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Dashboard ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            area,
        );
        return;
    };

    let export_path = app.config.package_path(&app.config_path, &app.id);
    let welcome_lines = vec![
        Line::styled(" Last analyzed: ready", Style::default().fg(Color::Gray)),
        Line::styled(
            format!(" Export: {}", export_path.display()),
            Style::default().fg(Color::DarkGray),
        ),
        Line::from(""),
        Line::from(vec![
            ratatui::text::Span::styled("  Messages ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                format!("{:<8}", fmt_num(data.messages.total)),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::styled("Channels ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                format!("{:<6}", fmt_num(data.messages.channels)),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::styled("Servers ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                fmt_num(data.servers.count),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(welcome_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Welcome ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: true }),
        right_rows[0],
    );

    let actions = Line::from(vec![
        ratatui::text::Span::styled("[R] Re-analyze  ", Style::default().fg(Color::Cyan)),
        ratatui::text::Span::styled("[D] Download  ", Style::default().fg(Color::Cyan)),
        ratatui::text::Span::styled("[E] Export", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(
        Paragraph::new(actions)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Quick Actions ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
        right_rows[1],
    );

    let bottom_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(right_rows[2]);

    let mut top_lines: Vec<Line> = data
        .messages
        .top_channels
        .iter()
        .take(6)
        .map(|(name, count)| {
            Line::from(vec![
                ratatui::text::Span::styled(format!("  {name:<18}"), Style::default().fg(Color::White)),
                ratatui::text::Span::styled(
                    format!("{:>8}", fmt_num(*count)),
                    Style::default().fg(Color::DarkGray),
                ),
            ])
        })
        .collect();
    if top_lines.is_empty() {
        top_lines = top_counts(&data.messages.by_channel_type, 5)
            .into_iter()
            .map(|(name, count)| {
                Line::from(vec![
                    ratatui::text::Span::styled(
                        format!("  {name:<18}"),
                        Style::default().fg(Color::White),
                    ),
                    ratatui::text::Span::styled(
                        format!("{:>8}", fmt_num(count)),
                        Style::default().fg(Color::DarkGray),
                    ),
                ])
            })
            .collect();
    }
    if top_lines.is_empty() {
        top_lines.push(Line::styled(
            "  No channel stats available.",
            Style::default().fg(Color::DarkGray),
        ));
    }
    top_lines.push(Line::from(""));
    top_lines.push(stat_line("Servers", &fmt_num(data.servers.count)));
    top_lines.push(stat_line("Tickets", &fmt_num(data.support_tickets.count)));
    top_lines.push(stat_line("Activity", &fmt_num(data.activity.total_events)));
    frame.render_widget(
        Paragraph::new(top_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Top Channels ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: true }),
        bottom_cols[0],
    );

    let peak = data
        .messages
        .temporal
        .by_hour
        .iter()
        .max_by_key(|&(_, c)| c)
        .map(|(h, c)| format!(" Peak {:02}:00 ({})", h, fmt_num(*c)))
        .unwrap_or_else(|| " Peak n/a".to_owned());
    let activity = vec![
        Line::styled(
            format!(
                " {}",
                hour_sparkline(data.messages.temporal.by_hour.values().copied().collect())
            ),
            Style::default().fg(Color::Cyan),
        ),
        Line::styled(" 00   06   12   18   23", Style::default().fg(Color::DarkGray)),
        Line::from(""),
        Line::styled(peak, Style::default().fg(Color::Gray)),
        Line::styled(
            format!(
                " With text: {:.1}%",
                ratio(data.messages.with_content, data.messages.total) * 100.0
            ),
            Style::default().fg(Color::Gray),
        ),
        Line::styled(
            format!(
                " Date range: {} -> {}",
                data.messages
                    .temporal
                    .first_message_date
                    .as_deref()
                    .unwrap_or("n/a"),
                data.messages
                    .temporal
                    .last_message_date
                    .as_deref()
                    .unwrap_or("n/a")
            ),
            Style::default().fg(Color::DarkGray),
        ),
        Line::styled(
            format!(
                " Busiest day: {}",
                busiest_day_label(&data.messages.temporal.by_day_of_week)
            ),
            Style::default().fg(Color::DarkGray),
        ),
        Line::styled(
            format!(" Top words: {}", top_words_preview(&data.messages.content.top_words)),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    frame.render_widget(
        Paragraph::new(activity)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Activity ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: true }),
        bottom_cols[1],
    );
}

fn top_words_preview(words: &[(String, u64)]) -> String {
    let names: Vec<&str> = words.iter().take(3).map(|(w, _)| w.as_str()).collect();
    if names.is_empty() {
        "n/a".to_owned()
    } else {
        names.join(", ")
    }
}

fn busiest_day_label(by_day: &std::collections::BTreeMap<u32, u64>) -> &'static str {
    let day = by_day
        .iter()
        .max_by_key(|&(_, count)| count)
        .map(|(d, _)| *d)
        .unwrap_or(0);
    match day {
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        6 => "Sat",
        7 => "Sun",
        _ => "n/a",
    }
}

fn hour_sparkline(values: Vec<u64>) -> String {
    if values.is_empty() {
        return "no activity".to_owned();
    }
    let max = values.iter().copied().max().unwrap_or(1).max(1);
    let bins = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
    values
        .into_iter()
        .map(|v| {
            let idx = ((v as f64 / max as f64) * (bins.len() as f64 - 1.0)).round() as usize;
            bins[idx.min(bins.len() - 1)]
        })
        .collect::<Vec<_>>()
        .join("")
}


