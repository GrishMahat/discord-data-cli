use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Gauge, Paragraph, Wrap},
};

use crate::{
    app::AppState,
    ui::components::centered_rect,
};

pub(crate) fn draw_downloading(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );
    let card = centered_rect(64, 50, area);
    frame.render_widget(Clear, card);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Downloading Attachments  ")
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(3)])
        .split(inner);

    let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner = spinner_frames[((app.animation_tick / 2) % spinner_frames.len() as u64) as usize];

    let text_lines = vec![
        Line::from(""),
        Line::styled(
            format!(" {spinner} Downloading media and attachments..."),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::styled(
            format!("    {}", app.status),
            Style::default().fg(Color::Gray),
        ),
    ];
    frame.render_widget(
        Paragraph::new(text_lines).wrap(Wrap { trim: true }),
        sections[0],
    );

    let pct = app.download_progress * 100.0;
    let label = format!("  {pct:>5.1}%");
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(app.download_progress as f64)
        .label(label);
    frame.render_widget(gauge, sections[1]);
}
