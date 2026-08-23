use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use crate::app::{
    AppState, ChannelFilter, SearchField, SearchHasFilter, open_search_result, start_message_search,
};
use crate::input::rect_contains;
use crate::ui::screens::search::search_layout;

const TYPE_FILTERS: [ChannelFilter; 4] = [
    ChannelFilter::All,
    ChannelFilter::Dm,
    ChannelFilter::GroupDm,
    ChannelFilter::PublicThread,
];

const HAS_FILTERS: [SearchHasFilter; 3] = [
    SearchHasFilter::Any,
    SearchHasFilter::Attachments,
    SearchHasFilter::Links,
];

pub(crate) fn handle_search_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    match app.search.field {
        SearchField::Input => handle_input_key(app, key),
        SearchField::TypeFilter => handle_type_filter_key(app, key),
        SearchField::HasFilter => handle_has_filter_key(app, key),
        SearchField::Results => handle_results_key(app, key),
    }
}

fn rerun_search(app: &mut AppState) {
    if !app.search.input.trim().is_empty() {
        start_message_search(app);
    }
}

fn handle_input_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char(c) => {
            if crate::app::is_printable_input(c) {
                app.search.input.push(c);
            }
        }
        KeyCode::Backspace => {
            app.search.input.pop();
        }
        KeyCode::Enter => {
            start_message_search(app);
            app.search.field = SearchField::Results;
        }
        KeyCode::Down => {
            app.search.field = SearchField::TypeFilter;
        }
        KeyCode::Esc => {
            app.screen = crate::app::Screen::Home;
        }
        _ => {}
    }
    Ok(())
}

fn handle_type_filter_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    let current = TYPE_FILTERS
        .iter()
        .position(|f| *f == app.search.type_filter)
        .unwrap_or(0);
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            app.search.type_filter = TYPE_FILTERS[current.saturating_sub(1)];
            rerun_search(app);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if current + 1 < TYPE_FILTERS.len() {
                app.search.type_filter = TYPE_FILTERS[current + 1];
                rerun_search(app);
            }
        }
        KeyCode::Char(c @ '1'..='4') => {
            let idx = c as usize - '1' as usize;
            app.search.type_filter = TYPE_FILTERS[idx];
            rerun_search(app);
        }
        KeyCode::Enter => rerun_search(app),
        // Shift+T anywhere on the search screen focuses the query box.
        KeyCode::Char('T') => app.search.field = SearchField::Input,
        KeyCode::Up | KeyCode::Char('k') => app.search.field = SearchField::Input,
        KeyCode::Down | KeyCode::Char('j') => app.search.field = SearchField::HasFilter,
        KeyCode::Esc => app.screen = crate::app::Screen::Home,
        _ => {}
    }
    Ok(())
}

fn handle_has_filter_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    let current = HAS_FILTERS
        .iter()
        .position(|f| *f == app.search.has_filter)
        .unwrap_or(0);
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            app.search.has_filter = HAS_FILTERS[current.saturating_sub(1)];
            rerun_search(app);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if current + 1 < HAS_FILTERS.len() {
                app.search.has_filter = HAS_FILTERS[current + 1];
                rerun_search(app);
            }
        }
        KeyCode::Char(c @ '1'..='3') => {
            let idx = c as usize - '1' as usize;
            app.search.has_filter = HAS_FILTERS[idx];
            rerun_search(app);
        }
        KeyCode::Enter => rerun_search(app),
        KeyCode::Char('T') => app.search.field = SearchField::Input,
        KeyCode::Up | KeyCode::Char('k') => app.search.field = SearchField::TypeFilter,
        KeyCode::Down | KeyCode::Char('j') => app.search.field = SearchField::Results,
        KeyCode::Esc => app.screen = crate::app::Screen::Home,
        _ => {}
    }
    Ok(())
}

