use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::{
    AppState, ChannelFilter, filtered_channels, open_selected_channel, switch_filter,
};
use crate::input::rect_contains;

pub(crate) fn handle_channel_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    let count = filtered_channels(app).len();

    if count == 0 {
        if matches!(
            key.code,
            KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace
        ) {
            app.screen = crate::app::Screen::Home;
        }
        return Ok(());
    }

    match key.code {
        KeyCode::Up
        | KeyCode::Char('w')
        | KeyCode::Char('W')
        | KeyCode::Char('k')
        | KeyCode::Char('K') => {
            app.channel_cursor = app.channel_cursor.saturating_sub(1);
        }
        KeyCode::Down
        | KeyCode::Char('s')
        | KeyCode::Char('S')
        | KeyCode::Char('j')
        | KeyCode::Char('J') => {
            if app.channel_cursor + 1 < count {
                app.channel_cursor += 1;
            }
        }
        KeyCode::PageUp | KeyCode::Char('u') | KeyCode::Char('U') => {
            app.channel_cursor = app.channel_cursor.saturating_sub(20);
        }
        KeyCode::PageDown | KeyCode::Char('d') | KeyCode::Char('D') => {
            app.channel_cursor = (app.channel_cursor + 20).min(count - 1);
        }
        KeyCode::Enter => {
            open_selected_channel(app)?;
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = crate::app::Screen::Home;
        }
        KeyCode::Char('1') => switch_filter(app, ChannelFilter::All)?,
        KeyCode::Char('2') => switch_filter(app, ChannelFilter::Dm)?,
        KeyCode::Char('3') => switch_filter(app, ChannelFilter::GroupDm)?,
        KeyCode::Char('4') => switch_filter(app, ChannelFilter::PublicThread)?,
        KeyCode::Char('5') => switch_filter(app, ChannelFilter::Voice)?,
        _ => {}
    }

    Ok(())
}

pub(crate) fn handle_channel_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) -> Result<()> {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(4)])
        .split(area);

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && rect_contains(rows[0], mouse.column, mouse.row)
        && rows[0].width > 0
    {
        let rel_x = mouse.column.saturating_sub(rows[0].x) as usize;
        let idx = (rel_x * 5) / rows[0].width as usize;
        match idx {
            0 => switch_filter(app, ChannelFilter::All)?,
            1 => switch_filter(app, ChannelFilter::Dm)?,
            2 => switch_filter(app, ChannelFilter::GroupDm)?,
            3 => switch_filter(app, ChannelFilter::PublicThread)?,
            4 => switch_filter(app, ChannelFilter::Voice)?,
            _ => {}
        }
        return Ok(());
    }

    let count = filtered_channels(app).len();
    if count == 0 {
        return Ok(());
    }

    let list_area = rows[1];
    if !rect_contains(list_area, mouse.column, mouse.row) {
        return Ok(());
    }

    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if list_area.height <= 2 {
                return Ok(());
            }

            let visible_rows = list_area.height.saturating_sub(2) as usize;
            let page_size = visible_rows.max(1);
            let start = app
                .channel_cursor
                .saturating_sub(page_size / 2)
                .min(count.saturating_sub(page_size));
            let end = (start + page_size).min(count);
            let row = mouse.row.saturating_sub(list_area.y + 1) as usize;
            if row < end.saturating_sub(start) {
                app.channel_cursor = start + row;
                open_selected_channel(app)?;
            }
        }
        MouseEventKind::ScrollUp => {
            app.channel_cursor = app.channel_cursor.saturating_sub(1);
        }
        MouseEventKind::ScrollDown => {
            if app.channel_cursor + 1 < count {
                app.channel_cursor += 1;
            }
        }
        _ => {}
    }

    Ok(())
}
