use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::AppState;
use discord_data_cli_core::compare::diff_snapshots;

pub(crate) fn draw_compare(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let snapshots = app.compare_snapshots();
    if snapshots.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled("  No snapshots yet.", Style::default().fg(Color::DarkGray)),
                Line::from(""),
                Line::styled(
                    "  Every analysis saves a snapshot of your stats.",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::styled(
                    "  Run 'Analyze Now' again after a fresh Discord export",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::styled(
                    "  and the differences will show up here.",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Compare Exports ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            area,
        );
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
        .split(area);

    draw_snapshot_list(frame, app, &snapshots, cols[0]);
    draw_diff(frame, app, &snapshots, cols[1]);
}

fn snapshot_label(messages_total: u64, taken_at: &str) -> String {
    let date = taken_at.chars().take(10).collect::<String>();
    format!("{date}  {:>6} msgs", crate::app::fmt_num(messages_total))
}

fn draw_snapshot_list(
    frame: &mut ratatui::Frame<'_>,
    app: &AppState,
    snapshots: &[(String, discord_data_cli_core::compare::Snapshot)],
    area: Rect,
) {
    let mut items: Vec<ListItem> = Vec::new();
    for (_stem, snap) in snapshots.iter() {
        items.push(ListItem::new(Line::from(vec![Span::styled(
            format!(" {} ", snapshot_label(snap.messages_total, &snap.taken_at)),
            Style::default().fg(Color::White),
        )])));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Snapshots ({}) [↑↓ Select] ", snapshots.len()))
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
    state.select(Some(app.compare_cursor.min(snapshots.len() - 1)));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_diff(
    frame: &mut ratatui::Frame<'_>,
    app: &AppState,
    snapshots: &[(String, discord_data_cli_core::compare::Snapshot)],
    area: Rect,
) {
    let sel = app.compare_cursor.min(snapshots.len() - 1);
    let (_, current) = &snapshots[sel];

    let lines: Vec<Line> = match snapshots.get(sel + 1) {
        Some((_, older)) => {
            let rows = diff_snapshots(older, current);
            let mut lines = vec![
                Line::from(vec![
                    Span::styled(" comparing ", dim()),
                    Span::styled(
                        format!(
                            "{} -> {}",
                            older.taken_at.chars().take(10).collect::<String>(),
                            current.taken_at.chars().take(10).collect::<String>()
                        ),
                        cyan_bold(),
                    ),
                ]),
                Line::from(""),
            ];
            for r in rows {
                let delta_span = match r.trend {
                    Some(true) => Span::styled(format!(" {}", r.delta), green()),
                    Some(false) => Span::styled(format!(" {}", r.delta), red()),
                    None => Span::styled(format!(" {}", r.delta), dim()),
                };
                lines.push(Line::from(vec![
                    Span::styled(format!(" {:<18}", r.label), dim()),
                    Span::styled(format!("{:>10}", r.old), gray()),
                    Span::styled("  →  ", dim()),
                    Span::styled(format!("{:>10}", r.new), white()),
                    delta_span,
                ]));
            }
            lines
        }
        None => vec![
            Line::from(""),
            Line::styled(
                "  This is the oldest snapshot — nothing older to compare with.",
                Style::default().fg(Color::DarkGray),
            ),
            Line::from(""),
            Line::styled(
                format!("  Snapshot taken: {}", current.taken_at),
                Style::default().fg(Color::DarkGray),
            ),
        ],
    };

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" What Changed ")
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}
fn gray() -> Style {
    Style::default().fg(Color::Gray)
}
fn white() -> Style {
    Style::default().fg(Color::White)
}
fn green() -> Style {
    Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD)
}
fn red() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}
fn cyan_bold() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}
