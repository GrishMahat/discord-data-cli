use anyhow::{Context, Result};
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
    terminal::size,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::{
    analyzer,
    app::{
        ActivityFilterField, AppState, ChannelFilter, HOME_MENU_ITEMS, Screen, SetupStep,
        apply_settings_selection, execute_home_selection, filtered_activity_events,
        filtered_channels, home_item_disabled_reason, is_printable_input, open_activity,
        open_channel_filter, open_selected_activity_event, open_selected_channel,
        open_selected_support_ticket, open_support_activity, refresh_support_activity_data,
        screen_disabled_reason, setup_prev_step, setup_submit_step, switch_filter,
        try_load_existing_data,
    },
    data::SupportTicketView,
};

pub(crate) fn handle_paste(app: &mut AppState, text: &str) {
    if app.screen != Screen::Setup || app.setup.step == SetupStep::Confirm {
        return;
    }
    let sanitized = text.replace(['\r', '\n'], "");
    app.setup.input.push_str(&sanitized);
}

pub(crate) fn handle_mouse(app: &mut AppState, mouse: MouseEvent) -> Result<()> {
    if matches!(
        app.screen,
        Screen::Setup | Screen::Analyzing | Screen::Downloading
    ) {
        return Ok(());
    }

    let (width, height) = size().with_context(|| "failed to read terminal size".to_owned())?;
    if width == 0 || height == 0 {
        return Ok(());
    }

    let root = Rect::new(0, 0, width, height);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(root);

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && let Some(target) = tab_at_position(chunks[1], mouse.column, mouse.row)
    {
        navigate_to_tab(app, target)?;
        return Ok(());
    }

    match app.screen {
        Screen::Home => handle_home_mouse(app, mouse, chunks[2])?,
        Screen::SupportActivity => handle_support_activity_mouse(app, mouse, chunks[2]),
        Screen::Activity => handle_activity_mouse(app, mouse, chunks[2]),
        Screen::SupportTicketDetail => handle_support_ticket_detail_mouse(app, mouse, chunks[2]),
        Screen::ActivityDetail => handle_activity_detail_mouse(app, mouse, chunks[2]),
        Screen::ChannelList => handle_channel_mouse(app, mouse, chunks[2])?,
        Screen::MessageView => handle_message_mouse(app, mouse, chunks[2]),
        Screen::Settings => handle_settings_mouse(app, mouse, chunks[2]),
        _ => {}
    }

    Ok(())
}

fn tab_at_position(area: Rect, x: u16, y: u16) -> Option<Screen> {
    if !rect_contains(area, x, y) || area.width == 0 {
        return None;
    }
    let local_x = x.saturating_sub(area.x) as usize;
    let tabs = [
        (Screen::Home, "Home"),
        (Screen::Overview, "Overview"),
        (Screen::SupportActivity, "Support"),
        (Screen::Activity, "Activity"),
        (Screen::ChannelList, "Channels"),
        (Screen::Settings, "Settings"),
    ];

    let mut cursor = 1usize; // leading space in draw_tabs
    for (screen, label) in tabs {
        let tab_len = label.len() + 2; // " {label} "
        if local_x >= cursor && local_x < cursor + tab_len {
            return Some(screen);
        }
        cursor += tab_len + 2; // trailing two spaces after each tab
    }
    None
}

fn navigate_to_tab(app: &mut AppState, target: Screen) -> Result<()> {
    if let Some(reason) = screen_disabled_reason(app, target) {
        app.status = reason;
        app.error = None;
        return Ok(());
    }

    match target {
        Screen::Home => app.screen = Screen::Home,
        Screen::Overview => {
            try_load_existing_data(app);
            app.screen = Screen::Overview;
        }
        Screen::SupportActivity => {
            open_support_activity(app)?;
        }
        Screen::Activity => {
            open_activity(app)?;
        }
        Screen::ChannelList => {
            open_channel_filter(app, ChannelFilter::All)?;
        }
        Screen::Settings => app.screen = Screen::Settings,
        _ => {}
    }
    Ok(())
}

