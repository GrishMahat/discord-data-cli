use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::{ActivityFilterField, AppState, filtered_activity_events, fmt_num, ratio, top_counts},
    data::utils::truncate_text,
    ui::components::stat_line,
};

pub(crate) fn draw_activity(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
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
        let message = if app.activity_loading {
            "  Loading logs..."
        } else {
            "  No matches."
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(message, Style::default().fg(Color::Cyan)),
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

pub(crate) fn draw_activity_detail(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
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

pub(crate) fn draw_activity_quick_stats(
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
    if !app.activity_filters.query.is_empty() {
        count += 1;
    }
    if !app.activity_filters.event_type.is_empty() {
        count += 1;
    }
    if !app.activity_filters.source_file.is_empty() {
        count += 1;
    }
    if !app.activity_filters.from_date.is_empty() {
        count += 1;
    }
    if !app.activity_filters.to_date.is_empty() {
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
