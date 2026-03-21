use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::{AppState, HOME_MENU_ITEMS, fmt_num, home_item_disabled_reason, ratio},
    ui::components::stat_line,
};

pub(crate) fn draw_home(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(32), Constraint::Length(36)])
        .split(area);

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


