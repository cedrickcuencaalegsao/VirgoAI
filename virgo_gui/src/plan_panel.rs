//! Progress of a structured plan: one row per step, with Retry / Skip when a step fails.
//! Shown above the composer while a plan runs and after it ends, until it is dismissed.

use iced::alignment::Vertical;
use iced::widget::{Space, button, column, container, horizontal_space, row, text};
use iced::{Element, Length};

use agent::plan::StepStatus;
use app::{App, Message};

use super::short;

pub fn view(app: &App) -> Option<Element<'_, Message>> {
    let run = app.plan_run.as_ref()?;
    if app.reviewing {
        return None; // the review box is on screen instead
    }

    let total = run.status.len();
    let finished = run.count(|s| matches!(s, StepStatus::Done | StepStatus::Skipped));

    let header = row![
        icon_manager::tint(icon_manager::lightbulb(), 15.0, icon_manager::accent()),
        text(format!("Plan: {finished}/{total} steps"))
            .size(13)
            .font(style::bold()),
        horizontal_space(),
        button(icon_manager::tint(
            icon_manager::x(),
            12.0,
            icon_manager::muted()
        ))
        .padding(6)
        .on_press_maybe((!app.is_busy()).then_some(Message::ClosePlanRun))
        .style(style::ghost_button),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let mut steps = column![].spacing(4);
    for (step, status) in run.plan.steps.iter().zip(&run.status) {
        let mark = match status {
            StepStatus::Done => text("\u{2713}").style(style::success),
            StepStatus::Running => text("\u{25B6}").style(style::accent),
            StepStatus::Failed(_) => text("\u{2717}").style(style::danger_text),
            StepStatus::Skipped => text("\u{2013}").style(style::dim),
            StepStatus::Todo => text("\u{25CB}").style(style::dim),
        };
        let mark = mark.size(13).width(Length::Fixed(16.0));

        let line = row![
            mark,
            text(short(&step.file, 30)).size(12).font(style::bold()),
            text(short(&step.change, 50)).size(12).style(style::dim),
        ]
        .spacing(8)
        .align_y(Vertical::Center);

        let mut item = column![line].spacing(2);
        if let StepStatus::Failed(error) = status {
            let first = error.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            item = item.push(row![
                Space::with_width(Length::Fixed(24.0)),
                text(short(first.trim_start_matches("- "), 100))
                    .size(11)
                    .style(style::danger_text),
            ]);
        }
        steps = steps.push(item);
    }

    let mut body = column![header, steps].spacing(8);

    // A failed step stops the run until the user decides.
    if run.failed().is_some() && !app.is_busy() {
        body = body.push(
            row![
                button(text("Retry step").size(12))
                    .padding([6, 12])
                    .on_press(Message::RetryStep)
                    .style(style::primary_button),
                button(text("Skip step").size(12))
                    .padding([6, 12])
                    .on_press(Message::SkipStep)
                    .style(style::ghost_button),
            ]
            .spacing(8),
        );
    }

    Some(
        container(body)
            .padding(12)
            .width(Length::Fill)
            .style(style::plan_card)
            .into(),
    )
}