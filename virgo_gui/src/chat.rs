use iced::alignment::{Horizontal, Vertical};
use iced::widget::text_editor::{Binding, Status};
use iced::widget::{
    button, column, container, horizontal_space, pick_list, row, scrollable, text, text_editor,
};
use iced::{Element, Length, keyboard};

use super::{markdown, short};
use agent::{ChatMessage, Role};
use app::{App, Message, Phase};
use iced::{Background, Border, Color, Theme};

use icon_manager;

pub fn scroll_id() -> scrollable::Id {
    scrollable::Id::new("chat-scroll")
}

pub fn view(app: &App) -> Element<'_, Message> {
    let models = &app.models[..];

    let mut pickers = row![].spacing(8).align_y(Vertical::Center);
    if app.cfg.use_planner {
        pickers = pickers
            .push(text("Planner").size(style::T_12).style(style::dim))
            .push(
                pick_list(
                    models,
                    Some(app.cfg.planner_model.clone()),
                    Message::PlannerSelected,
                )
                .placeholder("planner")
                .width(Length::Fill)
                .padding(7)
                .text_size(style::T_13)
                .style(style::pick),
            );
    }
    pickers = pickers
        .push(text("Executor").size(style::T_12).style(style::dim))
        .push(
            pick_list(
                models,
                Some(app.cfg.executor_model.clone()),
                Message::ExecutorSelected,
            )
            .placeholder("executor")
            .padding(7)
            .text_size(style::T_13)
            .style(style::pick),
        );

    let body: Element<'_, Message> = if app.messages.is_empty() {
        welcome()
    } else {
        let mut items = column(
            app.messages
                .iter()
                .enumerate()
                .map(|(i, m)| bubble(app, i, m)),
        )
        .spacing(style::SP_4);
        // Scouting has no message of its own yet, so show it here.
        if app.phase == Phase::Scouting {
            items = items.push(working_line(app, "Reading project files".to_string()));
        }
        items.padding([style::SP_2, style::SP_3]).into()
    };

    let history = scrollable(
        container(body)
            .max_width(style::READING_WIDTH)
            .center_x(Length::Fill),
    )
    .id(scroll_id())
    .style(style::scroll)
    .width(Length::Fill)
    .height(Length::Fill);

    // ---- modes: small ghost row above the composer ----
    let n = app.revertable();
    let can_toggle = app.project.is_some() && !app.is_busy();
    let modes = row![
        button(
            text(if app.edit_mode {
                "Edit mode on"
            } else {
                "Edit mode"
            })
            .size(style::T_12)
        )
        .padding([4, 8])
        .on_press_maybe(can_toggle.then_some(Message::ToggleEditMode(!app.edit_mode)))
        .style(style::ghost_button),
        button(icon_manager::label(
            icon_manager::undo_2(),
            format!("Revert last edit ({n})"),
            12,
            icon_manager::muted()
        ))
        .padding([4, 8])
        .on_press_maybe((n > 0).then_some(Message::RevertLast))
        .style(style::ghost_button),
        button(text("Remember").size(style::T_12))
            .padding([4, 8])
            .on_press_maybe(app.has_reply().then_some(Message::RememberLast))
            .style(style::ghost_button),
    ]
    .spacing(style::SP_1)
    .align_y(Vertical::Center);

    // ---- send / stop: 28px, signal only when there is text ----
    let can_send = !app.input.trim().is_empty();
    let action: Element<'_, Message> = if app.is_busy() {
        button(
            container(icon_manager::tint(
                icon_manager::square(),
                11.0,
                style::danger_color(),
            ))
            .center(Length::Fill),
        )
        .width(Length::Fixed(28.0))
        .height(Length::Fixed(28.0))
        .padding(0)
        .on_press(Message::Stop)
        .style(style::stop_button)
        .into()
    } else {
        let color = if can_send {
            style::signal()
        } else {
            style::dim_color()
        };
        button(
            container(icon_manager::tint(icon_manager::send(), 16.0, color)).center(Length::Fill),
        )
        .width(Length::Fixed(28.0))
        .height(Length::Fixed(28.0))
        .padding(0)
        .on_press_maybe(can_send.then_some(Message::Send))
        .style(style::send_button)
        .into()
    };

    let placeholder = if app.project.is_some() {
        "Message Virgo... (type @ to mention a project file)"
    } else {
        "Message Virgo..."
    };

    let composer = container(
        row![
            container(
                text_editor(&app.input_editor)
                    .placeholder(placeholder)
                    .on_action(Message::InputAction)
                    .key_binding(|kp| {
                        let enter =
                            matches!(kp.key, keyboard::Key::Named(keyboard::key::Named::Enter));
                        if matches!(kp.status, Status::Focused) && enter && !kp.modifiers.shift() {
                            Some(Binding::Custom(Message::Send))
                        } else {
                            Binding::from_key_press(kp)
                        }
                    })
                    .padding([10, 12])
                    .size(style::T_15)
                    .height(Length::Shrink)
                    .style(style::composer_editor),
            )
            .max_height(160)
            .width(Length::Fill),
            container(action).padding([0, 6]),
        ]
        .align_y(Vertical::Center),
    )
    .padding(2)
    .style(style::composer);

    let mut lower = column![].spacing(style::SP_2);
    if let Some(s) = suggestions(app) {
        lower = lower.push(s);
    }
    if app.reviewing {
        lower = lower.push(plan_review(app));
    }
    if let Some(p) = super::plan_panel::view(app) {
        lower = lower.push(p);
    }
    if let Some(b) = review_bar(app) {
        lower = lower.push(b);
    }
    if let Some(b) = proposal_bar(app) {
        lower = lower.push(b);
    }
    if let Some(c) = attachments(app) {
        lower = lower.push(c);
    }
    let lower = lower.push(modes).push(composer);

    column![
        pickers,
        history,
        container(lower)
            .max_width(style::READING_WIDTH)
            .center_x(Length::Fill),
    ]
    .spacing(style::SP_3)
    .padding(style::SP_4)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn is_step(line: &str) -> bool {
    let t = line.trim_start();
    let d = t.chars().take_while(|c| c.is_ascii_digit()).count();
    d > 0 && t[d..].starts_with(". ")
}

