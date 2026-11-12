//! Projects UI.
//! - `Screen::Projects`: simple list of projects + "New project".
//! - `Screen::Project`: the project workspace: composer and recent conversations
//!   in the middle, instructions / knowledge / files in a right-hand panel.

use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, scrollable, text, text_editor,
    text_input,
};
use iced::{Element, Length};

use super::{short, style};
use crate::app::{App, Message, Screen};
use chats::{self, ChatSummary};
use crate::icons;
use crate::workspaces::{Workspace, PROJECT_NOTE};

pub fn view(app: &App) -> Element<'_, Message> {
    match (app.screen, app.selected_workspace.as_deref()) {
        (Screen::Project, Some(slug)) => home(app, slug),
        _ => list(app),
    }
}

fn ago(updated: u64) -> String {
    let d = chats::now_secs().saturating_sub(updated);
    match d {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", d / 60),
        3600..=86_399 => format!("{} h ago", d / 3600),
        _ => format!("{} d ago", d / 86_400),
    }
}

// ---------------------------------------------------------------------------
// Project list
// ---------------------------------------------------------------------------

fn list(app: &App) -> Element<'_, Message> {
    let header = column![
        row![
            icons::tint(icons::folder_kanban(), 20.0, icons::accent()),
            text("Projects").size(24).font(style::bold()),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
        text("Each project is a workspace with its own instructions, knowledge and conversations.")
            .size(12)
            .style(style::dim),
    ]
    .spacing(2);

    let create = row![
        text_input("New project name", &app.new_workspace_name)
            .on_input(Message::NewWorkspaceNameChanged)
            .on_submit(Message::CreateWorkspace)
            .padding(9)
            .size(14)
            .style(style::field),
        button(icons::label(icons::plus(), "New project", 13, icons::on_primary()))
            .padding([9, 14])
            .on_press(Message::CreateWorkspace)
            .style(style::primary_button),
    ]
    .spacing(6);

    let cards: Element<'_, Message> = if app.workspaces.is_empty() {
        text("No projects yet. Create one to start a dedicated workspace.")
            .size(13)
            .style(style::dim)
            .into()
    } else {
        scrollable(column(app.workspaces.iter().map(|w| card(app, w))).spacing(8))
            .height(Length::Fill)
            .into()
    };

    container(column![header, create, cards].spacing(14).max_width(760))
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style::card)
        .into()
}

