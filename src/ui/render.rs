use ratatui::layout::{Constraint, Direction, Layout};

use crate::app::{AppState, Screen};
use crate::ui::components::{draw_header, draw_sidebar_nav, draw_statusbar};
use crate::ui::screens::activity::{draw_activity, draw_activity_detail};
use crate::ui::screens::analyzing::draw_analyzing;
use crate::ui::screens::channel_list::draw_channels;
use crate::ui::screens::download::draw_downloading;
use crate::ui::screens::gallery::draw_gallery;
use crate::ui::screens::home::draw_home;
use crate::ui::screens::messages::draw_message_view;
use crate::ui::screens::overview::draw_overview;
use crate::ui::screens::settings::draw_settings;
use crate::ui::screens::setup::draw_setup;
use crate::ui::screens::support::{draw_support_activity, draw_support_ticket_detail};

pub(crate) fn draw_ui(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    if app.screen == Screen::Setup {
        draw_setup(frame, app);
        return;
    }

    if app.screen == Screen::Analyzing {
        draw_analyzing(frame, app);
        return;
    }

    if app.screen == Screen::Downloading {
        draw_downloading(frame, app);
        return;
    }

    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(10)])
        .split(chunks[1]);
    draw_sidebar_nav(frame, app, body[0]);

    match app.screen {
        Screen::Home => draw_home(frame, app, body[1]), // Home screen
        Screen::Overview => draw_overview(frame, app, body[1]),
        Screen::SupportActivity => draw_support_activity(frame, app, body[1]),
        Screen::SupportTicketDetail => draw_support_ticket_detail(frame, app, body[1]),
        Screen::Activity => draw_activity(frame, app, body[1]),
        Screen::ActivityDetail => draw_activity_detail(frame, app, body[1]),
        Screen::ChannelList => draw_channels(frame, app, body[1]),
        Screen::MessageView => draw_message_view(frame, app, body[1]),
        Screen::Gallery => draw_gallery(frame, app, body[1]),
        Screen::Settings => draw_settings(frame, app, body[1]),
        _ => {}
    }

    draw_statusbar(frame, app, chunks[2]);
}
