use iced::alignment::Vertical;
use iced::widget::{Space, Svg, button, column, container, row, scrollable, text};
use iced::{Element, Length};

use super::short;
use app::{App, Message, Screen};
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

    let new_chat = button(icon_manager::label(
        icon_manager::plus(),
        "New chat",
        14,
        icon_manager::on_primary(),
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
            icon_manager::folder_kanban(),
            "Projects",
            Screen::Projects,
            projects_current
        ),
        item(
            icon_manager::message_square(),
            "Chat",
            Screen::Chat,
            app.screen
        ),
        item(
            icon_manager::database(),
            "Memory",
            Screen::Memory,
            app.screen
        ),
        item(
            icon_manager::sliders_horizontal(),
            "Settings",
            Screen::Settings,
            app.screen
        ),
    ]
    .spacing(style::SP_1);

    // ---- projects with their conversations ----
    let projects: Element<'_, Message> = if app.workspaces.is_empty() {
        text("No projects yet").size(12).style(style::dim).into()
    } else {
        scrollable(column(app.workspaces.iter().map(|w| project_block(app, w))).spacing(2))
            .style(style::scroll)
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
        .style(style::scroll)
        .height(Length::Fill)
        .into()
    };

    container(
        column![
            brand,
            new_chat,
            items,
            text("Projects").size(10).style(style::dim),
            container(projects).height(Length::FillPortion(1)),
            text("Recent Chats").size(10).style(style::dim),
            container(recent).height(Length::FillPortion(1)),
        ]
        .spacing(style::SP_4)
        .padding(style::SP_6 - 4.0),
    )
    .width(Length::Fixed(248.0))
    .height(Length::Fill)
    .style(style::sidebar)
    .into()
}

/// A 3px gold bar that shows next to the active item.
fn indicator<'a>(active: bool) -> Element<'a, Message> {
    container(Space::new(Length::Fixed(3.0), Length::Fixed(18.0)))
        .style(style::indicator(active))
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
        style::text_color()
    } else {
        icon_manager::muted()
    };

    row![
        indicator(active),
        button(icon_manager::label(icon, label, 14, color))
            .width(Length::Fixed(2.0))
            .padding([10, 14])
            .on_press(Message::Navigate(target))
            .style(style::nav_item(active)),
    ]
    .spacing(style::SP_1)
    .align_y(Vertical::Center)
    .into()
}

/// A project row. The open project (or the project of the chat on screen)
/// expands to show its own conversations.
fn project_block<'a>(app: &'a App, w: &'a Workspace) -> Element<'a, Message> {
    let expanded = app.selected_workspace.as_deref() == Some(w.slug.as_str())
        || app.chat_workspace().as_deref() == Some(w.slug.as_str());
    let active =
        app.screen == Screen::Project && app.selected_workspace.as_deref() == Some(w.slug.as_str());

    let icon = if expanded {
        icon_manager::tint(icon_manager::folder_open(), 14.0, icon_manager::accent())
    } else {
        icon_manager::tint(icon_manager::folder_kanban(), 14.0, icon_manager::muted())
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
        button(icon_manager::tint(
            icon_manager::trash_2(),
            14.0,
            icon_manager::muted()
        ))
        .padding([7, 8])
        .on_press_maybe(enabled.then(|| Message::DeleteChat(id.to_string())))
        .style(style::ghost_button),
    ]
    .spacing(2)
    .into()
}
