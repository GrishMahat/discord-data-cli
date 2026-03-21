use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::{
    AppState, Screen, filtered_gallery_files, switch_gallery_filter,
};
use crate::input::rect_contains;

pub(crate) fn handle_gallery_key(app: &mut AppState, key: KeyEvent) {
    let files = filtered_gallery_files(app);
    let count = files.len();

    match key.code {
        KeyCode::Up
        | KeyCode::Char('w')
        | KeyCode::Char('W')
        | KeyCode::Char('k')
        | KeyCode::Char('K') => {
            app.gallery.cursor = app.gallery.cursor.saturating_sub(1);
        }
        KeyCode::Down
        | KeyCode::Char('s')
        | KeyCode::Char('S')
        | KeyCode::Char('j')
        | KeyCode::Char('J') => {
            if app.gallery.cursor + 1 < count {
                app.gallery.cursor += 1;
            }
        }
        KeyCode::PageUp | KeyCode::Char('u') | KeyCode::Char('U') => {
            app.gallery.cursor = app.gallery.cursor.saturating_sub(20);
        }
        KeyCode::PageDown | KeyCode::Char('d') | KeyCode::Char('D') => {
            app.gallery.cursor = (app.gallery.cursor + 20).min(count.saturating_sub(1));
        }
        KeyCode::Char('1') => switch_gallery_filter(app, None),
        KeyCode::Char('2') => switch_gallery_filter(app, Some("imgs".to_owned())),
        KeyCode::Char('3') => switch_gallery_filter(app, Some("vids".to_owned())),
        KeyCode::Char('4') => switch_gallery_filter(app, Some("audios".to_owned())),
        KeyCode::Char('5') => switch_gallery_filter(app, Some("docs".to_owned())),
        KeyCode::Char('6') => switch_gallery_filter(app, Some("txts".to_owned())),
        KeyCode::Char('7') => switch_gallery_filter(app, Some("codes".to_owned())),
        KeyCode::Char('8') => switch_gallery_filter(app, Some("zips".to_owned())),
        KeyCode::Char('9') => switch_gallery_filter(app, Some("unknowns".to_owned())),
        KeyCode::Enter => {
            if count > 0 && app.gallery.cursor < count {
                let file = &files[app.gallery.cursor];
                if let Err(e) = open::that(&file._path) {
                    app.error = Some(format!("Failed to open file: {}", e));
                } else {
                    app.status = format!("Opened: {}", file.name);
                }
            }
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        _ => {}
    }
}

pub(crate) fn handle_gallery_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    let files = filtered_gallery_files(app);
    let count = files.len();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(5)])
        .split(area);

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if rect_contains(chunks[0], mouse.column, mouse.row) && chunks[0].width > 0 {
                let rel_x = mouse.column.saturating_sub(chunks[0].x) as usize;
                // Roughly 9 categories, spaced
                let idx = (rel_x * 9) / chunks[0].width as usize;
                let categories = [
                    None,
                    Some("imgs"),
                    Some("vids"),
                    Some("audios"),
                    Some("docs"),
                    Some("txts"),
                    Some("codes"),
                    Some("zips"),
                    Some("unknowns"),
                ];
                if let Some(opt) = categories.get(idx) {
                    switch_gallery_filter(app, opt.map(|s| s.to_owned()));
                }
                return;
            }

            if count > 0 && rect_contains(chunks[1], mouse.column, mouse.row) && chunks[1].height > 2 {
                let visible = chunks[1].height.saturating_sub(2) as usize;
                let page_size = visible.max(1);
                let start = app.gallery.cursor.saturating_sub(page_size / 2).min(count.saturating_sub(page_size));
                let end = (start + page_size).min(count);
                let row = mouse.row.saturating_sub(chunks[1].y + 1) as usize;
                if row < end.saturating_sub(start) {
                    app.gallery.cursor = start + row;
                }
            }
        }
        MouseEventKind::ScrollUp => {
            app.gallery.cursor = app.gallery.cursor.saturating_sub(1);
        }
        MouseEventKind::ScrollDown => {
            if app.gallery.cursor + 1 < count {
                app.gallery.cursor += 1;
            }
        }
        _ => {}
    }
}