fn handle_results_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    let count = app.search.results.len();
    match key.code {
        KeyCode::Char('T') => app.search.field = SearchField::Input,
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            if app.search.cursor == 0 {
                // Walking up past the first result exits into the filter row.
                app.search.field = SearchField::HasFilter;
            } else {
                app.search.cursor -= 1;
            }
        }
        KeyCode::Down
        | KeyCode::Char('s')
        | KeyCode::Char('S')
        | KeyCode::Char('j')
        | KeyCode::Char('J') => {
            if count > 0 && app.search.cursor + 1 < count {
                app.search.cursor += 1;
            }
        }
        KeyCode::PageUp | KeyCode::Char('u') | KeyCode::Char('U') => {
            app.search.cursor = app.search.cursor.saturating_sub(20);
        }
        KeyCode::PageDown | KeyCode::Char('d') | KeyCode::Char('D') => {
            if count > 0 {
                app.search.cursor = (app.search.cursor + 20).min(count - 1);
            }
        }
        KeyCode::Enter => {
            open_search_result(app)?;
        }
        KeyCode::Left | KeyCode::Char('h') => app.search.field = SearchField::HasFilter,
        KeyCode::Esc => app.screen = crate::app::Screen::Home,
        _ => {}
    }
    Ok(())
}

pub(crate) fn handle_search_mouse(
    app: &mut AppState,
    mouse: MouseEvent,
    area: ratatui::layout::Rect,
) -> Result<()> {
    let (input_area, type_area, has_area, progress_area, results_area) = search_layout(area);

    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
    ) {
        return Ok(());
    }

    // Query input: click focuses.
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && rect_contains(input_area, mouse.column, mouse.row)
    {
        app.search.field = SearchField::Input;
        return Ok(());
    }

    // Type filter pills: click selects directly.
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && rect_contains(type_area, mouse.column, mouse.row)
        && type_area.width > 0
    {
        let rel_x = mouse.column.saturating_sub(type_area.x + 1) as usize;
        let idx = (rel_x * TYPE_FILTERS.len()) / type_area.width as usize;
        if idx < TYPE_FILTERS.len() {
            app.search.field = SearchField::TypeFilter;
            app.search.type_filter = TYPE_FILTERS[idx];
            rerun_search(app);
        }
        return Ok(());
    }

    // Has filter pills.
    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && rect_contains(has_area, mouse.column, mouse.row)
        && has_area.width > 0
    {
        let rel_x = mouse.column.saturating_sub(has_area.x + 1) as usize;
        let idx = (rel_x * HAS_FILTERS.len()) / has_area.width as usize;
        if idx < HAS_FILTERS.len() {
            app.search.field = SearchField::HasFilter;
            app.search.has_filter = HAS_FILTERS[idx];
            rerun_search(app);
        }
        return Ok(());
    }

    if rect_contains(progress_area, mouse.column, mouse.row) {
        return Ok(());
    }

    // Results area: wheel scrolls selection, click selects.
    if rect_contains(results_area, mouse.column, mouse.row) {
        let count = app.search.results.len();
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                app.search.cursor = app.search.cursor.saturating_sub(3);
                app.search.field = SearchField::Results;
            }
            MouseEventKind::ScrollDown => {
                if count > 0 {
                    app.search.cursor = (app.search.cursor + 3).min(count - 1);
                }
                app.search.field = SearchField::Results;
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if results_area.height > 2 && count > 0 {
                    let visible_rows = results_area.height.saturating_sub(2) as usize;
                    let page_size = visible_rows.max(1);
                    let start = app
                        .search
                        .cursor
                        .saturating_sub(page_size / 2)
                        .min(count.saturating_sub(page_size));
                    let row = mouse.row.saturating_sub(results_area.y + 1) as usize;
                    if row < count - start {
                        app.search.cursor = start + row;
                        app.search.field = SearchField::Results;
                    }
                }
            }
            _ => {}
        }
    }

    Ok(())
}
