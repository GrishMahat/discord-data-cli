use std::time::Duration;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::{Alignment, Color, Modifier, Position, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Gauge, Paragraph, Wrap},
};

use crate::{
    app::{AppState, SetupStep, format_duration},
    ui::components::{centered_rect, fit_input_for_box},
};

pub(crate) fn draw_analyzing(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );
    let card = centered_rect(64, 50, area);
    frame.render_widget(Clear, card);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Analyzing Discord Export  ")
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);

    let spinner_frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let spinner = spinner_frames[((app.animation_tick / 2) % spinner_frames.len() as u64) as usize];
    let elapsed = app
        .analysis_started_at
        .map(|s| s.elapsed())
        .unwrap_or_default();

    let text_lines = vec![
        Line::from(""),
        Line::styled(
            format!(" {spinner} Analyzing your Discord data..."),
            Style::default()
                .fg(Color::Cyan)
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

    let pct = app.analysis_progress * 100.0;
    let label = format!("  {pct:>5.1}%  elapsed: {}", format_duration(elapsed));
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(app.analysis_progress as f64)
        .label(label);
    frame.render_widget(gauge, sections[1]);

    let eta_line = if app.analysis_progress > 0.02 {
        let rate = app.analysis_progress as f64 / elapsed.as_secs_f64().max(0.001);
        let remaining = ((1.0 - app.analysis_progress as f64) / rate) as u64;
        Line::styled(
            format!("  ETA ~{}", format_duration(Duration::from_secs(remaining))),
            Style::default().fg(Color::DarkGray),
        )
    } else {
        Line::styled("  Calculating ETA...", Style::default().fg(Color::DarkGray))
    };
    frame.render_widget(Paragraph::new(eta_line), sections[2]);
}

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

pub(crate) fn draw_setup(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );

    let card = centered_rect(72, 80, area);
    frame.render_widget(Clear, card);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Setup  ")
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(2),
        ])
        .split(inner);

    let title = Paragraph::new(Line::styled(
        "Discord Data Analyzer",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, sections[0]);

    let steps = ["Export Path", "Results Dir", "Profile ID", "Confirm"];
    let current_step = match app.setup.step {
        SetupStep::ExportPath => 0,
        SetupStep::ResultsPath => 1,
        SetupStep::ProfileId => 2,
        SetupStep::Confirm => 3,
    };
    let mut dot_spans = Vec::new();
    for (i, name) in steps.iter().enumerate() {
        let (dot, style) = if i < current_step {
            ("●", Style::default().fg(Color::Green))
        } else if i == current_step {
            (
                "●",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("○", Style::default().fg(Color::DarkGray))
        };
        dot_spans.push(ratatui::text::Span::styled(format!("{dot} {name}"), style));
        if i < steps.len() - 1 {
            dot_spans.push(ratatui::text::Span::styled(
                "  ──  ",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    frame.render_widget(
        Paragraph::new(Line::from(dot_spans)).alignment(Alignment::Center),
        sections[2],
    );

    let instruction = match app.setup.step {
        SetupStep::ExportPath => {
            "Paste the full path to your extracted Discord data folder.\nThe directory must already exist."
        }
        SetupStep::ResultsPath => {
            "Where should results be saved?\nPress Enter to accept the default (inside your export folder)."
        }
        SetupStep::ProfileId => {
            "Optional: enter a profile ID if you have multiple exports.\nLeave empty and press Enter to skip."
        }
        SetupStep::Confirm => {
            "Everything looks good.\nPress Enter to continue, or Left/Up to go back and edit."
        }
    };
    frame.render_widget(
        Paragraph::new(instruction)
            .style(Style::default().fg(Color::Gray))
            .wrap(Wrap { trim: true }),
        sections[3],
    );

    let preview = Paragraph::new(vec![
        Line::from(vec![
            ratatui::text::Span::styled("Export:  ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(&app.setup.export_path, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            ratatui::text::Span::styled("Results: ", Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(&app.setup.results_path, Style::default().fg(Color::White)),
        ]),
    ])
    .block(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(preview, sections[4]);

    if app.setup.step == SetupStep::Confirm {
        let confirm = Paragraph::new(Line::styled(
            "  Press Enter to open Home  ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(confirm, sections[5]);
    } else {
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .border_style(Style::default().fg(Color::Cyan));
        let input_area = input_block.inner(sections[5]);
        frame.render_widget(input_block, sections[5]);

        let max_width = input_area.width.saturating_sub(1) as usize;
        let (display, cursor_offset) = fit_input_for_box(&app.setup.input, max_width);
        frame.render_widget(Paragraph::new(display), input_area);
        frame.set_cursor_position(Position::new(
            input_area.x + cursor_offset as u16,
            input_area.y,
        ));
    }

    let (notice_text, notice_color) = if app.setup.notice.to_lowercase().contains("error")
        || app.setup.notice.to_lowercase().contains("not found")
        || app.setup.notice.to_lowercase().contains("required")
    {
        (format!("  {}", app.setup.notice), Color::Red)
    } else {
        (format!("  {}", app.setup.notice), Color::Yellow)
    };
    frame.render_widget(
        Paragraph::new(notice_text)
            .style(Style::default().fg(notice_color))
            .wrap(Wrap { trim: true }),
        sections[6],
    );
}