fn handle_home_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) -> Result<()> {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return Ok(());
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(32), Constraint::Length(36)])
        .split(area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(3)])
        .split(cols[0]);
    let list_area = rows[0];

    if !rect_contains(list_area, mouse.column, mouse.row) || list_area.height <= 2 {
        return Ok(());
    }

    let row = mouse.row.saturating_sub(list_area.y + 1) as usize;
    if row < HOME_MENU_ITEMS.len() {
        app.home_cursor = row;
        execute_home_selection(app)?;
    }

    Ok(())
}

fn handle_channel_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) -> Result<()> {
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

fn handle_message_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    if !matches!(
        mouse.kind,
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
    ) {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(4)])
        .split(area);
    let message_area = rows[1];
    if !rect_contains(message_area, mouse.column, mouse.row) {
        return;
    }

    let max_scroll = app.open_message_lines.len().saturating_sub(1);
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.open_message_scroll = app.open_message_scroll.saturating_sub(3);
        }
        MouseEventKind::ScrollDown => {
            app.open_message_scroll = (app.open_message_scroll + 3).min(max_scroll);
        }
        _ => {}
    }
}

fn handle_settings_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) || area.height <= 2 {
        return;
    }
    if !rect_contains(area, mouse.column, mouse.row) {
        return;
    }

    let row = mouse.row.saturating_sub(area.y + 1) as usize;
    if row < 4 {
        app.settings_cursor = row;
        apply_settings_selection(app);
    }
}

fn handle_support_activity_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    let tickets: &[SupportTicketView] = app.support_tickets.as_deref().unwrap_or(&[]);

    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left))
        && !tickets.is_empty()
        && rect_contains(area, mouse.column, mouse.row)
        && area.height > 2
    {
        let visible_rows = area.height.saturating_sub(2) as usize;
        let page_size = visible_rows.max(1);
        let start = app
            .support_ticket_cursor
            .saturating_sub(page_size / 2)
            .min(tickets.len().saturating_sub(page_size));
        let end = (start + page_size).min(tickets.len());
        let row = mouse.row.saturating_sub(area.y + 1) as usize;
        if row < end.saturating_sub(start) {
            app.support_ticket_cursor = start + row;
            app.support_ticket_scroll = 0;
            open_selected_support_ticket(app);
        }
    }
}

fn handle_support_ticket_detail_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    if !rect_contains(area, mouse.column, mouse.row) {
        return;
    }

    let max_scroll = app
        .support_tickets
        .as_ref()
        .and_then(|tickets| tickets.get(app.support_ticket_cursor))
        .map(|ticket| ticket.detail_lines.len().saturating_sub(1))
        .unwrap_or(0);
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.support_ticket_scroll = app.support_ticket_scroll.saturating_sub(3);
        }
        MouseEventKind::ScrollDown => {
            app.support_ticket_scroll = (app.support_ticket_scroll + 3).min(max_scroll);
        }
        _ => {}
    }
}

fn handle_activity_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    if area.width <= 2 || area.height <= 2 {
        return;
    }

    let inner = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(1)])
        .split(inner);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(rows[1]);
    let list_area = cols[0];
    if !rect_contains(list_area, mouse.column, mouse.row) {
        return;
    }

    let filtered = filtered_activity_events(app);
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if filtered.is_empty() || list_area.height <= 2 {
                return;
            }
            let visible = list_area.height.saturating_sub(2) as usize;
            let page_size = visible.max(1);
            let cursor = app.activity_cursor.min(filtered.len().saturating_sub(1));
            let start = cursor
                .saturating_sub(page_size / 2)
                .min(filtered.len().saturating_sub(page_size));
            let end = (start + page_size).min(filtered.len());
            let row = mouse.row.saturating_sub(list_area.y + 1) as usize;
            if row < end.saturating_sub(start) {
                app.activity_cursor = start + row;
                open_selected_activity_event(app);
            }
        }
        MouseEventKind::ScrollUp => {
            app.activity_cursor = app.activity_cursor.saturating_sub(1);
        }
        MouseEventKind::ScrollDown => {
            let max_idx = filtered.len().saturating_sub(1);
            app.activity_cursor = (app.activity_cursor + 1).min(max_idx);
        }
        _ => {}
    }
}

