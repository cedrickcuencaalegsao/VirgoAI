//! Right-hand panel: an explorer for the open folder. Flat full-width rows,
//! a small section title, and the attach action tucked at the row end.

use iced::alignment::Vertical;
use iced::widget::{
    Space, Svg, button, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Element, Length};

use app::{App, Message};
use project::{Entry, Project};

pub fn view(app: &App) -> Element<'_, Message> {
    let body: Element<'_, Message> = match &app.project {
        Some(project) => open_view(app, project),
        None => empty_view(app),
    };

    container(body)
        .padding([10, 8])
        .width(Length::Fixed(300.0))
        .height(Length::Fill)
        .style(style::card)
        .into()
}

fn icon_button<'a>(icon: Svg<'a>, message: Message) -> Element<'a, Message> {
    button(icon_manager::tint(icon, 15.0, icon_manager::muted()))
        .padding(6)
        .on_press(message)
        .style(style::ghost_button)
        .into()
}

/// Shown until a folder is opened.
fn empty_view(app: &App) -> Element<'_, Message> {
    column![
        text("Explorer").size(style::T_12).style(style::dim),
        text(
            "Open a folder to chat about your code. Click a file to open it in the code \
             panel, use the paperclip to attach it, or type @ in the message to mention it."
        )
        .size(13)
        .style(style::dim),
        button(icon_manager::label(
            icon_manager::folder_open(),
            "Open folder",
            14,
            icon_manager::on_primary()
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
    .padding([0, 4])
    .into()
}

fn open_view<'a>(app: &'a App, project: &'a Project) -> Element<'a, Message> {
    // Section title with the folder actions, then the root folder as the first row.
    let header = row![
        text("Explorer")
            .size(style::T_12)
            .font(style::semibold())
            .style(style::dim),
        horizontal_space(),
        icon_button(icon_manager::folder_open(), Message::OpenFolder),
        icon_button(icon_manager::refresh_cw(), Message::RefreshProject),
        icon_button(icon_manager::x(), Message::CloseProject),
    ]
    .spacing(0)
    .padding([0, 4])
    .align_y(Vertical::Center);

    let root = row![
        icon_manager::tint(icon_manager::chevron_down(), 14.0, icon_manager::muted()),
        text(project.name.as_str())
            .size(style::T_13)
            .font(style::semibold()),
    ]
    .spacing(6)
    .padding([4, 8])
    .align_y(Vertical::Center);

    let rows = column(
        project
            .visible(&app.expanded)
            .into_iter()
            .map(|entry| tree_row(app, entry)),
    )
    .spacing(0);

    let mut page = column![header, root, scrollable(rows).style(style::scroll).height(Length::Fill)]
        .spacing(4);

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
    let indent = Space::with_width(Length::Fixed(entry.depth as f32 * 12.0));

    if entry.is_dir {
        let open = app.expanded.contains(&entry.rel);
        let chevron = if open {
            icon_manager::chevron_down()
        } else {
            icon_manager::chevron_right()
        };

        let content = row![
            indent,
            icon_manager::tint(chevron, 14.0, icon_manager::muted()),
            icon_manager::tint(icon_manager::folder(), 15.0, icon_manager::accent()),
            text(entry.name.as_str()).size(13),
        ]
        .spacing(6)
        .align_y(Vertical::Center);

        button(content)
            .width(Length::Fill)
            .padding([3, 8])
            .on_press(Message::ToggleDir(entry.rel.clone()))
            .style(style::tree_item(false))
            .into()
    } else {
        let attached = app.attached.contains(&entry.rel);
        let open_here = app.code.as_ref().is_some_and(|doc| doc.rel == entry.rel);
        let icon_color = if open_here {
            icon_manager::accent()
        } else {
            icon_manager::muted()
        };

        let content = row![
            indent,
            Space::with_width(Length::Fixed(14.0)),
            icon_manager::tint(icon_manager::file_text(), 15.0, icon_color),
            text(entry.name.as_str()).size(13),
        ]
        .spacing(6)
        .align_y(Vertical::Center);

        // The paperclip stays quiet until a file is attached, then it becomes a check.
        let (attach_icon, attach_color) = if attached {
            (icon_manager::check(), icon_manager::accent())
        } else {
            (
                icon_manager::paperclip(),
                style::alpha(icon_manager::muted(), 0.4),
            )
        };

        // One band: the open file's selection runs under the attach button too.
        let line = row![
            button(content)
                .width(Length::Fill)
                .padding([3, 8])
                .on_press(Message::OpenFile(entry.rel.clone()))
                .style(style::tree_item(open_here)),
            button(icon_manager::tint(attach_icon, 14.0, attach_color))
                .padding([4, 6])
                .on_press(Message::ToggleAttach(entry.rel.clone()))
                .style(style::ghost_button),
        ]
        .spacing(0)
        .align_y(Vertical::Center);
        line.into()
    }
}

fn footer(app: &App) -> Element<'_, Message> {
    row![
        icon_manager::tint(icon_manager::paperclip(), 14.0, icon_manager::muted()),
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
    .padding([0, 4])
    .align_y(Vertical::Center)
    .into()
}