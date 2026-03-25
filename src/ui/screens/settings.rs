use ratatui::{
    layout::Rect,
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, List, ListItem, ListState},
};

use crate::app::AppState;

pub(crate) fn draw_settings(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let items = vec![
        ListItem::new(Line::from(vec![
            ratatui::text::Span::styled(
                " Auto-download attachments  ",
                Style::default().fg(Color::White),
            ),
            ratatui::text::Span::styled(
                if app.settings.download_attachments {
                    " ON "
                } else {
                    " OFF"
                },
                if app.settings.download_attachments {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray).bg(Color::Black)
                },
            ),
        ])),
        ListItem::new(Line::from(vec![
            ratatui::text::Span::styled(
                " Preview messages per channel  ",
                Style::default().fg(Color::White),
            ),
            ratatui::text::Span::styled(
                format!(" {} ", app.settings.preview_messages),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::styled("  ← → to adjust", Style::default().fg(Color::DarkGray)),
        ])),
        ListItem::new(Line::styled(
            " Reconfigure export / results / profile",
            Style::default().fg(Color::White),
        )),
        ListItem::new(Line::styled(" Back", Style::default().fg(Color::DarkGray))),
    ];

    let list = List::new(items)
        .block(Block::default().title(" Settings [↑↓ Select, ←→ Adjust, Enter Toggle] "))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.settings_cursor));
    frame.render_stateful_widget(list, area, &mut state);
}
