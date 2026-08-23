use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::AppState;

/// Layout mirror shared with the input handler. Returns the four section rects.
pub(crate) fn settings_layout(area: Rect) -> [Rect; 4] {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7), // General (3 fields + borders + padding)
            Constraint::Length(4), // Display (1 field)
            Constraint::Length(4), // Downloads (1 field)
            Constraint::Min(3),    // Privacy note
        ])
        .split(area);
    [rows[0], rows[1], rows[2], rows[3]]
}

/// Map a click position to a logical settings field (None if outside a field).
pub(crate) fn field_at(x: u16, y: u16, area: Rect) -> Option<usize> {
    let sections = settings_layout(area);

    // General section rows (inner): export=0, results=1, profile=2
    if crate::input::rect_contains(sections[0], x, y) && sections[0].height >= 5 {
        let rel = y.saturating_sub(sections[0].y + 1);
        return Some(match rel {
            0 => 0,
            1 => 1,
            _ => 2,
        });
    }
    if crate::input::rect_contains(sections[1], x, y) {
        return Some(3);
    }
    if crate::input::rect_contains(sections[2], x, y) {
        return Some(4);
    }
    None
}

pub(crate) fn draw_settings(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let sections = settings_layout(area);

    draw_general_section(frame, app, sections[0]);
    draw_display_section(frame, app, sections[1]);
    draw_downloads_section(frame, app, sections[2]);
    draw_privacy_section(frame, app, sections[3]);
}

fn sel_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    }
}

fn hint_span(text: &str) -> Span<'static> {
    Span::styled(text.to_owned(), Style::default().fg(Color::DarkGray))
}

fn value_span(text: &str) -> Span<'static> {
    Span::styled(
        text.to_owned(),
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )
}

fn truncate_path(path: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= width {
        return path.to_owned();
    }
    format!("…{}", chars[chars.len() - width + 1..].iter().collect::<String>())
}

fn draw_general_section(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let export = app.config.package_path(&app.config_path, &app.id).display().to_string();
    let results = app.config.results_path(&app.config_path, &app.id).display().to_string();
    let profile = if app.id.is_empty() {
        "(none)".to_owned()
    } else {
        app.id.clone()
    };

    let value_width = area.width.saturating_sub(46).max(8) as usize;

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{:<14}", "Export Path"),
                sel_style(app.settings_cursor == 0),
            ),
            Span::raw(" "),
            value_span(&truncate_path(&export, value_width)),
            hint_span("  Enter to edit"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{:<14}", "Results Path"),
                sel_style(app.settings_cursor == 1),
            ),
            Span::raw(" "),
            value_span(&truncate_path(&results, value_width)),
            hint_span("  Enter to edit"),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{:<14}", "Profile ID"),
                sel_style(app.settings_cursor == 2),
            ),
            Span::raw(" "),
            value_span(&profile),
            hint_span("  Enter to edit"),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" General ")
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        area,
    );
}

fn draw_display_section(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let selected = app.settings_cursor == 3;
    let lines = vec![Line::from(vec![
        Span::styled(
            format!("{:<14}", "Preview Msgs"),
            sel_style(selected),
        ),
        Span::raw(" "),
        value_span(&format!("{}", app.settings.preview_messages)),
        hint_span("  ← → adjust (5-500), also drives channel preview pane"),
    ])];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Display ")
                .border_style(Style::default().fg(if selected {
                    Color::Cyan
                } else {
                    Color::DarkGray
                })),
        ),
        area,
    );
}

fn draw_downloads_section(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let selected = app.settings_cursor == 4;
    let on = app.settings.download_attachments;
    let toggle_style = if on {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let lines = vec![Line::from(vec![
        Span::styled(
            format!("{:<14}", "Auto-download"),
            sel_style(selected),
        ),
        Span::raw(" "),
        Span::styled(if on { " ON " } else { " OFF" }, toggle_style),
        hint_span("  after analysis finishes, download attachments"),
    ])];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Downloads ")
                .border_style(Style::default().fg(if selected {
                    Color::Cyan
                } else {
                    Color::DarkGray
                })),
        ),
        area,
    );
}

fn draw_privacy_section(frame: &mut ratatui::Frame<'_>, _app: &AppState, area: Rect) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(" Your data stays on this computer."),
            Line::from(" Nothing is uploaded anywhere — the analyzer only reads your local Discord export."),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Privacy ")
                .border_style(Style::default().fg(Color::Green)),
        ),
        area,
    );
}
