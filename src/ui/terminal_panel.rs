use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Element, Font, Length};

use super::style;
use crate::app::{App, Message};
use crate::icons;

pub fn scroll_id() -> scrollable::Id {
    scrollable::Id::new("term-scroll")
}

pub fn view(app: &App) -> Element<'_, Message> {
    let project = app.project.as_ref().map(|p| p.name.as_str()).unwrap_or("no project open");

    let header = row![
        icons::tint(icons::terminal(), 16.0, icons::accent()),
        text("Terminal").size(14).font(style::bold()),
        text(project).size(11).style(style::dim),
        horizontal_space(),
        button(text("Clear").size(12))
            .padding([4, 8])
            .on_press(Message::TermClear)
            .style(style::ghost_button),
        button(icons::tint(icons::x(), 14.0, icons::muted()))
            .padding(6)
            .on_press(Message::ToggleTerminal)
            .style(style::ghost_button),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let log: Element<'_, Message> = if app.term_log.is_empty() {
        text("Run a command, or press Run on a command the model proposes.")
            .size(12)
            .style(style::dim)
            .into()
    } else {
        text(app.term_log.as_str()).font(Font::MONOSPACE).size(12).into()
    };

    let output = container(
        scrollable(container(log).padding(8).width(Length::Fill))
            .id(scroll_id())
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(style::code_area);

    let can_run = !app.term_running && app.project.is_some();
    let input = row![
        text("$").font(Font::MONOSPACE).size(13),
        text_input("cargo check", &app.term_input)
            .on_input(Message::TermInputChanged)
            .on_submit(Message::TermSubmit)
            .font(Font::MONOSPACE)
            .padding(8)
            .size(13)
            .style(style::field),
        button(icons::label(
            icons::play(),
            if app.term_running { "Running..." } else { "Run" },
            13,
            icons::on_primary()
        ))
        .padding([8, 14])
        .on_press_maybe(can_run.then_some(Message::TermSubmit))
        .style(style::primary_button),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    container(column![header, output, input].spacing(8))
        .padding(12)
        .width(Length::Fill)
        .height(Length::Fixed(240.0))
        .style(style::card)
        .into()
}