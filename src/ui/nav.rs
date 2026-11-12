use iced::alignment::Vertical;
use iced::widget::{button, column, container, row, scrollable, text, Space, Svg};
use iced::{Element, Length};

use super::{short, style};
use crate::app::{App, Message, Screen};
use crate::icons;
use workspaces::Workspace;

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

    // The Projects entry stays highlighted while a project home is open.
    let projects_current = if app.screen == Screen::Project {
        Screen::Projects
    } else {
        app.screen
    };

    let items = column![
        item(
            icons::folder_kanban(),
            "Projects",
            Screen::Projects,
            projects_current
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

    // ---- projects with their conversations ----
    let projects: Element<'_, Message> = if app.workspaces.is_empty() {
        text("No projects yet").size(12).style(style::dim).into()
    } else {
        scrollable(
            column(app.workspaces.iter().map(|w| project_block(app, w))).spacing(2),
        )
        .height(Length::Fill)
        .into()
    };

    // ---- general chats only (never project conversations) ----
    let general: Vec<_> = app.chats.iter().filter(|c| c.project.is_none()).collect();
    let recent: Element<'_, Message> = if general.is_empty() {
        text("No chats yet").size(12).style(style::dim).into()
    } else {
        scrollable(
            column(
                general
                    .into_iter()
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
        .chat_workspace_name()
        .unwrap_or_else(|| "none".to_string());
    let models = column![
        text("CHAT PROJECT").size(10).style(style::dim),
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
            text("PROJECTS").size(10).style(style::dim),
            container(projects).height(Length::FillPortion(1)),
            text("RECENT CHATS").size(10).style(style::dim),
            container(recent).height(Length::FillPortion(1)),
            models,
        ]
        .spacing(14)
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

/// A project row. The open project (or the project of the chat on screen)
/// expands to show its own conversations.
fn project_block<'a>(app: &'a App, w: &'a Workspace) -> Element<'a, Message> {
    let expanded = app.selected_workspace.as_deref() == Some(w.slug.as_str())
        || app.chat_workspace().as_deref() == Some(w.slug.as_str());
    let active = app.screen == Screen::Project
        && app.selected_workspace.as_deref() == Some(w.slug.as_str());

    let icon = if expanded {
        icons::tint(icons::folder_open(), 14.0, icons::accent())
    } else {
        icons::tint(icons::folder_kanban(), 14.0, icons::muted())
    };

    let head = button(
        row![icon, text(short(&w.name, 20)).size(13)]
            .spacing(8)
            .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([7, 10])
    .on_press(Message::OpenWorkspace(w.slug.clone()))
    .style(style::nav_item(active));

    let mut block = column![head].spacing(2);
    if expanded {
        for chat in app
            .chats
            .iter()
            .filter(|c| c.project.as_deref() == Some(w.slug.as_str()))
            .take(8)
        {
            block = block.push(row![
                Space::with_width(Length::Fixed(14.0)),
                chat_row(app, &chat.id, &chat.title),
            ]);
        }
    }
    block.into()
}

/// One saved chat: click the title to open it, the trash icon to delete it.
fn chat_row<'a>(app: &'a App, id: &'a str, title: &str) -> Element<'a, Message> {
    let active = app.current_chat.as_deref() == Some(id);
    let enabled = !app.is_busy();

    row![
        button(text(short(title, 22)).size(13))
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