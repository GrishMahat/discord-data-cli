use ratatui::layout::{Constraint, Direction, Layout};

use crate::{
    app::{AppState, Screen},
};
use crate::ui::screens::gallery::draw_gallery;
use crate::ui::screens::setup::{draw_analyzing, draw_setup};
use crate::ui::screens::download::draw_downloading;
use crate::ui::screens::home::draw_home;
use crate::ui::screens::overview::draw_overview;
use crate::ui::screens::channel_list::draw_channels;
use crate::ui::screens::messages::draw_message_view;
use crate::ui::screens::support::{draw_support_activity, draw_support_ticket_detail};
use crate::ui::screens::activity::{draw_activity, draw_activity_detail};
use crate::ui::screens::settings::draw_settings;
use crate::ui::components::{draw_header, draw_statusbar, draw_tabs};

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
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);
    draw_tabs(frame, app, chunks[1]);

    match app.screen {
        Screen::Home => draw_home(frame, app, chunks[2]),
        Screen::Overview => draw_overview(frame, app, chunks[2]),
        Screen::SupportActivity => draw_support_activity(frame, app, chunks[2]),
        Screen::SupportTicketDetail => draw_support_ticket_detail(frame, app, chunks[2]),
        Screen::Activity => draw_activity(frame, app, chunks[2]),
        Screen::ActivityDetail => draw_activity_detail(frame, app, chunks[2]),
        Screen::ChannelList => draw_channels(frame, app, chunks[2]),
        Screen::MessageView => draw_message_view(frame, app, chunks[2]),
        Screen::Gallery => draw_gallery(frame, app, chunks[2]),
        Screen::Settings => draw_settings(frame, app, chunks[2]),
        _ => {}
    }

    draw_statusbar(frame, app, chunks[3]);
}













