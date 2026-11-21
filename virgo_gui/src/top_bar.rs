use iced::alignment::Vertical;
use iced::widget::{Space, button, column, container, horizontal_space, row, text};
use iced::{Element, Length};

use super::short;
use app::{App, Message, Phase, Screen};

const NAMES: [&str; 5] = ["Scout", "Plan", "Review", "Execute", "Verify"];

fn current(app: &App) -> Option<usize> {
    if app.term_running {
        return Some(4);
    }
    if app.reviewing {
        return Some(2);
    }
    match app.phase {
        Phase::Scouting => Some(0),
        Phase::Planning => Some(1),
        Phase::Executing => Some(3),
        Phase::Idle => None,
    }
}

/// 2px line: done = text colour, current = signal, upcoming = line colour.
fn progress(app: &App, cur: Option<usize>) -> Element<'_, Message> {
    let _ = app;
    let mut r = row![].height(Length::Fixed(2.0));
    for i in 0..NAMES.len() {
        let color = match cur {
            Some(c) if i < c => style::text_color(),
            Some(c) if i == c => style::signal(),
            _ => style::line(),
        };
        r = r.push(
            container(Space::new(Length::Fill, Length::Fixed(2.0)))
                .width(Length::FillPortion(1))
                .style(style::segment(color)),
        );
    }
    r.into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let title = match app.screen {
        Screen::Chat => "Conversation",
        Screen::Projects | Screen::Project => "Projects",
        Screen::Memory => "Memory",
        Screen::Settings => "Settings",
    };
    let cur = if app.screen == Screen::Chat {
        current(app)
    } else {
        None
    };

    let mut left = row![text(title).size(style::T_20).font(style::semibold())]
        .spacing(style::SP_3)
        .align_y(Vertical::Center);
    if let Some(c) = cur {
        left = left.push(text(NAMES[c]).size(style::T_13).style(style::dim));
    }
    if app.screen == Screen::Chat {
        if let Some(name) = app.chat_workspace_name() {
            left = left.push(text(short(&name, 32)).size(style::T_12).style(style::dim));
        }
    }

    let right = row![
        button(text("Search  ⌘K").size(style::T_12))
            .padding([6, 10])
            .on_press(Message::TogglePalette)
            .style(style::ghost_button),
        button(icon_manager::tint(
            icon_manager::terminal(),
            16.0,
            icon_manager::muted()
        ))
        .padding(8)
        .on_press(Message::ToggleTerminal)
        .style(style::ghost_button),
        button(icon_manager::tint(
            icon_manager::panel_right(),
            16.0,
            icon_manager::muted()
        ))
        .padding(8)
        .on_press(Message::TogglePanel)
        .style(style::ghost_button),
    ]
    .spacing(style::SP_1)
    .align_y(Vertical::Center);

    column![
        row![left, horizontal_space(), right]
            .padding([10, 16])
            .align_y(Vertical::Center),
        progress(app, cur),
    ]
    .into()
}
