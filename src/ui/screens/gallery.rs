use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::{
    app::{AppState, filtered_gallery_files},
    data::utils::truncate_text,
};

pub(crate) fn draw_gallery(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let files = filtered_gallery_files(app);
    let categories = [
        (None, "All"),
        (Some("imgs"), "Images"),
        (Some("vids"), "Videos"),
        (Some("audios"), "Audio"),
        (Some("docs"), "Docs"),
        (Some("txts"), "Text"),
        (Some("codes"), "Code"),
        (Some("zips"), "Archives"),
        (None, "Others"),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(5)])
        .split(area);

    let mut tab_spans = Vec::new();
    for (i, (opt, label)) in categories.iter().enumerate() {
        let active = match (&app.gallery.category_filter, opt) {
            (None, None) => i == 0,
            (Some(f), Some(o)) => f == o,
            _ => false,
        };
        let (num_key, label_text) = if i < 9 {
            (format!("{}", i + 1), *label)
        } else {
            ("?".to_owned(), *label)
        };

        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        tab_spans.push(ratatui::text::Span::styled(
            format!(" {num_key}:{label_text} "),
            style,
        ));
        if i < categories.len() - 1 {
            tab_spans.push(ratatui::text::Span::raw(" "));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), chunks[0]);

    if files.is_empty() {
        frame.render_widget(
            Paragraph::new("\n  No files found in this category.\n  Make sure you have downloaded attachments first.")
                .block(Block::default().borders(Borders::ALL).title(" Gallery ")
                .border_style(Style::default().fg(Color::Cyan))),
            chunks[1]
        );
        return;
    }

    let visible_rows = chunks[1].height.saturating_sub(2) as usize;
    let page_size = visible_rows.max(1);
    let start = app
        .gallery
        .cursor
        .saturating_sub(page_size / 2)
        .min(files.len().saturating_sub(page_size));
    let end = (start + page_size).min(files.len());

    let mut items = Vec::new();
    for (local_idx, file) in files[start..end].iter().enumerate() {
        let idx = start + local_idx + 1;
        let cat_color = match file.category.as_str() {
            "imgs" => Color::Green,
            "vids" => Color::Yellow,
            "audios" => Color::Magenta,
            "docs" => Color::Blue,
            "codes" | "txts" => Color::Cyan,
            "zips" | "exes" => Color::Red,
            _ => Color::DarkGray,
        };

        let file_info = Line::from(vec![
            ratatui::text::Span::styled(format!("{idx:>4} "), Style::default().fg(Color::DarkGray)),
            ratatui::text::Span::styled(
                format!("{:<10} ", file.category),
                Style::default().fg(cat_color),
            ),
            ratatui::text::Span::styled(
                truncate_text(&file.name, 45),
                Style::default().fg(Color::White),
            ),
            ratatui::text::Span::raw("  "),
            ratatui::text::Span::styled(
                format_size(file.size),
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        items.push(ListItem::new(file_info));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(
                    " Gallery: {} files discovered | [Enter] Open ",
                    files.len()
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
    state.select(Some(app.gallery.cursor.saturating_sub(start)));
    frame.render_stateful_widget(list, chunks[1], &mut state);
}

fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{} B", size)
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
