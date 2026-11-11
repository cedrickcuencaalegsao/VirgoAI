use iced::alignment::Vertical;
use iced::widget::{button, column, container, horizontal_space, row, text};
use iced::{Element, Font, Length, Theme};

use super::{short, style};
use crate::app::{ActionStatus, App, Message};
use crate::icons;
use crate::workspace::{self, Action, Hunk};

const PREVIEW_LINES: usize = 6;

pub fn view<'a>(app: &App, msg_idx: usize, content: &str) -> Option<Element<'a, Message>> {
    let actions = workspace::parse_actions(content);
    if actions.is_empty() {
        return None;
    }

    let mut list = column![].spacing(8);
    let mut unapplied = 0;
    for (i, action) in actions.iter().enumerate() {
        let status = app.action_status.get(&(msg_idx, i));
        match action {
            Action::Edit { path, hunks } => {
                if !matches!(status, Some(ActionStatus::Applied(_))) {
                    unapplied += 1;
                }
                list = list.push(edit_card(app, msg_idx, i, path, hunks, status));
            }
            Action::Run(command) => {
                list = list.push(run_card(app, msg_idx, i, command, status));
            }
        }
    }

    if unapplied > 1 {
        list = list.push(
            button(icons::label(
                icons::check(),
                format!("Apply all edits ({unapplied})"),
                13,
                icons::on_primary(),
            ))
            .padding([7, 14])
            .on_press_maybe(app.project.is_some().then_some(Message::ApplyAll(msg_idx)))
            .style(style::primary_button),
        );
    }
    Some(list.into())
}

fn line_count(text: &str) -> usize {
    if text.is_empty() { 0 } else { text.lines().count() }
}

fn edit_card<'a>(
    app: &App,
    msg_idx: usize,
    i: usize,
    path: &str,
    hunks: &[Hunk],
    status: Option<&ActionStatus>,
) -> Element<'a, Message> {
    let added: usize = hunks.iter().map(|h| line_count(&h.replace)).sum();
    let removed: usize = hunks.iter().map(|h| line_count(&h.search)).sum();

    let mut header = row![
        icons::tint(icons::file_text(), 15.0, icons::accent()),
        text(path.to_string()).size(13).font(style::bold()),
        text(format!("+{added}")).size(12).style(style::success),
        text(format!("-{removed}")).size(12).style(style::danger_text),
        horizontal_space(),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    match status {
        Some(ActionStatus::Applied(id)) => {
            header = header
                .push(text("Applied").size(12).style(style::success))
                .push(
                    button(icons::label(icons::undo_2(), "Revert", 12, icons::muted()))
                        .padding([5, 10])
                        .on_press(Message::RevertChange(*id))
                        .style(style::ghost_button),
                );
        }
        _ => {
            header = header.push(
                button(icons::label(icons::check(), "Apply", 12, icons::on_primary()))
                    .padding([5, 12])
                    .on_press_maybe(
                        app.project.is_some().then_some(Message::ApplyAction(msg_idx, i)),
                    )
                    .style(style::primary_button),
            );
        }
    }

    let mut body = column![header].spacing(6);
    if let Some(ActionStatus::Failed(error)) = status {
        body = body.push(text(error.clone()).size(12).style(style::danger_text));
    }
    body = body.push(diff_preview(hunks));

    container(body).padding(10).width(Length::Fill).style(style::code_block).into()
}

fn diff_preview<'a>(hunks: &[Hunk]) -> Element<'a, Message> {
    let mut rows = column![].spacing(0);
    for hunk in hunks {
        for (marker, block, added) in [("-", &hunk.search, false), ("+", &hunk.replace, true)] {
            for line in block.lines().take(PREVIEW_LINES) {
                rows = rows.push(diff_row(marker, line, added));
            }
            let more = block.lines().count().saturating_sub(PREVIEW_LINES);
            if more > 0 {
                rows = rows.push(text(format!("   ... {more} more line(s)")).size(11).style(style::dim));
            }
        }
    }
    rows.into()
}

fn diff_row<'a>(marker: &str, line: &str, added: bool) -> Element<'a, Message> {
    container(
        text(format!("{marker} {}", short(line, 110))).font(Font::MONOSPACE).size(12),
    )
    .width(Length::Fill)
    .padding([1, 6])
    .style(move |theme: &Theme| {
        if added { style::row_added(theme) } else { style::row_removed(theme) }
    })
    .into()
}

fn run_card<'a>(
    app: &App,
    msg_idx: usize,
    i: usize,
    command: &str,
    status: Option<&ActionStatus>,
) -> Element<'a, Message> {
    let (note, label) = match status {
        Some(ActionStatus::Ran(Some(code))) => (format!("exit {code}"), "Run again"),
        Some(ActionStatus::Ran(None)) => ("no exit code".to_string(), "Run again"),
        _ => (String::new(), "Run"),
    };
    let can_run = app.project.is_some() && !app.term_running;

    container(
        row![
            icons::tint(icons::terminal(), 15.0, icons::accent()),
            text(command.to_string()).font(Font::MONOSPACE).size(13).width(Length::Fill),
            text(note).size(12).style(style::dim),
            button(icons::label(icons::play(), label, 12, icons::on_primary()))
                .padding([5, 12])
                .on_press_maybe(can_run.then_some(Message::RunAction(msg_idx, i)))
                .style(style::primary_button),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .padding(10)
    .width(Length::Fill)
    .style(style::code_block)
    .into()
}