use iced::alignment::Vertical;
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, scrollable, text, text_input,
};
use iced::{Element, Length, Theme};

use crate::agent::{ChatMessage, Role};
use crate::app::{App, Message};

pub fn scroll_id() -> scrollable::Id {
    scrollable::Id::new("chat-scroll")
}

pub fn view(app: &App) -> Element<'_, Message> {
    let models = &app.models[..];

    let pickers = row![
        text("Planner").size(13),
        pick_list(
            models,
            Some(app.cfg.planner_model.clone()),
            Message::PlannerSelected
        )
        .placeholder("planner model"),
        text("Executor").size(13),
        pick_list(
            models,
            Some(app.cfg.executor_model.clone()),
            Message::ExecutorSelected
        )
        .placeholder("executor model"),
        horizontal_space(),
        button(text("Refresh models").size(13))
            .on_press(Message::RefreshModels)
            .style(button::secondary),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let messages: Element<'_, Message> = if app.messages.is_empty() {
        container(
            text("Ask something. Notes in your memory folder are used as context.").size(14),
        )
        .padding(20)
        .into()
    } else {
        column(app.messages.iter().map(bubble))
            .spacing(10)
            .padding(10)
            .into()
    };

    let history = scrollable(messages)
        .id(scroll_id())
        .width(Length::Fill)
        .height(Length::Fill);

    let tools = row![button(text("Remember last reply").size(13))
        .on_press_maybe(app.has_reply().then_some(Message::RememberLast))
        .style(button::secondary)];

    let can_send = !app.input.trim().is_empty();
    let send: Element<'_, Message> = if app.is_busy() {
        button("Stop")
            .on_press(Message::Stop)
            .style(button::danger)
            .into()
    } else {
        button("Send")
            .on_press_maybe(can_send.then_some(Message::Send))
            .into()
    };

    let input_row = row![
        text_input("Ask something...", &app.input)
            .on_input(Message::InputChanged)
            .on_submit(Message::Send)
            .padding(10),
        send,
    ]
    .spacing(8);

    column![pickers, history, tools, input_row]
        .spacing(10)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn bubble(msg: &ChatMessage) -> Element<'_, Message> {
    let label = match msg.role {
        Role::User => "You",
        Role::Plan => "Plan (planner model)",
        Role::Assistant => "Assistant (executor model)",
        Role::Error => "Error",
    };
    let body = if msg.content.is_empty() {
        "..."
    } else {
        msg.content.as_str()
    };
    let is_user = msg.role == Role::User;

    container(column![text(label).size(12), text(body).size(15)].spacing(4))
        .padding(10)
        .width(Length::Fill)
        .style(move |theme: &Theme| {
            if is_user {
                container::bordered_box(theme)
            } else {
                container::rounded_box(theme)
            }
        })
        .into()
}
