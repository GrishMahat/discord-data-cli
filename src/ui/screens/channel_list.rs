use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::{AppState, ChannelFilter, ChannelKind, filtered_channels, fmt_num};
use crate::ui::screens::messages::{kind_color, render_preview_lines};

const FILTER_TABS: [(&str, &str); 5] = [
    ("1", "All"),
    ("2", "DMs"),
    ("3", "Groups"),
    ("4", "Threads"),
    ("5", "Voice"),
];

/// Layout mirror shared with the mouse handler.
pub(crate) fn channel_browser_layout(area: Rect) -> (Rect, Rect, Rect, Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(area);
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(4)])
        .split(cols[0]);
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(7)])
        .split(cols[1]);
    (left_rows[0], left_rows[1], right_rows[0], right_rows[1])
}

pub(crate) fn draw_channels(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let channels = filtered_channels(app);

    if channels.is_empty() {
        let (msg, note) = if app.channel_loading {
            (
                "  Loading channels...",
                "This may take a moment for large exports.",
            )
        } else {
            (
                "  No channels match this filter.",
                "  1:All  2:DMs  3:Groups  4:Threads  5:Voice",
            )
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(msg, Style::default().fg(Color::Cyan)),
                Line::from(""),
                Line::styled(note, Style::default().fg(Color::DarkGray)),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(
                        " Channels: {} ",
                        app.current_filter.label()
                    ))
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            area,
        );
        return;
    }

    let (tabs_area, list_area, preview_area, stats_area) = channel_browser_layout(area);

    draw_filter_tabs(frame, app, tabs_area);
    draw_channel_list_pane(frame, app, &channels, list_area);
    draw_preview_pane(frame, app, preview_area);
    draw_stats_pane(frame, app, stats_area);
}

fn draw_filter_tabs(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let mut tab_spans = Vec::new();
    for (i, (key, label)) in FILTER_TABS.into_iter().enumerate() {
        let active = matches!(
            (key, app.current_filter),
            ("1", ChannelFilter::All)
                | ("2", ChannelFilter::Dm)
                | ("3", ChannelFilter::GroupDm)
                | ("4", ChannelFilter::PublicThread)
                | ("5", ChannelFilter::Voice)
        );
        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        tab_spans.push(ratatui::text::Span::styled(
            format!(" {key}:{label} "),
            style,
        ));
        if i + 1 < FILTER_TABS.len() {
            tab_spans.push(ratatui::text::Span::raw(" "));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(tab_spans)).block(Block::default().borders(Borders::NONE)),
        area,
    );
}

fn draw_channel_list_pane(
    frame: &mut ratatui::Frame<'_>,
    app: &AppState,
    channels: &[&crate::data::MessageChannel],
    area: Rect,
) {
    let visible_rows = area.height.saturating_sub(2) as usize;
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

    // Narrow pane: drop the bar column so titles still fit.
    let show_bar = area.width >= 46;
    let title_width = if show_bar { 26 } else { 34 };

    let mut items = Vec::new();
    for (local_idx, channel) in channels[start..end].iter().enumerate() {
        let idx = start + local_idx + 1;
        let k_color = kind_color(channel.kind);

        let mut spans = vec![
            ratatui::text::Span::styled(
                format!("{idx:>4} "),
                Style::default().fg(Color::DarkGray),
            ),
            ratatui::text::Span::styled(
                format!("{:<10} ", short_kind_label(channel.kind)),
                Style::default().fg(k_color),
            ),
        ];

        let title_width = title_width.min(area.width.saturating_sub(24) as usize).max(8);
        let short_title = crate::data::utils::truncate_text(&channel.title, title_width);
        spans.push(ratatui::text::Span::styled(
            format!("{short_title:<title_width$} "),
            Style::default().fg(Color::White),
        ));
        // Activity-tier badge: bright for busy channels, dim ghost for empty ones.
        let badge_style = if channel.message_count == 0 {
            Style::default().fg(Color::DarkGray)
        } else if channel.message_count >= 1000 {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if channel.message_count >= 100 {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Gray)
        };
        let count_label = fmt_num(channel.message_count as u64);
        spans.push(ratatui::text::Span::styled(
            format!("[{count_label:>7}] "),
            badge_style,
        ));

        if show_bar {
            let bar_len = (channel.message_count * 6 / max_count)
                .max(if channel.message_count > 0 { 1 } else { 0 });
            let bar = format!(
                "{}{}",
                "█".repeat(bar_len),
                "░".repeat(6usize.saturating_sub(bar_len))
            );
            spans.push(ratatui::text::Span::raw(" "));
            spans.push(ratatui::text::Span::styled(
                bar,
                Style::default().fg(Color::Cyan),
            ));
        }

        items.push(ListItem::new(Line::from(spans)));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    " Channels: {} by {} ({}) [O Sort, Enter Preview] ",
                    app.current_filter.label(),
                    app.channel_sort.label(),
                    channels.len()
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
    state.select(Some(app.channel_cursor.saturating_sub(start)));
    frame.render_stateful_widget(list, area, &mut state);
}

fn short_kind_label(kind: ChannelKind) -> &'static str {
    match kind {
        ChannelKind::Dm => "DM",
        ChannelKind::GroupDm => "GROUP",
        ChannelKind::PublicThread => "THREAD",
        ChannelKind::Voice => "VOICE",
        ChannelKind::Guild => "SERVER",
        ChannelKind::Other => "OTHER",
    }
}

