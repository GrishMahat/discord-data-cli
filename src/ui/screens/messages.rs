use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{AppState, ChannelKind, fmt_num};

pub(crate) fn draw_message_view(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
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
