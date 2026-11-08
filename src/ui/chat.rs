use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, scrollable, text, text_input,
};
use iced::{Element, Length, Theme};

use super::style;
use crate::agent::{ChatMessage, Role};
use crate::app::{App, Message};

pub fn scroll_id() -> scrollable::Id {
    scrollable::Id::new("chat-scroll")
}

pub fn view(app: &App) -> Element<'_, Message> {
    let models = &app.models[..];

    // ---- header with model pickers ----
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

    let subtitle = if app.cfg.use_planner {
        "Plan first, then answer"
    } else {
        "Direct answer"
    };

    let header = row![
        column![
            text("Conversation").size(22).font(style::bold()),
            text(subtitle).size(12).style(style::dim),
        ]
        .spacing(2),
        horizontal_space(),
        pickers,
        button(text("Refresh").size(12))
            .padding([7, 12])
            .on_press(Message::RefreshModels)
            .style(style::ghost_button),
    ]
    .spacing(12)
    .align_y(Vertical::Center);

    // ---- messages ----
    let body: Element<'_, Message> = if app.messages.is_empty() {
        welcome()
    } else {
        column(
            app.messages
                .iter()
                .enumerate()
                .map(|(index, msg)| bubble(app, index, msg)),
        )
        .spacing(14)
        .padding([4, 12])
        .into()
    };

    let history = scrollable(body)
        .id(scroll_id())
        .width(Length::Fill)
        .height(Length::Fill);

    // ---- composer ----
    let remember = row![
        horizontal_space(),
        button(text("Remember last reply").size(12))
            .padding([6, 12])
            .on_press_maybe(app.has_reply().then_some(Message::RememberLast))
            .style(style::ghost_button),
    ];

    let can_send = !app.input.trim().is_empty();
    let action: Element<'_, Message> = if app.is_busy() {
        button(text("Stop").size(14))
            .padding([10, 20])
            .on_press(Message::Stop)
            .style(style::danger_button)
            .into()
    } else {
        button(text("Send").size(14))
            .padding([10, 20])
            .on_press_maybe(can_send.then_some(Message::Send))
            .style(style::primary_button)
            .into()
    };

    let composer = container(
        row![
            text_input("Message Virgo...", &app.input)
                .on_input(Message::InputChanged)
                .on_submit(Message::Send)
                .padding(12)
                .size(15)
                .style(style::composer_input),
            action,
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .padding(6)
    .style(style::composer);

    column![header, history, remember, composer]
        .spacing(12)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn welcome<'a>() -> Element<'a, Message> {
    container(
        column![
            text("Virgo AI")
                .size(40)
                .font(style::bold())
                .style(style::accent),
            text(
                "A planner model thinks it through, an executor model answers, \
                 and your markdown notes keep the memory."
            )
            .size(15)
            .style(style::dim)
            .width(Length::Fill)
            .align_x(Horizontal::Center),
        ]
        .spacing(10)
        .align_x(Horizontal::Center)
        .max_width(520),
    )
    .width(Length::Fill)
    .padding(60)
    .align_x(Horizontal::Center)
    .into()
}

fn bubble<'a>(app: &'a App, index: usize, msg: &'a ChatMessage) -> Element<'a, Message> {
    match msg.role {
        Role::User => {
            let bubble = container(text(msg.content.as_str()).size(15))
                .padding([10, 14])
                .max_width(640)
                .style(style::user_bubble);
            container(bubble)
                .width(Length::Fill)
                .align_x(Horizontal::Right)
                .into()
        }
        Role::Error => container(
            column![
                text("Error").size(12).font(style::bold()),
                text(msg.content.as_str()).size(14),
            ]
            .spacing(4),
        )
        .padding(12)
        .width(Length::Fill)
        .style(style::error_card)
        .into(),
        Role::Plan | Role::Assistant => ai_bubble(app, index, msg),
    }
}

fn ai_bubble<'a>(app: &'a App, index: usize, msg: &'a ChatMessage) -> Element<'a, Message> {
    let is_plan = msg.role == Role::Plan;
    let (label, model) = if is_plan {
        ("Plan", app.cfg.planner_model.as_str())
    } else {
        ("Virgo", app.cfg.executor_model.as_str())
    };

    // Is this the bubble the model is writing into right now?
    let active = app.is_busy() && index + 1 == app.messages.len();

    let header = row![
        text(label).size(12).font(style::bold()).style(style::accent),
        text(model).size(11).style(style::dim),
    ]
    .spacing(8);

    let mut body = column![header].spacing(8);

    // "Thinking..." status while waiting, or "Thought for Ns" afterwards.
    if !msg.thinking.is_empty() || (active && msg.content.is_empty()) {
        body = body.push(thinking_header(app, index, msg, active));
    }
    if msg.show_thinking && !msg.thinking.is_empty() {
        body = body.push(
            container(text(msg.thinking.as_str()).size(13).style(style::dim))
                .padding(10)
                .width(Length::Fill)
                .style(style::thinking_box),
        );
    }
    if !msg.content.is_empty() {
        body = body.push(text(msg.content.as_str()).size(15));
    }

    container(body)
        .padding(14)
        .width(Length::Fill)
        .style(move |theme: &Theme| {
            if is_plan {
                style::plan_card(theme)
            } else {
                style::card(theme)
            }
        })
        .into()
}

fn thinking_header<'a>(
    app: &'a App,
    index: usize,
    msg: &'a ChatMessage,
    active: bool,
) -> Element<'a, Message> {
    let thinking_now = active && msg.think_secs.is_none();

    let label = if thinking_now {
        let secs = app.think_started.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        if msg.thinking.is_empty() {
            format!("Thinking{}", dots(app.tick))
        } else {
            format!("Thinking{}  {secs}s", dots(app.tick))
        }
    } else if let Some(secs) = msg.think_secs {
        format!("Thought for {secs:.1}s")
    } else {
        String::new()
    };

    let mut line = row![text(label).size(13).style(move |theme: &Theme| {
        if thinking_now {
            style::accent(theme)
        } else {
            style::dim(theme)
        }
    })]
    .spacing(8)
    .align_y(Vertical::Center);

    if !msg.thinking.is_empty() {
        let toggle = if msg.show_thinking { "Hide" } else { "Show" };
        line = line.push(
            button(text(toggle).size(12))
                .padding([3, 10])
                .on_press(Message::ToggleThinking(index))
                .style(style::ghost_button),
        );
    }
    line.into()
}

/// Animated dots with a fixed width, so the label does not jitter.
fn dots(tick: u32) -> String {
    let n = (tick % 4) as usize;
    format!("{}{}", ".".repeat(n), " ".repeat(3 - n))
}