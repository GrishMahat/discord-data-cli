use anyhow::Result;
use crossterm::{
    event::{MouseButton, MouseEvent, MouseEventKind},
    terminal::size,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
};

use crate::app::{cancel_analysis, AppState, Screen};
use crate::input::rect_contains;
use crate::ui::components::centered_rect;

pub(crate) fn handle_analyzing_mouse(app: &mut AppState, mouse: MouseEvent) -> Result<()> {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return Ok(());
    }

    let (width, height) = size()?;
    let area = Rect::new(0, 0, width, height);
    let card = centered_rect(74, 64, area);

    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(card);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(10),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    let button_area = sections[10];
    if !rect_contains(button_area, mouse.column, mouse.row) {
        return Ok(());
    }

    let btn_widths = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(button_area);

    if rect_contains(btn_widths[0], mouse.column, mouse.row) {
        app.screen = Screen::Home;
    } else if rect_contains(btn_widths[1], mouse.column, mouse.row) {
        cancel_analysis(app);
    }

    Ok(())
}
