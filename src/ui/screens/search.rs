use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::app::{AppState, ChannelFilter, SearchField};
use crate::ui::components::fit_input_for_box;

/// Layout mirror shared with the mouse handler.
pub(crate) fn search_layout(area: Rect) -> (Rect, Rect, Rect, Rect, Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(4),
        ])
        .split(area);
    let filter_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Length(26),
            Constraint::Min(10),
        ])
        .split(rows[1]);
    (
        rows[0],
        filter_cols[0],
        filter_cols[1],
        filter_cols[2],
        rows[2],
    )
}

pub(crate) fn draw_search(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let (input_area, type_area, has_area, progress_area, results_area) = search_layout(area);

    draw_query_input(frame, app, input_area);
    draw_type_filter(frame, app, type_area);
    draw_has_filter(frame, app, has_area);
    draw_progress(frame, app, progress_area);
    draw_results(frame, app, results_area);
}

fn focused_border(app: &AppState, field: SearchField) -> Style {
    if app.search.field == field {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn pill(label: &str, active: bool) -> Span<'static> {
    if active {
        Span::styled(
            format!(" {label} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(format!(" {label} "), Style::default().fg(Color::Gray))
    }
}

fn draw_query_input(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let inner_w = area.width.saturating_sub(4) as usize;
    let (display, _) = fit_input_for_box(&app.search.input, inner_w.saturating_sub(2));
    let cursor = if app.search.field == SearchField::Input {
        "█"
    } else {
        " "
    };

    let title = if app.search.field == SearchField::Input {
        " Search [type · Enter run · Shift+T refocus] "
    } else {
        " Search [Shift+T refocus] "
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::styled(display, Style::default().fg(Color::White)),
            Span::styled(cursor, Style::default().fg(Color::Cyan)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(focused_border(app, SearchField::Input)),
        ),
        area,
    );
}

fn draw_type_filter(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let tabs = [
        ("All", ChannelFilter::All),
        ("DMs", ChannelFilter::Dm),
        ("Groups", ChannelFilter::GroupDm),
        ("Threads", ChannelFilter::PublicThread),
    ];
    let mut spans = vec![Span::styled("In:", Style::default().fg(Color::DarkGray))];
    for (label, filter) in tabs {
        spans.push(pill(
            label,
            app.search.type_filter == filter,
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(focused_border(app, SearchField::TypeFilter)),
        ),
        area,
    );
}

fn draw_has_filter(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let mut spans = vec![Span::styled("Has:", Style::default().fg(Color::DarkGray))];
    let options = [
        crate::app::SearchHasFilter::Any,
        crate::app::SearchHasFilter::Attachments,
        crate::app::SearchHasFilter::Links,
    ];
    for option in options {
        spans.push(pill(option.label(), app.search.has_filter == option));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(focused_border(app, SearchField::HasFilter)),
        ),
        area,
    );
}

fn draw_progress(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let text = if app.search.running {
        let spinner = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']
            [(app.animation_tick as usize) % 10];
        format!(
            "{spinner} scanning {} / {} files",
            app.search.scanned_files, app.search.total_files
        )
    } else if app.search.input.trim().is_empty() {
        "Type a query, press Enter".to_owned()
    } else {
        format!(
            "{} match{}{}",
            app.search.total_matches,
            if app.search.total_matches == 1 { "" } else { "es" },
            if app.search.truncated { " (capped)" } else { "" }
        )
    };
    let color = if app.search.running {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {text}"),
            Style::default().fg(color),
        )))
        .wrap(ratatui::widgets::Wrap { trim: true }),
        area,
    );
}