/// The plan as a card. The latest plan is open by default; older ones start collapsed.
fn plan_line<'a>(
    app: &'a App,
    index: usize,
    msg: &'a ChatMessage,
    active: bool,
) -> Element<'a, Message> {
    let latest = app.messages.iter().rposition(|m| m.role == Role::Plan) == Some(index);
    let steps = if active {
        None
    } else if latest && app.plan_run.is_some() {
        app.plan_run.as_ref().map(|r| r.plan.steps.len())
    } else {
        Some(msg.content.lines().filter(|l| is_step(l)).count()).filter(|n| *n > 0)
    };
    let label = match (active, steps) {
        (true, _) => "Planning".to_string(),
        (false, Some(1)) => "Plan · 1 step".to_string(),
        (false, Some(n)) => format!("Plan · {n} steps"),
        (false, None) => "Plan".to_string(),
    };
    // The latest plan is open by default; older ones start collapsed. A click flips either.
    let toggled = app.plan_open.contains(&index);
    let open = (latest != toggled) && !active && !msg.content.is_empty();
    let chevron = if open {
        icon_manager::chevron_down()
    } else {
        icon_manager::chevron_right()
    };

    // While the planner runs, show animated dots instead of a dead, disabled button.
    let head: Element<'a, Message> = if active {
        container(working_line(app, label)).padding([2, 6]).into()
    } else {
        button(icon_manager::label(chevron, label, 13, style::dim_color()))
            .padding([2, 6])
            .on_press_maybe((!msg.content.is_empty()).then_some(Message::TogglePlan(index)))
            .style(style::ghost_button)
            .into()
    };
    let mut col = column![head].spacing(style::SP_2);
    // The planner's reasoning: live while it thinks, then behind "Thought for Xs".
    if !msg.thinking.is_empty() {
        col = col.push(thinking_header(app, index, msg, active));
        if msg.show_thinking {
            col = col.push(thinking_panel(msg));
        }
    }
    if open {
        col = col.push(markdown::view(&msg.content));
        return container(col)
            .padding([style::SP_2, style::SP_3])
            .width(Length::Fill)
            .style(style::card)
            .into();
    }
    col.into()
}

