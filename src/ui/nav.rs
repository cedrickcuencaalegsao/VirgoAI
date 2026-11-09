use iced::widget::{button, column, container, row, scrollable, text, Svg};
use iced::{Element, Length};

use super::{short, style};
use crate::app::{App, Message, Screen};
use crate::icons;

pub fn view(app: &App) -> Element<'_, Message> {
    let brand = column![
        row![
            text("Virgo")
                .size(26)
                .font(style::bold())
                .style(style::accent),
            text(" AI").size(26).font(style::bold()),
        ],
        text("Runs locally on your Mac").size(12).style(style::dim),
    ]
    .spacing(2);

    let new_chat = button(icons::label(
        icons::plus(),
        "New chat",
        14,
        icons::on_primary(),
    ))
    .width(Length::Fill)
    .padding([10, 14])
    .on_press_maybe((!app.is_busy()).then_some(Message::NewChat))
    .style(style::primary_button);

    let items = column![
        item(
            icons::folder_kanban(),
            "Projects",
            Screen::Projects,
            app.screen
        ),
        item(icons::message_square(), "Chat", Screen::Chat, app.screen),
        item(icons::database(), "Memory", Screen::Memory, app.screen),
        item(
            icons::sliders_horizontal(),
            "Settings",
            Screen::Settings,
            app.screen
        ),
    ]
    .spacing(4);

    // ---- recent chats ----
    let recent: Element<'_, Message> = if app.chats.is_empty() {
        text("No chats yet").size(12).style(style::dim).into()
    } else {
        scrollable(
            column(
                app.chats
                    .iter()
                    .map(|chat| chat_row(app, &chat.id, &chat.title)),
            )
            .spacing(2),
        )
        .height(Length::Fill)
        .into()
    };

    // ---- models in use ----
    let planner = if app.cfg.use_planner {
        app.cfg.planner_model.as_str()
    } else {
        "off"
    };
    let project = app
        .active_workspace_name()
        .unwrap_or_else(|| "none".to_string());
    let models = column![
        text("PROJECT").size(10).style(style::dim),
        text(short(&project, 24)).size(13),
        text("PLANNER").size(10).style(style::dim),
        text(planner).size(13),
        text("EXECUTOR").size(10).style(style::dim),
        text(app.cfg.executor_model.as_str()).size(13),
    ]
    .spacing(2);

    container(
        column![
            brand,
            new_chat,
            items,
            text("RECENT CHATS").size(10).style(style::dim),
            container(recent).height(Length::Fill),
            models,
        ]
        .spacing(16)
        .padding(20),
    )
    .width(Length::Fixed(240.0))
    .height(Length::Fill)
    .style(style::sidebar)
    .into()
}

fn item<'a>(
    icon: Svg<'a>,
    label: &'a str,
    target: Screen,
    current: Screen,
) -> Element<'a, Message> {
    let active = target == current;
    let color = if active {
        icons::accent()
    } else {
        icons::muted()
    };

    button(icons::label(icon, label, 14, color))
        .width(Length::Fill)
        .padding([10, 14])
        .on_press(Message::Navigate(target))
        .style(style::nav_item(active))
        .into()
}

/// One saved chat: click the title to open it, the trash icon to delete it.
fn chat_row<'a>(app: &'a App, id: &'a str, title: &str) -> Element<'a, Message> {
    let active = app.current_chat.as_deref() == Some(id);
    let enabled = !app.is_busy();

    row![
        button(text(short(title, 24)).size(13))
            .width(Length::Fill)
            .padding([7, 10])
            .on_press_maybe(enabled.then(|| Message::OpenChat(id.to_string())))
            .style(style::nav_item(active)),
        button(icons::tint(icons::trash_2(), 14.0, icons::muted()))
            .padding([7, 8])
            .on_press_maybe(enabled.then(|| Message::DeleteChat(id.to_string())))
            .style(style::ghost_button),
    ]
    .spacing(2)
    .into()
}