fn draw_results(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    if app.search.results.is_empty() {
        let msg = if app.search.running {
            "  Searching... results stream in as they are found."
        } else if app.search.input.trim().is_empty() {
            "  Type a query above and press Enter to search all messages."
        } else {
            "  No matches found."
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(msg, Style::default().fg(Color::DarkGray)),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Results ")
                    .border_style(focused_border(app, SearchField::Results)),
            ),
            area,
        );
        return;
    }

    let needle = app.search.input.trim().to_lowercase();
    let visible_rows = area.height.saturating_sub(2) as usize;
    let page_size = visible_rows.max(1);
    let start = app
        .search
        .cursor
        .saturating_sub(page_size / 2)
        .min(app.search.results.len().saturating_sub(page_size));
    let end = (start + page_size).min(app.search.results.len());

    let mut items = Vec::new();
    for (local_idx, result) in app.search.results[start..end].iter().enumerate() {
        let idx = start + local_idx + 1;
        let mut spans = vec![
            Span::styled(format!("{idx:>5} "), Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:<8} ", short_kind(result.kind)),
                Style::default().fg(crate::ui::screens::messages::kind_color(result.kind)),
            ),
            Span::styled(
                format!(
                    "{:<26} ",
                    crate::data::utils::truncate_text(&result.title, 26)
                ),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                format!("{:<19} ", crate::data::utils::truncate_text(&result.timestamp, 19)),
                Style::default().fg(Color::Blue),
            ),
        ];
        append_snippet_spans(&mut spans, &result.content, &needle);
        items.push(ListItem::new(Line::from(spans)));
    }

    let status = if app.search.running {
        format!(" Results: {}+ (searching) ", app.search.results.len())
    } else {
        format!(" Results: {} ", app.search.results.len())
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(status)
                .border_style(focused_border(app, SearchField::Results)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.search.cursor.saturating_sub(start)));
    frame.render_stateful_widget(list, area, &mut state);
}

fn short_kind(kind: crate::data::ChannelKind) -> &'static str {
    use crate::data::ChannelKind;
    match kind {
        ChannelKind::Dm => "DM",
        ChannelKind::GroupDm => "GROUP",
        ChannelKind::PublicThread => "THREAD",
        ChannelKind::Voice => "VOICE",
        ChannelKind::Guild => "SERVER",
        ChannelKind::Other => "OTHER",
    }
}

/// Append a one-line snippet of `content` around the first case-insensitive
/// occurrence of `needle`, with the match visually highlighted.
fn append_snippet_spans(spans: &mut Vec<Span<'static>>, content: &str, needle: &str) {
    const WINDOW: usize = 60;

    // Per-char lowercase keeps a 1:1 index mapping onto the original string
    // (str::to_lowercase can change char counts, e.g. 'İ' or 'ﬀ', which
    // would make match indices drift when slicing the original text).
    let lower_chars: Vec<char> = content
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let lower_needle: Vec<char> = needle
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect();
    let chars: Vec<char> = content.chars().collect();

    let mut match_pos: Option<usize> = None;
    if !lower_needle.is_empty() && lower_chars.len() >= lower_needle.len() {
        for i in 0..=(lower_chars.len() - lower_needle.len()) {
            if lower_chars[i..i + lower_needle.len()] == lower_needle[..] {
                match_pos = Some(i);
                break;
            }
        }
    }

    let Some(pos) = match_pos else {
        // No visible match (e.g. attachment-only match): just truncate.
        spans.push(Span::styled(
            crate::data::utils::truncate_text(content, WINDOW * 2),
            Style::default().fg(Color::Gray),
        ));
        return;
    };

    // to_lowercase() can change char counts (e.g. 'İ'); clamp so slicing
    // the original text stays in bounds.
    let max_pos = chars.len().saturating_sub(lower_needle.len());
    let pos = pos.min(max_pos);
    let ctx = WINDOW.saturating_sub(lower_needle.len()) / 2;
    let start = pos.saturating_sub(ctx);
    let end = (pos + lower_needle.len() + ctx).min(chars.len());
    let prefix: String = chars[start..pos].iter().collect();
    let matched: String = chars[pos..pos + lower_needle.len()].iter().collect();
    let suffix: String = chars[end.min(chars.len())..].iter().collect();

    if start > 0 {
        spans.push(Span::styled("…", Style::default().fg(Color::DarkGray)));
    }
    spans.push(Span::styled(prefix, Style::default().fg(Color::Gray)));
    spans.push(Span::styled(
        matched,
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(suffix, Style::default().fg(Color::Gray)));
    if end < chars.len() {
        spans.push(Span::styled("…", Style::default().fg(Color::DarkGray)));
    }
}