fn ai_bubble<'a>(app: &'a App, index: usize, msg: &'a ChatMessage) -> Element<'a, Message> {
    let active = app.is_working() && index + 1 == app.messages.len();
    if msg.role == Role::Plan {
        return plan_line(app, index, msg, active);
    }

    let header = row![
        text("Virgo").size(style::T_12).font(style::semibold()),
        text(app.cfg.executor_model.as_str())
            .size(style::T_11)
            .style(style::dim),
        horizontal_space(),
        button(icon_manager::label(
            icon_manager::copy(),
            "Copy",
            11,
            icon_manager::muted()
        ))
        .padding([3, 8])
        .on_press_maybe((!msg.content.is_empty()).then(|| Message::Copy(msg.content.clone())))
        .style(style::ghost_button),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let mut body = column![header].spacing(style::SP_2);

    if !msg.thinking.is_empty() || (active && msg.content.is_empty()) {
        body = body.push(thinking_header(app, index, msg, active));
    }
    if msg.show_thinking && !msg.thinking.is_empty() {
        body = body.push(thinking_panel(msg));
    }
    if !msg.content.is_empty() {
        body = body.push(markdown::view(&msg.content));
        if active {
            body = body.push(working_line(app, "Writing".to_string()));
        }
    }
    if let Some(cards) = super::actions::view(app, index, &msg.content, active) {
        body = body.push(cards);
    }

    // Flat on the pane: no box.
    container(body).width(Length::Fill).padding([4, 0]).into()
}

fn thinking_header<'a>(
    app: &'a App,
    index: usize,
    msg: &'a ChatMessage,
    active: bool,
) -> Element<'a, Message> {
    let now = active && msg.think_secs.is_none() && msg.content.is_empty();
    let label = if now {
        match app.think_started {
            Some(t) => format!("Thinking {}s", t.elapsed().as_secs()),
            None => "Waiting for the model".to_string(),
        }
    } else if let Some(secs) = msg.think_secs {
        format!("Thought for {secs:.1}s")
    } else {
        String::new()
    };

    let mut line = row![].spacing(8).align_y(Vertical::Center);
    if now {
        line = line.push(working_dots(app));
    }
    line = line.push(text(label).size(style::T_12).style(style::dim));
    if !msg.thinking.is_empty() {
        line = line.push(
            button(text(if msg.show_thinking { "Hide" } else { "Show" }).size(style::T_12))
                .padding([2, 8])
                .on_press(Message::ToggleThinking(index))
                .style(style::ghost_button),
        );
    }
    line.into()
}

fn welcome<'a>() -> Element<'a, Message> {
    let starter = |label: &'static str| -> Element<'a, Message> {
        button(text(label).size(style::T_13))
            .padding([8, 14])
            .on_press(Message::InputChanged(label.to_string()))
            .style(style::ghost_button)
            .into()
    };
    container(
        column![
            text("Virgo AI").size(style::T_20).font(style::semibold()),
            text("Open a project folder in the right panel to chat about your code.")
                .size(style::T_13)
                .style(style::dim),
            row![
                starter("Explain this project"),
                starter("Find a bug in my code")
            ]
            .spacing(8),
            row![starter("Write tests"), starter("Suggest a refactor")].spacing(8),
        ]
        .spacing(style::SP_3)
        .align_x(Horizontal::Center),
    )
    .width(Length::Fill)
    .padding(60)
    .align_x(Horizontal::Center)
    .into()
}

