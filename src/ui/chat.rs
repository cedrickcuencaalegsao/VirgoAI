use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    button, checkbox, column, container, horizontal_space, pick_list, row, scrollable, text,
    text_editor, text_input, Space,
};
use iced::{Element, Length, Theme};

use super::{markdown, short, style};
use crate::app::{App, Message};
use crate::icons;
use agent::{ChatMessage, Role};

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
                .width(Length::Fill)
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

    let mode = if app.cfg.use_planner {
        "Plan first, then answer"
    } else {
        "Direct answer"
    };
    let subtitle = match app.chat_workspace_name() {
        Some(name) => format!("{mode} · Project: {name}"),
        None => mode.to_string(),
    };

    let panel_color = if app.show_panel {
        icons::accent()
    } else {
        icons::muted()
    };
    let terminal_color = if app.show_terminal {
        icons::accent()
    } else {
        icons::muted()
    };

    let back: Element<'_, Message> = match app.chat_workspace() {
        Some(slug) => button(icons::label(
            icons::folder_kanban(),
            "Project",
            12,
            icons::muted(),
        ))
        .padding([7, 12])
        .on_press(Message::OpenWorkspace(slug))
        .style(style::ghost_button)
        .into(),
        None => Space::new(Length::Shrink, Length::Shrink).into(),
    };

    let top = row![
        column![
            text("Conversation").size(22).font(style::bold()),
            text(short(&subtitle, 40)).size(12).style(style::dim),
        ]
        .spacing(2),
        horizontal_space(),
        back,
        button(icons::tint(icons::refresh_cw(), 16.0, icons::muted()))
            .padding(8)
            .on_press(Message::RefreshModels)
            .style(style::ghost_button),
        button(icons::tint(icons::terminal(), 18.0, terminal_color))
            .padding(8)
            .on_press(Message::ToggleTerminal)
            .style(style::ghost_button),
        button(icons::tint(icons::panel_right(), 18.0, panel_color))
            .padding(8)
            .on_press(Message::TogglePanel)
            .style(style::ghost_button),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    // Model pickers get their own row, so they never get squeezed.
    let header = column![top, pickers].spacing(10);

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
    let n = app.revertable();
    let remember = row![
        checkbox("Edit mode (plan, review, then apply code)", app.edit_mode)
            .on_toggle_maybe(
                (app.project.is_some() && !app.is_busy()).then_some(Message::ToggleEditMode)
            )
            .size(16)
            .text_size(12),
        horizontal_space(),
        button(icons::label(
            icons::undo_2(),
            format!("Revert last AI edit ({n})"),
            12,
            icons::muted()
        ))
        .padding([6, 12])
        .on_press_maybe((n > 0).then_some(Message::RevertLast))
        .style(style::ghost_button),
        horizontal_space(),
        button(icons::label(
            icons::database(),
            "Remember last reply",
            12,
            icons::muted()
        ))
        .padding([6, 12])
        .on_press_maybe(app.has_reply().then_some(Message::RememberLast))
        .style(style::ghost_button),
    ];

    let can_send = !app.input.trim().is_empty();
    let action: Element<'_, Message> = if app.is_busy() {
        button(icons::label(
            icons::square(),
            "Stop",
            14,
            icons::on_danger(),
        ))
        .padding([10, 18])
        .on_press(Message::Stop)
        .style(style::danger_button)
        .into()
    } else {
        button(icons::label(icons::send(), "Send", 14, icons::on_primary()))
            .padding([10, 18])
            .on_press_maybe(can_send.then_some(Message::Send))
            .style(style::primary_button)
            .into()
    };

    let placeholder = if app.project.is_some() {
        "Message Virgo... (type @ to mention a project file)"
    } else {
        "Message Virgo..."
    };

    let composer = container(
        row![
            text_input(placeholder, &app.input)
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

    let mut page = column![header, history].spacing(12);
    if let Some(suggestions) = suggestions(app) {
        page = page.push(suggestions);
    }
    if app.reviewing {
        page = page.push(plan_review(app));
    }
    if let Some(bar) = proposal_bar(app) {
        page = page.push(bar);
    }
    if let Some(chips) = attachments(app) {
        page = page.push(chips);
    }
    page.push(remember)
        .push(composer)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Editable plan with Run / Discard, shown while `app.reviewing`.
fn plan_review(app: &App) -> Element<'_, Message> {
    container(
        column![
            row![
                text("Review the plan").size(14).font(style::bold()),
                text("Edit it however you like. The model will follow your version.")
                    .size(12)
                    .style(style::dim),
            ]
            .spacing(10)
            .align_y(Vertical::Center),
            text_editor(&app.plan_editor)
                .on_action(Message::PlanAction)
                .height(Length::Fixed(180.0)),
            row![
                button(icons::label(
                    icons::check(),
                    "Run plan",
                    13,
                    icons::on_primary()
                ))
                .padding([8, 14])
                .on_press(Message::RunPlan)
                .style(style::primary_button),
                button(text("Discard").size(13))
                    .padding([8, 14])
                    .on_press(Message::DiscardPlan)
                    .style(style::ghost_button),
            ]
            .spacing(8),
        ]
        .spacing(8),
    )
    .padding(12)
    .width(Length::Fill)
    .style(style::plan_card)
    .into()
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
        icons::tint(icons::lightbulb(), 14.0, icons::accent()),
        text("Proposed changes:").size(12).style(style::dim),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    for file in files {
        let count = app.proposals.iter().filter(|p| p.file == file).count();
        let name = file.rsplit('/').next().unwrap_or(file);
        bar = bar.push(
            button(text(format!("{} ({count})", short(name, 22))).size(12))
                .padding([4, 8])
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

    let mut chips = row![icons::tint(icons::paperclip(), 14.0, icons::accent())]
        .spacing(6)
        .align_y(Vertical::Center);

    for rel in app.attached.iter().take(3) {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        chips = chips.push(
            button(
                row![
                    text(short(name, 18)).size(12),
                    icons::tint(icons::x(), 11.0, icons::muted()),
                ]
                .spacing(4)
                .align_y(Vertical::Center),
            )
            .padding([4, 8])
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
            icons::folder()
        } else {
            icons::file_text()
        };
        button(icons::label(icon, entry.rel.as_str(), 13, icons::muted()))
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

fn welcome<'a>() -> Element<'a, Message> {
    container(
        column![
            text("Virgo AI")
                .size(40)
                .font(style::bold())
                .style(style::accent),
            text(
                "A planner model thinks it through, an executor model answers, \
                 and your markdown notes keep the memory. Open a project folder \
                 in the right panel to chat about your code."
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
            let mut content = column![text(msg.content.as_str()).size(15)].spacing(6);
            if !msg.files.is_empty() {
                content = content.push(
                    text(format!("Files: {}", msg.files.join(", ")))
                        .size(11)
                        .style(style::dim),
                );
            }
            let bubble = container(content)
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
    let active = app.is_working() && index + 1 == app.messages.len();

    let header = row![
        text(label)
            .size(12)
            .font(style::bold())
            .style(style::accent),
        text(model).size(11).style(style::dim),
        horizontal_space(),
        button(icons::label(icons::copy(), "Copy", 11, icons::muted()))
            .padding([3, 10])
            .on_press_maybe((!msg.content.is_empty()).then(|| Message::Copy(msg.content.clone())))
            .style(style::ghost_button),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

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
        body = body.push(markdown::view(&msg.content));
    }

    // Edit / run cards, shown live while streaming (completed blocks only).
    if msg.role == Role::Assistant {
        if let Some(cards) = super::actions::view(app, index, &msg.content, active) {
            body = body.push(cards);
        }
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
        let secs = app
            .think_started
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);
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