fn card<'a>(app: &'a App, w: &'a Workspace) -> Element<'a, Message> {
    let mine = |c: &&ChatSummary| c.project.as_deref() == Some(w.slug.as_str());
    let count = app.chats.iter().filter(mine).count();
    let last = app
        .chats
        .iter()
        .find(|c| mine(c))
        .map(|c| ago(c.updated))
        .unwrap_or_else(|| "no conversations yet".to_string());

    button(
        row![
            icons::tint(icons::folder_kanban(), 18.0, icons::accent()),
            column![
                text(short(&w.name, 40)).size(15).font(style::bold()),
                text(format!("{count} conversation(s) · {last}"))
                    .size(12)
                    .style(style::dim),
            ]
            .spacing(2),
        ]
        .spacing(12)
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .padding([12, 14])
    .on_press(Message::OpenWorkspace(w.slug.clone()))
    .style(style::nav_item(false))
    .into()
}

// ---------------------------------------------------------------------------
// Project home
// ---------------------------------------------------------------------------

fn home<'a>(app: &'a App, slug: &'a str) -> Element<'a, Message> {
    row![center(app, slug), side_panel(app)]
        .spacing(14)
        .height(Length::Fill)
        .into()
}

fn center<'a>(app: &'a App, slug: &'a str) -> Element<'a, Message> {
    let name = app.workspace_name(slug);
    let confirming = app.confirm_delete.as_deref() == Some(slug);

    let delete = if confirming {
        button(icons::label(icons::trash_2(), "Confirm delete", 12, icons::on_danger()))
            .padding([6, 12])
            .on_press(Message::DeleteWorkspace(slug.to_string()))
            .style(style::danger_button)
    } else {
        button(icons::tint(icons::trash_2(), 15.0, icons::muted()))
            .padding(8)
            .on_press(Message::DeleteWorkspace(slug.to_string()))
            .style(style::ghost_button)
    };

    let header = row![
        column![
            button(text("Back to projects").size(12))
                .padding([4, 8])
                .on_press(Message::Navigate(Screen::Projects))
                .style(style::ghost_button),
            text(name).size(26).font(style::bold()),
        ]
        .spacing(4),
        horizontal_space(),
        delete,
    ]
    .align_y(Vertical::Center);

    let body: Element<'a, Message> = if app.selected_ws_note.is_some() {
        knowledge_editor(app)
    } else {
        workspace_body(app, slug)
    };

    container(column![header, body].spacing(16))
        .padding([4, 8])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn workspace_body<'a>(app: &'a App, slug: &'a str) -> Element<'a, Message> {
    // ---- composer ----
    let can_send = !app.project_input.trim().is_empty() && !app.is_busy();
    let composer = container(
        row![
            text_input("What would you like to work on?", &app.project_input)
                .on_input(Message::ProjectInputChanged)
                .on_submit(Message::ProjectSend)
                .padding(14)
                .size(16)
                .style(style::composer_input),
            button(icons::label(icons::send(), "Start", 14, icons::on_primary()))
                .padding([10, 18])
                .on_press_maybe(can_send.then_some(Message::ProjectSend))
                .style(style::primary_button),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .padding(6)
    .style(style::composer);

    // ---- model pickers (same integration as the chat screen) ----
    let models = &app.models[..];
    let mut pickers = row![].spacing(8).align_y(Vertical::Center);
    if app.cfg.use_planner {
        pickers = pickers
            .push(text("Planner").size(12).style(style::dim))
            .push(
                pick_list(
                    models,
                    Some(app.cfg.planner_model.clone()),
                    Message::PlannerSelected,
                )
                .placeholder("planner")
                .padding(7)
                .text_size(13)
                .style(style::pick),
            );
    }
    pickers = pickers
        .push(text("Executor").size(12).style(style::dim))
        .push(
            pick_list(
                models,
                Some(app.cfg.executor_model.clone()),
                Message::ExecutorSelected,
            )
            .placeholder("executor")
            .padding(7)
            .text_size(13)
            .style(style::pick),
        );

    // ---- recent conversations of THIS project only ----
    let mine: Vec<&ChatSummary> = app
        .chats
        .iter()
        .filter(|c| c.project.as_deref() == Some(slug))
        .collect();

    let recent: Element<'a, Message> = if mine.is_empty() {
        text("No conversations yet. Type above to start the first one.")
            .size(13)
            .style(style::dim)
            .into()
    } else {
        scrollable(column(mine.into_iter().map(|c| conversation_row(app, c))).spacing(4))
            .height(Length::Fill)
            .into()
    };

    column![
        composer,
        pickers,
        text("Recent conversations").size(14).font(style::bold()),
        recent,
    ]
    .spacing(14)
    .height(Length::Fill)
    .into()
}

fn conversation_row<'a>(app: &'a App, c: &'a ChatSummary) -> Element<'a, Message> {
    let active = app.current_chat.as_deref() == Some(c.id.as_str());
    let enabled = !app.is_busy();

    row![
        button(
            row![
                icons::tint(icons::message_square(), 15.0, icons::muted()),
                text(short(&c.title, 60)).size(14),
                horizontal_space(),
                text(ago(c.updated)).size(12).style(style::dim),
            ]
            .spacing(10)
            .align_y(Vertical::Center),
        )
        .width(Length::Fill)
        .padding([10, 12])
        .on_press_maybe(enabled.then(|| Message::OpenChat(c.id.clone())))
        .style(style::nav_item(active)),
        button(icons::tint(icons::trash_2(), 14.0, icons::muted()))
            .padding([7, 8])
            .on_press_maybe(enabled.then(|| Message::DeleteChat(c.id.clone())))
            .style(style::ghost_button),
    ]
    .spacing(2)
    .align_y(Vertical::Center)
    .into()
}

/// Editor for one knowledge note, shown in the centre instead of the composer.
fn knowledge_editor(app: &App) -> Element<'_, Message> {
    let name = app.selected_ws_note.clone().unwrap_or_default();

    let mut actions = row![
        button(text("Save").size(14))
            .padding([8, 18])
            .on_press(Message::SaveWorkspaceNote)
            .style(style::primary_button),
        button(text("Close").size(14))
            .padding([8, 18])
            .on_press(Message::CloseWorkspaceNote)
            .style(style::ghost_button),
    ]
    .spacing(8);
    if name != PROJECT_NOTE {
        actions = actions.push(
            button(text("Delete note").size(14))
                .padding([8, 18])
                .on_press(Message::DeleteWorkspaceNote)
                .style(style::danger_button),
        );
    }

    container(
        column![
            row![
                text(name).size(16).font(style::bold()),
                horizontal_space(),
                text("Markdown knowledge for this project")
                    .size(12)
                    .style(style::dim),
            ]
            .align_y(Vertical::Center),
            text_editor(&app.ws_editor)
                .on_action(Message::WorkspaceEditorAction)
                .height(Length::Fill),
            actions,
        ]
        .spacing(12)
        .height(Length::Fill),
    )
    .padding(16)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(style::card)
    .into()
}

// ---------------------------------------------------------------------------
// Right-hand project panel
// ---------------------------------------------------------------------------

fn side_panel(app: &App) -> Element<'_, Message> {
    // ---- instructions ----
    let instructions = column![
        text("Instructions").size(15).font(style::bold()),
        text("Applied to every conversation in this project.")
            .size(11)
            .style(style::dim),
        text_editor(&app.instructions_editor)
            .on_action(Message::InstructionsAction)
            .height(Length::Fixed(150.0)),
        button(text("Save instructions").size(13))
            .padding([7, 14])
            .on_press(Message::SaveInstructions)
            .style(style::primary_button),
    ]
    .spacing(8);

    // ---- memory / knowledge ----
    let items = column(app.workspace_notes.iter().map(|n| {
        let selected = app.selected_ws_note.as_deref() == Some(n.as_str());
        let label = if n == PROJECT_NOTE {
            "project.md (always sent)".to_string()
        } else {
            short(n.trim_end_matches(".md"), 30)
        };
        button(text(label).size(13))
            .width(Length::Fill)
            .padding([6, 10])
            .on_press(Message::SelectWorkspaceNote(n.clone()))
            .style(style::nav_item(selected))
            .into()
    }))
    .spacing(2);

    let knowledge = column![
        text("Memory / Knowledge").size(15).font(style::bold()),
        text(
            "Markdown notes available to the model in this project only. \
             The most relevant parts are picked for each message."
        )
        .size(11)
        .style(style::dim),
        items,
        row![
            text_input("new-note-name", &app.new_knowledge_name)
                .on_input(Message::KnowledgeNameChanged)
                .on_submit(Message::CreateKnowledge)
                .padding(7)
                .size(13)
                .style(style::field),
            button(text("Add").size(13))
                .padding([7, 12])
                .on_press(Message::CreateKnowledge)
                .style(style::primary_button),
        ]
        .spacing(6),
    ]
    .spacing(8);

    // ---- context / files ----
    let linked: Element<'_, Message> = match &app.linked_folder {
        Some(path) => text(short(path, 36)).size(12).into(),
        None => text("No folder linked.").size(12).style(style::dim).into(),
    };

    let mut buttons = row![].spacing(6);
    if app.linked_folder.is_some() {
        buttons = buttons.push(
            button(text("Open linked").size(12))
                .padding([6, 10])
                .on_press(Message::OpenLinkedFolder)
                .style(style::ghost_button),
        );
    }
    if app.project.is_some() {
        buttons = buttons.push(
            button(text("Link open folder").size(12))
                .padding([6, 10])
                .on_press(Message::LinkFolder)
                .style(style::ghost_button),
        );
    }
    buttons = buttons.push(
        button(icons::label(icons::folder_open(), "Browse", 12, icons::muted()))
            .padding([6, 10])
            .on_press(Message::OpenFolder)
            .style(style::ghost_button),
    );

    let mut context = column![
        text("Context / Files").size(15).font(style::bold()),
        linked,
        buttons,
    ]
    .spacing(8);
    if let Some(p) = &app.project {
        context = context.push(
            text(format!("Open now: {}", p.name))
                .size(11)
                .style(style::dim),
        );
    }
    context = context.push(
        text("In a project chat, type @ to mention files from the open folder.")
            .size(11)
            .style(style::dim),
    );

    container(scrollable(column![instructions, knowledge, context].spacing(22)))
        .padding(16)
        .width(Length::Fixed(320.0))
        .height(Length::Fill)
        .style(style::card)
        .into()
}