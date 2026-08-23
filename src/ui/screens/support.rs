use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::{AppState, SupportActivityTab},
    data::{SupportTicketView, utils::truncate_text},
    ui::screens::activity::draw_activity_tabbed,
};

pub(crate) fn draw_support_activity(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);
    draw_support_activity_tabs(frame, app, rows[0]);

    match app.support_activity_tab {
        SupportActivityTab::Support => draw_support_tabbed(frame, app, rows[1]),
        SupportActivityTab::Activity => draw_activity_tabbed(frame, app, rows[1]),
        SupportActivityTab::Search => draw_support_search_tab(frame, app, rows[1]),
    }
}

fn draw_support_activity_tabs(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let tabs = [
        SupportActivityTab::Support,
        SupportActivityTab::Activity,
        SupportActivityTab::Search,
    ];
    let mut spans = Vec::new();
    for (idx, tab) in tabs.iter().enumerate() {
        let active = *tab == app.support_activity_tab;
        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        spans.push(Span::styled(format!(" {} ", tab.label()), style));
        if idx + 1 < tabs.len() {
            spans.push(Span::raw("  "));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Tabs ")
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn draw_support_tabbed(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let tickets: &[SupportTicketView] = app.support_tickets.as_deref().unwrap_or(&[]);
    if tickets.is_empty() {
        let message = if app.support_activity_loading {
            "  Loading support tickets in background...".to_owned()
        } else if let Some(err) = &app.support_tickets_failed {
            format!("  Support tickets failed to load: {err}")
        } else {
            "  No support tickets found (or not loaded yet).".to_owned()
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(message, Style::default().fg(if app.support_tickets_failed.is_some() {
                    Color::Red
                } else {
                    Color::Cyan
                })),
                Line::from(""),
                Line::styled(
                    "  Press r to reload from your export.",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::styled(
                    "  Press 2 to switch to Activity.",
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

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);
    draw_support_ticket_list(frame, app, tickets, cols[0]);
    draw_support_ticket_preview(frame, app, tickets, cols[1]);
}

fn draw_support_ticket_list(
    frame: &mut ratatui::Frame<'_>,
    app: &AppState,
    tickets: &[SupportTicketView],
    area: Rect,
) {
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
            "{idx:>4}  [{:<10}] {:<36}  {:<8}  c:{}",
            truncate_text(&ticket.status, 10),
            truncate_text(&ticket.subject, 36),
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
                    " Support Tickets: {} [↑↓ Select, Enter Detail] ",
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

fn draw_support_ticket_preview(
    frame: &mut ratatui::Frame<'_>,
    app: &AppState,
    tickets: &[SupportTicketView],
    area: Rect,
) {
    let ticket = tickets.get(app.support_ticket_cursor);
    let mut lines = Vec::new();
    if let Some(ticket) = ticket {
        lines.push(Line::styled(
            " Ticket Details",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        lines.push(Line::styled(
            format!(" Subject: {}", truncate_text(&ticket.subject, 80)),
            Style::default().fg(Color::White),
        ));
        lines.push(Line::styled(
            format!(" Status: {}", ticket.status),
            Style::default().fg(Color::Gray),
        ));
        lines.push(Line::styled(
            format!(" Priority: {}", ticket.priority),
            Style::default().fg(Color::Gray),
        ));
        lines.push(Line::styled(
            format!(" Created: {}", ticket.created_at),
            Style::default().fg(Color::DarkGray),
        ));
        lines.push(Line::styled(
            format!(" Updated: {}", ticket.updated_at),
            Style::default().fg(Color::DarkGray),
        ));
        lines.push(Line::from(""));
        lines.push(Line::styled(
            " Preview",
            Style::default().fg(Color::DarkGray),
        ));
        for line in ticket.detail_lines.iter().take(10) {
            lines.push(Line::styled(
                format!(" {}", truncate_text(line, 96)),
                Style::default().fg(Color::Gray),
            ));
        }
        lines.push(Line::from(""));
        lines.push(Line::styled(
            " Enter to open full ticket view.",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        lines.push(Line::from("No support ticket selected."));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Ticket Detail ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_support_search_tab(frame: &mut ratatui::Frame<'_>, _app: &AppState, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::styled(
                "  Search all messages across every channel.",
                Style::default().fg(Color::White),
            ),
            Line::from(""),
            Line::styled(
                "  Full-text search with type and content filters,",
                Style::default().fg(Color::DarkGray),
            ),
            Line::styled(
                "  streaming results and match highlighting.",
                Style::default().fg(Color::DarkGray),
            ),
            Line::from(""),
            Line::styled(
                "  Press Enter to open Message Search.",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search ")
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

pub(crate) fn draw_support_ticket_detail(
    frame: &mut ratatui::Frame<'_>,
    app: &AppState,
    area: Rect,
) {
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
