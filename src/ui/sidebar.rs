use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, text, text_editor, text_input,
};
use iced::{Element, Length, Theme};

use crate::app::{App, Message};

pub fn view(app: &App) -> Element<'_, Message> {
    let header = row![
        text("Memory").size(18),
        horizontal_space(),
        button(text("Reload").size(13))
            .on_press(Message::ReloadNotes)
            .style(button::secondary),
    ]
    .align_y(Vertical::Center);

    let items = column(app.notes.iter().map(|name| {
        let selected = app.selected_note.as_deref() == Some(name.as_str());
        button(text(name.as_str()).size(14))
            .width(Length::Fill)
            .on_press(Message::SelectNote(name.clone()))
            .style(move |theme: &Theme, status| {
                if selected {
                    button::primary(theme, status)
                } else {
                    button::secondary(theme, status)
                }
            })
            .into()
    }))
    .spacing(4);

    let list = scrollable(items).height(Length::Fixed(170.0));

    let new_note = row![
        text_input("new-note-name", &app.new_note_name)
            .on_input(Message::NewNoteNameChanged)
            .on_submit(Message::CreateNote)
            .padding(6),
        button("Add").on_press(Message::CreateNote),
    ]
    .spacing(6);

    let editor: Element<'_, Message> = match &app.selected_note {
        Some(name) => column![
            text(name.as_str()).size(14),
            text_editor(&app.editor)
                .on_action(Message::EditorAction)
                .height(Length::Fill),
            row![
                button("Save").on_press(Message::SaveNote),
                button("Delete")
                    .on_press(Message::DeleteNote)
                    .style(button::danger),
            ]
            .spacing(6),
        ]
        .spacing(6)
        .height(Length::Fill)
        .into(),
        None => text("Select a note to edit it. profile.md is always sent to the model.")
            .size(13)
            .into(),
    };

    container(column![header, new_note, list, editor].spacing(8))
        .padding(10)
        .width(Length::Fixed(320.0))
        .height(Length::Fill)
        .style(container::bordered_box)
        .into()
}
