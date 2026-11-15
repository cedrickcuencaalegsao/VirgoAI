use iced::Element;
use iced::alignment::Vertical;
use iced::widget::{button, column, container, horizontal_space, row, text};

use super::short;
use app::{App, Message, Phase, Screen};
use style::StageState::{self, Done, Idle, Live};

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

fn strip(app: &App) -> Element<'_, Message> {
    let cur = current(app);
    let mut r = row![].spacing(6).align_y(Vertical::Center);

    for (i, name) in NAMES.iter().enumerate() {
        let state: StageState = match cur {
            Some(c) if i < c => Done,
            Some(c) if i == c => Live,
            _ => Idle,
        };
        let (mark, color) = match state {
            Live => (if app.tick % 2 == 0 { "●" } else { "○" }, style::live()),
            Done => ("✓", style::palette().text),
            Idle => ("○", style::dim_color()),
        };
        r = r.push(
            container(text(format!("{mark} {name}")).size(12).color(color))
                .padding([4, 10])
                .style(style::stage(state)),
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
    let subtitle = match (app.screen, app.chat_workspace_name()) {
        (Screen::Chat, Some(name)) => format!("Project: {name}"),
        (Screen::Chat, None) => "No project".to_string(),
        _ => String::new(),
    };

    let left = column![
        text(title).size(18).font(style::bold()),
        text(short(&subtitle, 40)).size(12).style(style::dim),
    ]
    .spacing(2);

    let middle: Element<'_, Message> = if app.screen == Screen::Chat {
        strip(app)
    } else {
        horizontal_space().into()
    };

    let right = row![
        button(text("Search  ⌘K").size(12))
            .padding([6, 12])
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
    .spacing(4)
    .align_y(Vertical::Center);

    row![left, horizontal_space(), middle, horizontal_space(), right]
        .spacing(12)
        .padding([10, 14])
        .align_y(Vertical::Center)
        .into()
}
