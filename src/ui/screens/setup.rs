// Setup screen

use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::{Alignment, Color, Modifier, Position, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::{
    app::{AppState, SetupStep},
    ui::components::{centered_rect, fit_input_for_box},
};

pub(crate) fn draw_setup(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        area,
    );

    let card = centered_rect(72, 88, area);
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

    let show_browse = matches!(
        app.setup.step,
        SetupStep::ExportPath | SetupStep::ResultsPath
    );

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
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

    let step_title = match app.setup.step {
        SetupStep::ExportPath => "Step 1 of 4: Where is your Discord export?",
        SetupStep::ResultsPath => "Step 2 of 4: Where should results be saved?",
        SetupStep::ProfileId => "Step 3 of 4: Profile ID (optional)",
        SetupStep::Confirm => "Step 4 of 4: Review and confirm",
    };
    frame.render_widget(
        Paragraph::new(Line::styled(
            step_title,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        sections[3],
    );

    if app.setup.step == SetupStep::Confirm {
        let summary = Paragraph::new(vec![
            Line::from(vec![
                ratatui::text::Span::styled("  Export:   ", Style::default().fg(Color::DarkGray)),
                ratatui::text::Span::styled(
                    &app.setup.export_path,
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                ratatui::text::Span::styled("  Results:  ", Style::default().fg(Color::DarkGray)),
                ratatui::text::Span::styled(
                    &app.setup.results_path,
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                ratatui::text::Span::styled("  Profile:  ", Style::default().fg(Color::DarkGray)),
                ratatui::text::Span::styled(
                    if app.setup.profile_id.is_empty() {
                        "(none)"
                    } else {
                        &app.setup.profile_id
                    },
                    Style::default().fg(Color::White),
                ),
            ]),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Summary ")
                .border_style(Style::default().fg(Color::Green)),
        );
        frame.render_widget(summary, sections[5]);
    } else {
        let border_color = if app.setup.browse_focus {
            Color::DarkGray
        } else {
            Color::Cyan
        };
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .border_style(Style::default().fg(border_color));
        let input_area = input_block.inner(sections[5]);
        frame.render_widget(input_block, sections[5]);

        let max_width = input_area.width.saturating_sub(1) as usize;
        let (display, cursor_offset) = fit_input_for_box(&app.setup.input, max_width);
        frame.render_widget(Paragraph::new(display), input_area);
        if !app.setup.browse_focus {
            frame.set_cursor_position(Position::new(
                input_area.x + cursor_offset as u16,
                input_area.y,
            ));
        }
    }

    let instruction = match app.setup.step {
        SetupStep::ExportPath => {
            "Paste the path to your extracted Discord data folder, or browse below."
        }
        SetupStep::ResultsPath => {
            "Press Enter to accept the default, or browse to choose a different location."
        }
        SetupStep::ProfileId => {
            "Optional: enter a profile ID if you have multiple exports. Leave empty to skip."
        }
        SetupStep::Confirm => {
            "Everything looks good. Press Enter to continue, or ← to go back and edit."
        }
    };
    frame.render_widget(
        Paragraph::new(instruction)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true }),
        sections[6],
    );

    if show_browse {
        draw_browse_panel(frame, app, sections[8]);
    } else if app.setup.step == SetupStep::Confirm {
        let confirm_area = sections[8];
        let centered_btn = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(34),
                Constraint::Fill(1),
            ])
            .split(confirm_area);

        let btn_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(3),
                Constraint::Fill(1),
            ])
            .split(centered_btn[1]);

        frame.render_widget(
            Paragraph::new(Line::styled(
                "  Press Enter to open Home  ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green)),
            ),
            btn_rows[1],
        );
    } else {
        let preview = Paragraph::new(vec![
            Line::from(""),
            Line::styled("  Export:  ", Style::default().fg(Color::DarkGray)),
            Line::styled(
                format!("  {}", app.setup.export_path),
                Style::default().fg(Color::White),
            ),
            Line::from(""),
            Line::styled("  Results: ", Style::default().fg(Color::DarkGray)),
            Line::styled(
                format!("  {}", app.setup.results_path),
                Style::default().fg(Color::White),
            ),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Current Paths ")
                .border_style(Style::default().fg(Color::DarkGray)),
        );
        frame.render_widget(preview, sections[8]);
    }

    let mut btn_spans = Vec::new();
    btn_spans.push(ratatui::text::Span::styled(
        "  [Esc] Cancel  ",
        Style::default().fg(Color::DarkGray),
    ));

    if current_step > 0 {
        btn_spans.push(ratatui::text::Span::styled(
            "  [←] Back  ",
            Style::default().fg(Color::Yellow),
        ));
    }

    if show_browse {
        let tab_label = if app.setup.browse_focus {
            "  [Tab] Input  "
        } else {
            "  [Tab] Browse  "
        };
        btn_spans.push(ratatui::text::Span::styled(
            tab_label,
            Style::default().fg(Color::Cyan),
        ));
    }

    let next_label = if app.setup.step == SetupStep::Confirm {
        "  [Enter] Finish →  "
    } else {
        "  [Enter] Next →  "
    };
    btn_spans.push(ratatui::text::Span::styled(
        next_label,
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    ));

    frame.render_widget(
        Paragraph::new(Line::from(btn_spans)).alignment(Alignment::Center),
        sections[10],
    );

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
        sections[12],
    );
}

fn draw_browse_panel(frame: &mut ratatui::Frame<'_>, app: &AppState, area: ratatui::layout::Rect) {
    let entries = &app.setup.browse_entries;
    let border_color = if app.setup.browse_focus {
        Color::Cyan
    } else {
        Color::DarkGray
    };

    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(
                    "  No directories found at this path.",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::styled(
                    "  Type or paste a valid directory path above.",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Browse ")
                    .border_style(Style::default().fg(border_color)),
            ),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = entries
        .iter()
        .map(|e| {
            ListItem::new(Line::styled(
                format!("  {}", e.name),
                Style::default().fg(Color::White),
            ))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Browse ")
                .border_style(Style::default().fg(border_color)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");

    let mut state = ListState::default();
    if app.setup.browse_focus {
        state.select(Some(
            app.setup.browse_cursor.min(entries.len().saturating_sub(1)),
        ));
    }
    frame.render_stateful_widget(list, area, &mut state);
}
