use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, text, text_editor, text_input,
};
use iced::{Element, Length};

use super::{short, style};
use crate::app::{App, Message};
use crate::icons;
use crate::workspaces;

pub fn view(app: &App) -> Element<'_, Message> {
    row![projects_panel(app), notes_panel(app), editor_panel(app)]
        .spacing(14)
        .height(Length::Fill)
        .into()
}

fn projects_panel(app: &App) -> Element<'_, Message> {
    let header = column![
        row![
            icons::tint(icons::folder_kanban(), 20.0, icons::accent()),
            text("Projects").size(22).font(style::bold()),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
        text("Conversations are saved as markdown, one folder per project")
            .size(12)
            .style(style::dim),
    ]
    .spacing(2);

    let create = row![
        text_input("new-project-name", &app.new_workspace_name)
            .on_input(Message::NewWorkspaceNameChanged)
            .on_submit(Message::CreateWorkspace)
            .padding(9)
            .size(14)
            .style(style::field),
        button(text("Add").size(14))
            .padding([9, 14])
            .on_press(Message::CreateWorkspace)
            .style(style::primary_button),
    ]
    .spacing(6);

    let items = column(app.workspaces.iter().map(|w| {
        let selected = app.selected_workspace.as_deref() == Some(w.slug.as_str());
        let active = app.active_workspace.as_deref() == Some(w.slug.as_str());
        let icon = if active {
            icons::tint(icons::check(), 14.0, icons::accent())
        } else {
            icons::tint(icons::folder_kanban(), 14.0, icons::muted())
        };
        row![
            button(
                row![icon, text(short(&w.name, 22)).size(14)]
                    .spacing(8)
                    .align_y(Vertical::Center)
            )
            .width(Length::Fill)
            .padding([8, 12])
            .on_press(Message::SelectWorkspace(w.slug.clone()))
            .style(style::nav_item(selected)),
            button(icons::tint(icons::trash_2(), 14.0, icons::muted()))
                .padding([7, 8])
                .on_press(Message::DeleteWorkspace(w.slug.clone()))
                .style(style::ghost_button),
        ]
        .spacing(2)
        .into()
    }))
    .spacing(2);

    let list: Element<'_, Message> = if app.workspaces.is_empty() {
        text("No projects yet").size(12).style(style::dim).into()
    } else {
        scrollable(items).height(Length::Fill).into()
    };

    container(column![header, create, list].spacing(12))
        .padding(16)
        .width(Length::Fixed(260.0))
        .height(Length::Fill)
        .style(style::card)
        .into()
}

fn notes_panel(app: &App) -> Element<'_, Message> {
    let body: Element<'_, Message> = match &app.selected_workspace {
        None => text("Select or create a project.")
            .size(13)
            .style(style::dim)
            .into(),
        Some(slug) => {
            let name = app
                .workspaces
                .iter()
                .find(|w| &w.slug == slug)
                .map(|w| w.name.as_str())
                .unwrap_or(slug.as_str());
            let active = app.active_workspace.as_deref() == Some(slug.as_str());

            let toggle: Element<'_, Message> = if active {
                button(icons::label(icons::check(), "Active", 12, icons::on_primary()))
                    .padding([6, 10])
                    .on_press(Message::ActivateWorkspace(None))
                    .style(style::primary_button)
                    .into()
            } else {
                button(text("Set active").size(12))
                    .padding([6, 10])
                    .on_press(Message::ActivateWorkspace(Some(slug.clone())))
                    .style(style::ghost_button)
                    .into()
            };

            let items = column(app.workspace_notes.iter().map(|n| {
                let selected = app.selected_ws_note.as_deref() == Some(n.as_str());
                let label = if n == "project.md" {
                    "project.md (always sent)".to_string()
                } else {
                    short(n.trim_end_matches(".md"), 30)
                };
                button(text(label).size(13))
                    .width(Length::Fill)
                    .padding([7, 10])
                    .on_press(Message::SelectWorkspaceNote(n.clone()))
                    .style(style::nav_item(selected))
                    .into()
            }))
            .spacing(2);

            column![
                row![
                    text(name).size(18).font(style::bold()),
                    horizontal_space(),
                    toggle
                ]
                .align_y(Vertical::Center),
                text(format!("{} note(s). New chats are saved here while active.", app.workspace_notes.len()))
                    .size(12)
                    .style(style::dim),
                scrollable(items).height(Length::Fill),
            ]
            .spacing(10)
            .height(Length::Fill)
            .into()
        }
    };

    container(body)
        .padding(16)
        .width(Length::Fixed(280.0))
        .height(Length::Fill)
        .style(style::card)
        .into()
}

fn editor_panel(app: &App) -> Element<'_, Message> {
    let content: Element<'_, Message> = match &app.selected_ws_note {
        Some(name) => {
            let mut actions = row![button(text("Save").size(14))
                .padding([8, 18])
                .on_press(Message::SaveWorkspaceNote)
                .style(style::primary_button)]
            .spacing(8);

            if let Some(id) = workspaces::chat_id(name) {
                actions = actions.push(
                    button(icons::label(icons::message_square(), "Open chat", 13, icons::muted()))
                        .padding([8, 14])
                        .on_press(Message::OpenWorkspaceChat(id))
                        .style(style::ghost_button),
                );
            }
            if name != "project.md" {
                actions = actions.push(
                    button(text("Delete note").size(14))
                        .padding([8, 18])
                        .on_press(Message::DeleteWorkspaceNote)
                        .style(style::danger_button),
                );
            }

            column![
                text(name.as_str()).size(16).font(style::bold()),
                text_editor(&app.ws_editor)
                    .on_action(Message::WorkspaceEditorAction)
                    .height(Length::Fill),
                actions,
            ]
            .spacing(12)
            .height(Length::Fill)
            .into()
        }
        None => text(
            "Open project.md to describe the project and list its features. \
             Saved conversations are what the models recall when you ask \
             \"do you remember this feature?\"",
        )
        .size(14)
        .style(style::dim)
        .into(),
    };

    container(content)
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style::card)
        .into()
}