fn handle_activity_detail_mouse(app: &mut AppState, mouse: MouseEvent, area: Rect) {
    if !rect_contains(area, mouse.column, mouse.row) {
        return;
    }
    let filtered = filtered_activity_events(app);
    let max_scroll = filtered
        .get(app.activity_cursor.min(filtered.len().saturating_sub(1)))
        .map(|event| event.detail.lines().count().saturating_sub(1))
        .unwrap_or(0);
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            app.activity_detail_scroll = app.activity_detail_scroll.saturating_sub(3);
        }
        MouseEventKind::ScrollDown => {
            app.activity_detail_scroll = (app.activity_detail_scroll + 3).min(max_scroll);
        }
        _ => {}
    }
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

pub(crate) fn handle_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    if app.screen == Screen::Analyzing || app.screen == Screen::Downloading {
        // Prevent key events while locked in processing screens unless it's Ctrl+C
        return Ok(());
    }

    if matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'))
        && matches!(
            app.screen,
            Screen::Home
                | Screen::Overview
                | Screen::SupportActivity
                | Screen::Activity
                | Screen::ChannelList
                | Screen::Settings
        )
    {
        app.should_quit = true;
        return Ok(());
    }

    if matches!(
        app.screen,
        Screen::Home
            | Screen::Overview
            | Screen::SupportActivity
            | Screen::SupportTicketDetail
            | Screen::Activity
            | Screen::ActivityDetail
            | Screen::ChannelList
            | Screen::MessageView
            | Screen::Settings
    ) && matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
    {
        let reverse = key.code == KeyCode::BackTab;
        let target = cycle_tab_screen(app, app.screen, reverse);
        navigate_to_tab(app, target)?;
        return Ok(());
    }

    match app.screen {
        Screen::Setup => handle_setup_key(app, key)?,
        Screen::Home => handle_home_key(app, key)?,
        Screen::Overview => handle_overview_key(app, key)?,
        Screen::SupportActivity => handle_support_activity_key(app, key)?,
        Screen::Activity => handle_activity_key(app, key)?,
        Screen::SupportTicketDetail => handle_support_ticket_detail_key(app, key),
        Screen::ActivityDetail => handle_activity_detail_key(app, key),
        Screen::ChannelList => handle_channel_key(app, key)?,
        Screen::MessageView => handle_message_key(app, key),
        Screen::Settings => handle_settings_key(app, key),
        _ => {}
    }

    Ok(())
}

fn cycle_tab_screen(app: &AppState, current: Screen, reverse: bool) -> Screen {
    let tabs = [
        Screen::Home,
        Screen::Overview,
        Screen::SupportActivity,
        Screen::Activity,
        Screen::ChannelList,
        Screen::Settings,
    ];
    let current = tab_group_screen(current);
    let current_idx = tabs.iter().position(|s| *s == current).unwrap_or(0);
    let len = tabs.len();
    let next_idx = if reverse {
        (current_idx + len - 1) % len
    } else {
        (current_idx + 1) % len
    };

    for i in 0..len {
        let idx = if reverse {
            (next_idx + len - i) % len
        } else {
            (next_idx + i) % len
        };
        let candidate = tabs[idx];
        if screen_disabled_reason(app, candidate).is_none() {
            return candidate;
        }
    }

    Screen::Home
}

fn tab_group_screen(screen: Screen) -> Screen {
    match screen {
        Screen::SupportTicketDetail => Screen::SupportActivity,
        Screen::ActivityDetail => Screen::Activity,
        Screen::MessageView => Screen::ChannelList,
        other => other,
    }
}

fn handle_setup_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
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

