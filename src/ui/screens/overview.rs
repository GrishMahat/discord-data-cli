use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    analyzer,
    app::{AppState, fmt_num, ratio, top_counts},
    ui::components::stat_line,
};

pub(crate) fn draw_overview(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
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

    draw_hour_chart(frame, data, bot_cols[0]);

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
