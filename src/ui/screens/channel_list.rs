use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::{AppState, ChannelFilter, ChannelKind, filtered_channels, fmt_num};

pub(crate) fn draw_channels(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let channels = filtered_channels(app);

    let filter_tabs = "  1:All  2:DMs  3:Groups  4:Threads  5:Voice";
    let title = format!(
        " Channels: {} [↑↓ Select, Enter Messages, 1-5 Filter] ",
        app.current_filter.label()
    );

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
                    .title(format!(
                        " Channels: {} [No matches] ",
                        app.current_filter.label()
                    ))
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