fn handle_home_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    let disabled_reason = |idx: usize| home_item_disabled_reason(app, idx);

    match key.code {
        KeyCode::Up
        | KeyCode::Char('w')
        | KeyCode::Char('W')
        | KeyCode::Char('k')
        | KeyCode::Char('K') => {
            app.home_cursor = app.home_cursor.saturating_sub(1);
        }
        KeyCode::Down
        | KeyCode::Char('s')
        | KeyCode::Char('S')
        | KeyCode::Char('j')
        | KeyCode::Char('J') => {
            if app.home_cursor + 1 < HOME_MENU_ITEMS.len() {
                app.home_cursor += 1;
            }
        }
        KeyCode::Enter => {
            execute_home_selection(app)?;
        }
        KeyCode::Char(c) if ('1'..='9').contains(&c) => {
            let idx = (c as u8 - b'1') as usize;
            if idx < HOME_MENU_ITEMS.len() {
                if let Some(reason) = disabled_reason(idx) {
                    app.home_cursor = idx; // move cursor so user can see disabled item
                    app.status = reason;
                    app.error = None;
                } else {
                    app.home_cursor = idx;
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn handle_overview_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            let results_dir = app.config.results_path(&app.config_path, &app.id);
            app.last_data = analyzer::read_data(&results_dir)?;
            app.status = "Overview refreshed from data.json".to_owned();
            app.error = None;
        }
        _ => {}
    }
    Ok(())
}

fn handle_support_activity_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    let ticket_count = app.support_tickets.as_ref().map(|v| v.len()).unwrap_or(0);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.support_ticket_cursor = app.support_ticket_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            if app.support_ticket_cursor + 1 < ticket_count {
                app.support_ticket_cursor += 1;
            }
        }
        KeyCode::Enter => open_selected_support_ticket(app),
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            refresh_support_activity_data(app)?;
        }
        _ => {}
    }
    Ok(())
}

fn handle_activity_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    if let Some(field) = app.activity_filter_edit {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                app.activity_filter_edit = None;
            }
            KeyCode::Backspace => {
                active_filter_mut(app, field).pop();
                app.activity_cursor = 0;
            }
            KeyCode::Char('u') | KeyCode::Char('U')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                active_filter_mut(app, field).clear();
                app.activity_cursor = 0;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if is_printable_input(c) {
                    active_filter_mut(app, field).push(c);
                    app.activity_cursor = 0;
                }
            }
            _ => {}
        }
        return Ok(());
    }

    let event_count = filtered_activity_events(app).len();
    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.activity_cursor = app.activity_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            if app.activity_cursor + 1 < event_count {
                app.activity_cursor += 1;
            }
        }
        KeyCode::PageUp => {
            app.activity_cursor = app.activity_cursor.saturating_sub(15);
        }
        KeyCode::PageDown => {
            app.activity_cursor = (app.activity_cursor + 15).min(event_count.saturating_sub(1));
        }
        KeyCode::Char('/') => app.activity_filter_edit = Some(ActivityFilterField::Query),
        KeyCode::Char('t') | KeyCode::Char('T') => {
            app.activity_filter_edit = Some(ActivityFilterField::EventType)
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            app.activity_filter_edit = Some(ActivityFilterField::SourceFile)
        }
        KeyCode::Char('[') => app.activity_filter_edit = Some(ActivityFilterField::FromDate),
        KeyCode::Char(']') => app.activity_filter_edit = Some(ActivityFilterField::ToDate),
        KeyCode::Char('o') | KeyCode::Char('O') => {
            app.activity_sort = app.activity_sort.next();
            app.activity_cursor = 0;
        }
        KeyCode::Enter => open_selected_activity_event(app),
        KeyCode::Char('c') | KeyCode::Char('C') => {
            app.activity_filters.query.clear();
            app.activity_filters.event_type.clear();
            app.activity_filters.source_file.clear();
            app.activity_filters.from_date.clear();
            app.activity_filters.to_date.clear();
            app.activity_cursor = 0;
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            refresh_support_activity_data(app)?;
        }
        _ => {}
    }
    Ok(())
}

fn handle_support_ticket_detail_key(app: &mut AppState, key: KeyEvent) {
    let max_scroll = app
        .support_tickets
        .as_ref()
        .and_then(|tickets| tickets.get(app.support_ticket_cursor))
        .map(|ticket| ticket.detail_lines.len().saturating_sub(1))
        .unwrap_or(0);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.support_ticket_scroll = app.support_ticket_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            app.support_ticket_scroll = (app.support_ticket_scroll + 1).min(max_scroll);
        }
        KeyCode::PageUp => {
            app.support_ticket_scroll = app.support_ticket_scroll.saturating_sub(15);
        }
        KeyCode::PageDown => {
            app.support_ticket_scroll = (app.support_ticket_scroll + 15).min(max_scroll);
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::SupportActivity;
        }
        _ => {}
    }
}