/// Editable plan with Run / Discard, shown while `app.reviewing`.
/// Editable plan with Run / Discard, shown while `app.reviewing`.
fn plan_review(app: &App) -> Element<'_, Message> {
    let header = row![
        icon_manager::tint(icon_manager::lightbulb(), 18.0, icon_manager::muted()),
        column![
            text("Review the plan").size(15).font(style::bold()),
            text("Edit it however you like. The model will follow your version.")
                .size(12)
                .style(style::dim),
        ]
        .spacing(2),
    ]
    .spacing(10)
    .align_y(Vertical::Center);

    let editor = text_editor(&app.plan_editor)
        .on_action(Message::PlanAction)
        .padding(12)
        .size(14)
        .height(Length::Fixed(200.0))
        .style(style::plan_editor);

    let actions = row![
        button(icon_manager::label(
            icon_manager::check(),
            "Run plan",
            13,
            icon_manager::on_primary()
        ))
        .padding([9, 18])
        .on_press(Message::RunPlan)
        .style(style::primary_button),
        button(text("Discard").size(13))
            .padding([9, 16])
            .on_press(Message::DiscardPlan)
            .style(style::ghost_button),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    container(column![header, editor, actions].spacing(12))
        .padding(16)
        .width(Length::Fill)
        .style(style::card)
        .into()
}

/// Files with live changes waiting for a decision: open one, or accept / reject all.
fn review_bar(app: &App) -> Option<Element<'_, Message>> {
    let files = app.review_files();
    if files.is_empty() {
        return None;
    }
    let typing = app.is_typing_edit();
    let total: usize = files.iter().map(|(_, n)| n).sum();

    let mut bar = row![
        icon_manager::tint(icon_manager::lightbulb(), 14.0, icon_manager::accent()),
        text(if typing { "Writing:" } else { "To review:" })
            .size(12)
            .style(style::dim),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    for (file, count) in files.iter().take(4) {
        let name = file.rsplit('/').next().unwrap_or(file);
        bar = bar.push(
            button(text(format!("{} ({count})", short(name, 22))).size(12))
                .padding([4, 10])
                .on_press(Message::OpenFile(file.clone()))
                .style(style::chip),
        );
    }
    if files.len() > 4 {
        bar = bar.push(
            text(format!("+{} more", files.len() - 4))
                .size(12)
                .style(style::dim),
        );
    }

    Some(
        bar.push(horizontal_space())
            .push(
                button(text("Reject all").size(12))
                    .padding([4, 10])
                    .on_press(Message::RejectAllFiles)
                    .style(style::ghost_button),
            )
            .push(
                button(icon_manager::label(
                    icon_manager::check(),
                    format!("Accept all ({total})"),
                    12,
                    icon_manager::on_primary(),
                ))
                .padding([5, 12])
                .on_press_maybe((!typing).then_some(Message::AcceptAllFiles))
                .style(style::primary_button),
            )
            .into(),
    )
}

/// One chip per file the model wants to change. Click to stage it in the code panel.
fn proposal_bar(app: &App) -> Option<Element<'_, Message>> {
    if app.is_busy() || app.proposals.is_empty() {
        return None;
    }

    let mut files: Vec<&str> = Vec::new();
    for p in &app.proposals {
        if !files.contains(&p.file.as_str()) {
            files.push(p.file.as_str());
        }
    }

    let mut bar = row![
        icon_manager::tint(icon_manager::lightbulb(), 14.0, icon_manager::accent()),
        text("Proposed changes:").size(12).style(style::dim),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    for file in files {
        let count = app.proposals.iter().filter(|p| p.file == file).count();
        let name = file.rsplit('/').next().unwrap_or(file);
        bar = bar.push(
            button(text(format!("{} ({count})", short(name, 22))).size(12))
                .padding([4, 10])
                .on_press(Message::ReviewChange(file.to_string()))
                .style(style::chip),
        );
    }
    Some(bar.into())
}

// ---------------------------------------------------------------------------
// Project files: attached chips and @mention suggestions
// ---------------------------------------------------------------------------

