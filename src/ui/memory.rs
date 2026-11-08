use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, text, text_editor, text_input,
};
use iced::{Element, Length};

use super::style;
use crate::app::{App, Message};

pub fn view(app: &App) -> Element<'_, Message> {
    // ---- left: list of notes ----
    let header = row![
        column![
            text("Memory").size(22).font(style::bold()),
            text("Markdown notes on your SSD").size(12).style(style::dim),
        ]
        .spacing(2),
        horizontal_space(),
        button(text("Reload").size(12))
            .padding([6, 12])
            .on_press(Message::ReloadNotes)
            .style(style::ghost_button),
    ]
    .align_y(Vertical::Center);

    let new_note = row![
        text_input("new-note-name", &app.new_note_name)
            .on_input(Message::NewNoteNameChanged)
            .on_submit(Message::CreateNote)
            .padding(9)
            .size(14)
            .style(style::field),
        button(text("Add").size(14))
            .padding([9, 14])
            .on_press(Message::CreateNote)
            .style(style::primary_button),
    ]
    .spacing(6);

    let items = column(app.notes.iter().map(|name| {
        let selected = app.selected_note.as_deref() == Some(name.as_str());
        button(text(name.as_str()).size(14))
            .width(Length::Fill)
            .padding([8, 12])
            .on_press(Message::SelectNote(name.clone()))
            .style(style::nav_item(selected))
            .into()
    }))
    .spacing(2);

    let list_panel = container(
        column![header, new_note, scrollable(items).height(Length::Fill)].spacing(12),
    )
    .padding(16)
    .width(Length::Fixed(290.0))
    .height(Length::Fill)
    .style(style::card);

    // ---- right: editor ----
    let editor: Element<'_, Message> = match &app.selected_note {
        Some(name) => column![
            text(name.as_str()).size(16).font(style::bold()),
            text_editor(&app.editor)
                .on_action(Message::EditorAction)
                .height(Length::Fill),
            row![
                button(text("Save").size(14))
                    .padding([8, 18])
                    .on_press(Message::SaveNote)
                    .style(style::primary_button),
                button(text("Delete").size(14))
                    .padding([8, 18])
                    .on_press(Message::DeleteNote)
                    .style(style::danger_button),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .height(Length::Fill)
        .into(),
        None => text("Select a note to edit it. profile.md is always sent to the model.")
            .size(14)
            .style(style::dim)
            .into(),
    };

    let editor_panel = container(editor)
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style::card);

    row![list_panel, editor_panel]
        .spacing(14)
        .height(Length::Fill)
        .into()
}