fn handle_activity_detail_key(app: &mut AppState, key: KeyEvent) {
    let filtered = filtered_activity_events(app);
    let max_scroll = filtered
        .get(app.activity_cursor.min(filtered.len().saturating_sub(1)))
        .map(|event| event.detail.lines().count().saturating_sub(1))
        .unwrap_or(0);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.activity_detail_scroll = app.activity_detail_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            app.activity_detail_scroll = (app.activity_detail_scroll + 1).min(max_scroll);
        }
        KeyCode::PageUp => {
            app.activity_detail_scroll = app.activity_detail_scroll.saturating_sub(15);
        }
        KeyCode::PageDown => {
            app.activity_detail_scroll = (app.activity_detail_scroll + 15).min(max_scroll);
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Activity;
        }
        _ => {}
    }
}

fn active_filter_mut(app: &mut AppState, field: ActivityFilterField) -> &mut String {
    match field {
        ActivityFilterField::Query => &mut app.activity_filters.query,
        ActivityFilterField::EventType => &mut app.activity_filters.event_type,
        ActivityFilterField::SourceFile => &mut app.activity_filters.source_file,
        ActivityFilterField::FromDate => &mut app.activity_filters.from_date,
        ActivityFilterField::ToDate => &mut app.activity_filters.to_date,
    }
}

fn handle_channel_key(app: &mut AppState, key: KeyEvent) -> Result<()> {
    let count = filtered_channels(app).len();

    if count == 0 {
        if matches!(
            key.code,
            KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace
        ) {
            app.screen = Screen::Home;
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
        KeyCode::Char('1') => switch_filter(app, ChannelFilter::All)?,
        KeyCode::Char('2') => switch_filter(app, ChannelFilter::Dm)?,
        KeyCode::Char('3') => switch_filter(app, ChannelFilter::GroupDm)?,
        KeyCode::Char('4') => switch_filter(app, ChannelFilter::PublicThread)?,
        KeyCode::Char('5') => switch_filter(app, ChannelFilter::Voice)?,
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home;
        }
        _ => {}
    }

    Ok(())
}

fn handle_message_key(app: &mut AppState, key: KeyEvent) {
    let max_scroll = app.open_message_lines.len().saturating_sub(1);

    match key.code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
            app.open_message_scroll = app.open_message_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
            app.open_message_scroll = (app.open_message_scroll + 1).min(max_scroll);
        }
        KeyCode::PageUp => {
            app.open_message_scroll = app.open_message_scroll.saturating_sub(15);
        }
        KeyCode::PageDown => {
            app.open_message_scroll = (app.open_message_scroll + 15).min(max_scroll);
        }
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::ChannelList;
            app.open_channel = None;
            app.open_message_lines.clear();
            app.open_message_scroll = 0;
        }
        _ => {}
    }
}

fn handle_settings_key(app: &mut AppState, key: KeyEvent) {
    const ITEMS: usize = 4;

    match key.code {
        KeyCode::Up
        | KeyCode::Char('w')
        | KeyCode::Char('W')
        | KeyCode::Char('k')
        | KeyCode::Char('K') => {
            app.settings_cursor = app.settings_cursor.saturating_sub(1);
        }
        KeyCode::Down
        | KeyCode::Char('s')
        | KeyCode::Char('S')
        | KeyCode::Char('j')
        | KeyCode::Char('J') => {
            if app.settings_cursor + 1 < ITEMS {
                app.settings_cursor += 1;
            }
        }
        KeyCode::Left
        | KeyCode::Char('a')
        | KeyCode::Char('A')
        | KeyCode::Char('h')
        | KeyCode::Char('H') => {
            if app.settings_cursor == 1 {
                app.settings.preview_messages =
                    app.settings.preview_messages.saturating_sub(5).max(5);
                app.save_session();
            }
        }
        KeyCode::Right
        | KeyCode::Char('d')
        | KeyCode::Char('D')
        | KeyCode::Char('l')
        | KeyCode::Char('L') => {
            if app.settings_cursor == 1 {
                app.settings.preview_messages = (app.settings.preview_messages + 5).min(500);
                app.save_session();
            }
        }
        KeyCode::Enter => apply_settings_selection(app),
        KeyCode::Char('b') | KeyCode::Char('B') | KeyCode::Esc | KeyCode::Backspace => {
            app.screen = Screen::Home
        }
        _ => {}
    }
}
