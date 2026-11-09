//! Right-hand panel: open a project folder, browse its files, attach them.

use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, text, text_input, Space, Svg,
};
use iced::{Element, Length};

use super::style;
use crate::app::{App, Message};
use crate::icons;
use crate::project::{Entry, Project};

pub fn view(app: &App) -> Element<'_, Message> {
    let body: Element<'_, Message> = match &app.project {
        Some(project) => open_view(app, project),
        None => empty_view(app),
    };

    container(body)
        .padding(16)
        .width(Length::Fixed(300.0))
        .height(Length::Fill)
        .style(style::card)
        .into()
}

fn icon_button<'a>(icon: Svg<'a>, message: Message) -> Element<'a, Message> {
    button(icons::tint(icon, 16.0, icons::muted()))
        .padding(7)
        .on_press(message)
        .style(style::ghost_button)
        .into()
}

/// Shown until a folder is opened.
fn empty_view(app: &App) -> Element<'_, Message> {
    column![
        text("Project").size(18).font(style::bold()),
        text(
            "Open a folder to chat about your code. Click files to attach them, \
             or type @ in the message to mention a file or folder."
        )
        .size(13)
        .style(style::dim),
        button(icons::label(
            icons::folder_open(),
            "Open folder",
            14,
            icons::on_primary()
        ))
        .width(Length::Fill)
        .padding([10, 14])
        .on_press(Message::OpenFolder)
        .style(style::primary_button),
        text("or paste a path").size(11).style(style::dim),
        row![
            text_input("~/Projects/my-app", &app.project_path_input)
                .on_input(Message::ProjectPathChanged)
                .on_submit(Message::OpenProjectPath)
                .padding(8)
                .size(13)
                .style(style::field),
            button(text("Open").size(13))
                .padding([8, 12])
                .on_press(Message::OpenProjectPath)
                .style(style::primary_button),
        ]
        .spacing(6),
    ]
    .spacing(12)
    .into()
}

fn open_view<'a>(app: &'a App, project: &'a Project) -> Element<'a, Message> {
    let header = row![
        column![
            text("Project").size(18).font(style::bold()),
            text(project.name.as_str()).size(12).style(style::dim),
        ]
        .spacing(2),
        horizontal_space(),
        icon_button(icons::folder_open(), Message::OpenFolder),
        icon_button(icons::refresh_cw(), Message::RefreshProject),
        icon_button(icons::x(), Message::CloseProject),
    ]
    .spacing(2)
    .align_y(Vertical::Center);

    let hint = text("Click a file to attach it. Type @ in the chat to mention files or folders.")
        .size(11)
        .style(style::dim);

    let rows = column(
        project
            .visible(&app.expanded)
            .into_iter()
            .map(|entry| tree_row(app, entry)),
    )
    .spacing(1);

    let mut page = column![header, hint, scrollable(rows).height(Length::Fill)].spacing(10);

    if project.truncated {
        page = page.push(
            text("Large folder: showing the first entries only.")
                .size(11)
                .style(style::dim),
        );
    }

    page.push(footer(app)).height(Length::Fill).into()
}

fn tree_row<'a>(app: &'a App, entry: &'a Entry) -> Element<'a, Message> {
    let indent = Space::with_width(Length::Fixed(entry.depth as f32 * 14.0));

    if entry.is_dir {
        let open = app.expanded.contains(&entry.rel);
        let chevron = if open {
            icons::chevron_down()
        } else {
            icons::chevron_right()
        };

        let content = row![
            indent,
            icons::tint(chevron, 14.0, icons::muted()),
            icons::tint(icons::folder(), 15.0, icons::accent()),
            text(entry.name.as_str()).size(13),
        ]
        .spacing(6)
        .align_y(Vertical::Center);

        button(content)
            .width(Length::Fill)
            .padding([5, 8])
            .on_press(Message::ToggleDir(entry.rel.clone()))
            .style(style::nav_item(false))
            .into()
    } else {
        let attached = app.attached.contains(&entry.rel);
        let icon_color = if attached { icons::accent() } else { icons::muted() };

        let mut content = row![
            indent,
            Space::with_width(Length::Fixed(14.0)),
            icons::tint(icons::file_text(), 15.0, icon_color),
            text(entry.name.as_str()).size(13),
            horizontal_space(),
        ]
        .spacing(6)
        .align_y(Vertical::Center);

        if attached {
            content = content.push(icons::tint(icons::check(), 14.0, icons::accent()));
        }

        button(content)
            .width(Length::Fill)
            .padding([5, 8])
            .on_press(Message::ToggleAttach(entry.rel.clone()))
            .style(style::nav_item(attached))
            .into()
    }
}

fn footer(app: &App) -> Element<'_, Message> {
    row![
        icons::tint(icons::paperclip(), 14.0, icons::muted()),
        text(format!("{} attached", app.attached.len()))
            .size(12)
            .style(style::dim),
        horizontal_space(),
        button(text("Clear").size(12))
            .padding([4, 8])
            .on_press_maybe((!app.attached.is_empty()).then_some(Message::ClearAttached))
            .style(style::ghost_button),
    ]
    .spacing(6)
    .align_y(Vertical::Center)
    .into()
}