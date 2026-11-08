use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};

use super::style;
use crate::app::{App, Message, Screen};

pub fn view(app: &App) -> Element<'_, Message> {
    let brand = column![
        row![
            text("Virgo").size(26).font(style::bold()).style(style::accent),
            text(" AI").size(26).font(style::bold()),
        ],
        text("Runs locally on your Mac").size(12).style(style::dim),
    ]
    .spacing(2);

    let new_chat = button(text("+  New chat").size(14))
        .width(Length::Fill)
        .padding([10, 14])
        .on_press_maybe((!app.is_busy()).then_some(Message::NewChat))
        .style(style::primary_button);

    let items = column![
        item("Chat", Screen::Chat, app.screen),
        item("Memory", Screen::Memory, app.screen),
        item("Settings", Screen::Settings, app.screen),
    ]
    .spacing(4);

    // ---- recent chats ----
    let recent: Element<'_, Message> = if app.chats.is_empty() {
        text("No chats yet").size(12).style(style::dim).into()
    } else {
        scrollable(
            column(app.chats.iter().map(|chat| chat_row(app, &chat.id, &chat.title))).spacing(2),
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
    let models = column![
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

fn item(label: &str, target: Screen, current: Screen) -> Element<'_, Message> {
    button(text(label).size(14))
        .width(Length::Fill)
        .padding([10, 14])
        .on_press(Message::Navigate(target))
        .style(style::nav_item(target == current))
        .into()
}

/// One saved chat: click the title to open it, "x" to delete it.
fn chat_row<'a>(app: &'a App, id: &'a str, title: &str) -> Element<'a, Message> {
    let active = app.current_chat.as_deref() == Some(id);
    let enabled = !app.is_busy();

    row![
        button(text(short(title, 24)).size(13))
            .width(Length::Fill)
            .padding([7, 10])
            .on_press_maybe(enabled.then(|| Message::OpenChat(id.to_string())))
            .style(style::nav_item(active)),
        button(text("x").size(12))
            .padding([6, 9])
            .on_press_maybe(enabled.then(|| Message::DeleteChat(id.to_string())))
            .style(style::ghost_button),
    ]
    .spacing(2)
    .into()
}

/// Shorten a title so it fits on one line in the sidebar.
fn short(title: &str, max: usize) -> String {
    if title.chars().count() <= max {
        title.to_string()
    } else {
        let head: String = title.chars().take(max).collect();
        format!("{head}...")
    }
}