use iced::alignment::Horizontal;
use iced::widget::{Svg, button, column, container, text, tooltip, vertical_space};
use iced::{Element, Length};

use app::{App, Message, Screen};

fn nav(
    icon: Svg<'static>,
    tip: &'static str,
    to: Screen,
    current: Screen,
) -> Element<'static, Message> {
    let active = to == current;
    let color = if active {
        icon_manager::accent()
    } else {
        icon_manager::muted()
    };

    tooltip(
        button(icon_manager::tint(icon, 18.0, color))
            .padding(11)
            .on_press(Message::Navigate(to))
            .style(style::nav_item(active)),
        container(text(tip).size(12))
            .padding([4, 8])
            .style(style::card),
        tooltip::Position::Right,
    )
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let current = if app.screen == Screen::Project {
        Screen::Projects
    } else {
        app.screen
    };

    let new_chat = button(icon_manager::tint(
        icon_manager::plus(),
        18.0,
        icon_manager::on_primary(),
    ))
    .padding(11)
    .on_press_maybe((!app.is_busy()).then_some(Message::NewChat))
    .style(style::primary_button);

    container(
        column![
            new_chat,
            nav(
                icon_manager::message_square(),
                "Chat",
                Screen::Chat,
                current
            ),
            nav(
                icon_manager::folder_kanban(),
                "Projects",
                Screen::Projects,
                current
            ),
            nav(icon_manager::database(), "Memory", Screen::Memory, current),
            vertical_space(),
            nav(
                icon_manager::sliders_horizontal(),
                "Settings",
                Screen::Settings,
                current
            ),
        ]
        .spacing(6)
        .align_x(Horizontal::Center)
        .padding([14, 8]),
    )
    .width(Length::Fixed(56.0))
    .height(Length::Fill)
    .style(style::rail)
    .into()
}
