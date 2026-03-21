use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::AppState,
    data::{utils::truncate_text, SupportTicketView},
};

pub(crate) fn draw_support_activity(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let tickets: &[SupportTicketView] = app.support_tickets.as_deref().unwrap_or(&[]);
    if tickets.is_empty() {
        let message = if app.support_activity_loading {
            "  Loading support tickets in background..."
        } else {
            "  No support tickets found (or not loaded yet)."
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(message, Style::default().fg(Color::Cyan)),
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

pub(crate) fn draw_support_ticket_detail(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
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
