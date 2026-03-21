use anyhow::Result;
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    terminal::size,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
};

use crate::app::{AppState, SetupStep, Screen, setup_prev_step, setup_submit_step, is_printable_input, cancel_analysis};
use crate::ui::components::centered_rect;
use crate::input::rect_contains;

pub(crate) fn handle_setup_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.should_quit = true;
        }
        KeyCode::Backspace => {
            if app.setup.step == SetupStep::Confirm {
                setup_prev_step(app);
            } else {
                app.setup.input.pop();
            }
        }
        KeyCode::Left | KeyCode::Up | KeyCode::BackTab => {
            setup_prev_step(app);
        }
        KeyCode::Enter | KeyCode::Tab | KeyCode::Down | KeyCode::Right => {
            if let Err(err) = setup_submit_step(app) {
                app.setup.notice = err.to_string();
            }
        }
        KeyCode::Char('u') | KeyCode::Char('U')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            if app.setup.step != SetupStep::Confirm {
                app.setup.input.clear();
            }
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            if is_printable_input(c) && app.setup.step != SetupStep::Confirm {
                app.setup.input.push(c);
            }
        }
        _ => {}
    }

    Ok(())
}

pub(crate) fn handle_analyzing_mouse(app: &mut AppState, mouse: MouseEvent) -> Result<()> {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return Ok(());
    }
    
    let (width, height) = size()?;
    let area = Rect::new(0, 0, width, height);
    let card = centered_rect(80, 80, area);
    
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(card);
    
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
        
    let button_area = sections[5];
    if !rect_contains(button_area, mouse.column, mouse.row) {
        return Ok(());
    }
    
    // Split the button area horizontally into two
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