/// Files attached from the project panel, shown above the composer.
fn attachments(app: &App) -> Option<Element<'_, Message>> {
    if app.project.is_none() || app.attached.is_empty() {
        return None;
    }

    let mut chips = row![icon_manager::tint(
        icon_manager::paperclip(),
        14.0,
        icon_manager::accent()
    )]
    .spacing(6)
    .align_y(Vertical::Center);

    for rel in app.attached.iter().take(3) {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        chips = chips.push(
            button(
                row![
                    text(short(name, 18)).size(12),
                    icon_manager::tint(icon_manager::x(), 11.0, icon_manager::muted()),
                ]
                .spacing(4)
                .align_y(Vertical::Center),
            )
            .padding([4, 10])
            .on_press(Message::ToggleAttach(rel.clone()))
            .style(style::chip),
        );
    }
    if app.attached.len() > 3 {
        chips = chips.push(
            text(format!("+{} more", app.attached.len() - 3))
                .size(12)
                .style(style::dim),
        );
    }

    Some(
        chips
            .push(horizontal_space())
            .push(
                button(text("Clear").size(12))
                    .padding([4, 8])
                    .on_press(Message::ClearAttached)
                    .style(style::ghost_button),
            )
            .into(),
    )
}

/// The `@query` being typed at the end of the input, if any.
fn mention_query(input: &str) -> Option<&str> {
    let token = input.rsplit(char::is_whitespace).next()?;
    token.strip_prefix('@')
}

/// Autocomplete list for `@` mentions.
fn suggestions(app: &App) -> Option<Element<'_, Message>> {
    let project = app.project.as_ref()?;
    let query = mention_query(&app.input)?;
    let hits = project.suggest(query, 6);
    if hits.is_empty() {
        return None;
    }

    let list = column(hits.into_iter().map(|entry| {
        let icon = if entry.is_dir {
            icon_manager::folder()
        } else {
            icon_manager::file_text()
        };
        button(icon_manager::label(
            icon,
            entry.rel.as_str(),
            13,
            icon_manager::muted(),
        ))
        .width(Length::Fill)
        .padding([6, 10])
        .on_press(Message::PickMention(entry.rel.clone()))
        .style(style::nav_item(false))
        .into()
    }))
    .spacing(2);

    Some(container(list).padding(6).style(style::card).into())
}

// ---------------------------------------------------------------------------
// Messages
// ---------------------------------------------------------------------------

fn bubble<'a>(app: &'a App, index: usize, msg: &'a ChatMessage) -> Element<'a, Message> {
    match msg.role {
        Role::User => {
            let mut content = column![text(msg.content.as_str()).size(15)].spacing(6);
            if !msg.files.is_empty() {
                content = content.push(
                    text(format!("Files: {}", msg.files.join(", ")))
                        .size(11)
                        .style(style::dim),
                );
            }
            let bubble = container(content)
                .padding([10, 16])
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

/// Three dots that light up in turn, so it is obvious a model is still working.
fn working_dots<'a>(app: &App) -> Element<'a, Message> {
    let lit = ((app.tick / 2) % 3) as usize;
    row((0..3).map(|i| {
        let a = if i == lit { 1.0 } else { 0.25 };
        container(horizontal_space())
            .width(Length::Fixed(6.0))
            .height(Length::Fixed(6.0))
            .style(move |_t: &Theme| container::Style {
                background: Some(Background::Color(Color {
                    a,
                    ..style::signal()
                })),
                border: Border {
                    radius: 3.0.into(),
                    ..Border::default()
                },
                ..Default::default()
            })
            .into()
    }))
    .spacing(4)
    .align_y(Vertical::Center)
    .into()
}

/// Animated "a model is working" line: pulsing dots followed by a label.
fn working_line<'a>(app: &App, label: String) -> Element<'a, Message> {
    row![
        working_dots(app),
        text(label).size(style::T_12).style(style::dim)
    ]
    .spacing(8)
    .align_y(Vertical::Center)
    .into()
}

/// The reasoning text, shown live while the model thinks.
fn thinking_panel<'a>(msg: &'a ChatMessage) -> Element<'a, Message> {
    container(
        text(msg.thinking.as_str())
            .size(style::T_12)
            .style(style::dim),
    )
    .padding(10)
    .width(Length::Fill)
    .style(style::thinking_box)
    .into()
}
