// Analysis progress and setup screens

use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::{Alignment, Color, Modifier, Position, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Gauge, Paragraph, Wrap},
};

use crate::{
    app::{AppState, SetupStep},
    analyzer::AnalysisStep,
    ui::components::{centered_rect, fit_input_for_box},
};

pub(crate) fn draw_analyzing(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    // Dim the background
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );
    
    // Modal card - slightly larger to fit the checklist
    let card = centered_rect(80, 80, area);
    frame.render_widget(Clear, card);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Analysis Engine  ")
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
            Constraint::Length(3), // Title
            Constraint::Length(3), // Main Gauge
            Constraint::Length(2), // Current Step Label
            Constraint::Min(2),    // Status detail
            Constraint::Length(10), // Checklist
            Constraint::Length(3),  // Buttons
        ])
        .split(inner);

    // 1. Title
    frame.render_widget(
        Paragraph::new(Line::styled(
            "Analyzing your Discord data...",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        sections[0],
    );

    // 2. Main Gauge
    let pct = app.analysis_progress * 100.0;
    let gauge = Gauge::default()
        .gauge_style(
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(app.analysis_progress as f64)
        .label(format!("{pct:>5.1}%"));
    frame.render_widget(gauge, sections[1]);

    // 3. Current Step
    let step_num = match app.analysis_step {
        AnalysisStep::Preparing => 1,
        AnalysisStep::Account => 2,
        AnalysisStep::Messages => 3,
        AnalysisStep::Servers => 4,
        AnalysisStep::Support => 5,
        AnalysisStep::Activity => 6,
        AnalysisStep::Activities => 7,
        AnalysisStep::Programs => 8,
        AnalysisStep::Writing => 9,
        AnalysisStep::Complete => 9,
    };
    frame.render_widget(
        Paragraph::new(Line::styled(
            format!("Step {step_num} of 9: {}...", app.analysis_step.label()),
            Style::default().fg(Color::White),
        ))
        .alignment(Alignment::Center),
        sections[2],
    );

    // 4. Status Detail
    frame.render_widget(
        Paragraph::new(Line::styled(
            format!("  {}", app.status),
            Style::default().fg(Color::Gray),
        ))
        .wrap(Wrap { trim: true }),
        sections[3],
    );

    // 5. Checklist
    let steps = [
        (AnalysisStep::Account, "Account"),
        (AnalysisStep::Messages, "Messages"),
        (AnalysisStep::Servers, "Servers"),
        (AnalysisStep::Support, "Support"),
        (AnalysisStep::Activity, "Activity"),
        (AnalysisStep::Activities, "Activities"),
        (AnalysisStep::Programs, "Programs"),
        (AnalysisStep::Writing, "Writing"),
    ];

    let mut checklist_lines = Vec::new();
    let current_step_idx = match app.analysis_step {
        AnalysisStep::Preparing => 0,
        AnalysisStep::Account => 0,
        AnalysisStep::Messages => 1,
        AnalysisStep::Servers => 2,
        AnalysisStep::Support => 3,
        AnalysisStep::Activity => 4,
        AnalysisStep::Activities => 5,
        AnalysisStep::Programs => 6,
        AnalysisStep::Writing => 7,
        AnalysisStep::Complete => 8,
    };

    for (i, (_step, label)) in steps.iter().enumerate() {
        let (icon, style) = if i < current_step_idx {
            ("✓", Style::default().fg(Color::Green))
        } else if i == current_step_idx {
            (
                "●",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("○", Style::default().fg(Color::DarkGray))
        };

        let mut row = vec![
            ratatui::text::Span::styled(format!("  {} {:<12} ", icon, label), style),
        ];

        let bar_width: usize = 30;
        if i == current_step_idx {
            // Estimate progress within the current step (roughly)
            // Steps 1..=8 correspond to i 0..=7 (Account..Writing)
            let step_start = (i + 1) as f32 / 9.0;
            let step_end = (i + 2) as f32 / 9.0;
            let step_progress = if app.analysis_progress > step_start {
                ((app.analysis_progress - step_start) / (step_end - step_start)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            
            let filled = (step_progress * bar_width as f32) as usize;
            let bar = format!(
                "{}{}",
                "█".repeat(filled),
                "░".repeat(bar_width.saturating_sub(filled))
            );
            row.push(ratatui::text::Span::styled(bar, style));
        } else if i < current_step_idx {
            row.push(ratatui::text::Span::styled(
                "█".repeat(bar_width),
                Style::default().fg(Color::Green),
            ));
        } else {
            row.push(ratatui::text::Span::styled(
                "░".repeat(bar_width),
                Style::default().fg(Color::DarkGray),
            ));
        }

        checklist_lines.push(Line::from(row));
    }

    // Center the checklist horizontally
    let checklist_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(2),
            Constraint::Length(55),
            Constraint::Min(2),
        ])
        .split(sections[4]);

    frame.render_widget(
        Paragraph::new(checklist_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Progress ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .wrap(Wrap { trim: false }),
        checklist_cols[1],
    );
    // 6. Buttons
    let buttons = Line::from(vec![
        ratatui::text::Span::styled("  [Run in Background]  ", Style::default().fg(Color::Cyan)),
        ratatui::text::Span::styled("  [Cancel]  ", Style::default().fg(Color::Red)),
    ]);
    frame.render_widget(
        Paragraph::new(buttons).alignment(Alignment::Center),
        sections[5],
    );
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