fn draw_preview_pane(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let channels = filtered_channels(app);
    let selected = channels.get(app.channel_cursor);
    let base_title = match selected {
        Some(c) => format!(
            "Preview: {}",
            crate::data::utils::truncate_text(&c.title, 42)
        ),
        None => "Preview".to_owned(),
    };

    let loading_note = app.channel_preview_loading || selected.is_none();
    let stale = match selected {
        Some(c) => app.channel_preview_for.as_deref() != Some(c.dir_name.as_str()),
        None => true,
    };

    if loading_note || stale || app.channel_preview_lines.is_empty() {
        let msg = if loading_note || stale {
            let spinner = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']
                [(app.animation_tick as usize) % 10];
            format!("  {spinner} Loading recent messages...")
        } else {
            "  No messages found.".to_owned()
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(msg, Style::default().fg(Color::DarkGray)),
                Line::from(""),
                Line::styled(
                    "  Enter opens the full message view.",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {base_title} "))
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            area,
        );
        return;
    }

    let lines = render_preview_lines(&app.channel_preview_lines);
    let visible = area.height.saturating_sub(2) as usize;
    let bottom_offset = lines.len().saturating_sub(visible.max(1));
    // scroll counts how far the user paged up from the live tail.
    let offset = bottom_offset.saturating_sub(app.channel_preview_scroll);

    let scroll_indicator = if app.channel_preview_scroll == 0 {
        format!(" {base_title} [live tail] ")
    } else {
        format!(
            " {base_title} [-{} lines, , . Scroll] ",
            app.channel_preview_scroll
        )
    };

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(scroll_indicator)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .scroll((offset as u16, 0)),
        area,
    );
}

fn draw_stats_pane(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let channels = filtered_channels(app);
    let selected = channels.get(app.channel_cursor);

    let cached = selected.and_then(|c| {
        app.last_data
            .as_ref()
            .and_then(|d| d.channels_cache.get(&c.dir_name))
    });

    let mut lines: Vec<Line> = Vec::new();
    if let (Some(c), Some(s)) = (selected, cached) {
        lines.push(Line::from(vec![
            stat_span(" Total: "),
            stat_value(&fmt_num(c.message_count as u64)),
            stat_span("   Type: "),
            stat_value(short_kind_label(c.kind)),
        ]));
        lines.push(Line::from(vec![
            stat_span(" First: "),
            stat_value(s.temporal.first_message_date.as_deref().unwrap_or("n/a")),
            stat_span("   Last: "),
            stat_value(s.temporal.last_message_date.as_deref().unwrap_or("n/a")),
        ]));
        lines.push(Line::from(vec![
            stat_span(" Attachments: "),
            stat_value(&fmt_num(s.attachment_count)),
            stat_span("   Emoji: "),
            stat_value(&fmt_num(s.content.emoji_unicode + s.content.emoji_custom)),
        ]));
        lines.push(Line::from(vec![
            stat_span(" Avg msg: "),
            stat_value(&format!("{:.0} chars", s.content.avg_length_chars)),
            stat_span("   Words: "),
            stat_value(&top_words_inline(&s.content.top_words)),
        ]));
    } else if let Some(c) = selected {
        lines.push(Line::from(vec![
            stat_span(" Total: "),
            stat_value(&fmt_num(c.message_count as u64)),
            stat_span("   Type: "),
            stat_value(short_kind_label(c.kind)),
        ]));
        lines.push(Line::styled(
            "  Run Analyze Now for per-channel stats.",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        lines.push(Line::styled(
            "  No channel selected.",
            Style::default().fg(Color::DarkGray),
        ));
    }

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Stats ")
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn stat_span(text: &str) -> ratatui::text::Span<'static> {
    ratatui::text::Span::styled(text.to_owned(), Style::default().fg(Color::DarkGray))
}

fn stat_value(text: &str) -> ratatui::text::Span<'static> {
    ratatui::text::Span::styled(
        text.to_owned(),
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )
}

fn top_words_inline(words: &[(String, u64)]) -> String {
    if words.is_empty() {
        return "n/a".to_owned();
    }
    words
        .iter()
        .take(3)
        .map(|(w, _)| w.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
