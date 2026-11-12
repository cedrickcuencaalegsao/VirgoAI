//! Middle panel: the open file with a suggestion button on every line.
//! AI changes show as red (original) and green (new) rows until applied.
//!
//! Only the rows near the scroll position are built, so big files stay fast.

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    Space, Svg, button, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Element, Font, Length, Theme};

use super::{short, style};
use crate::app::{App, Message};
use crate::icons;
use code::{self, CodeDoc, RowKind, RowRef};

/// Every row has exactly this height, which is what makes windowing possible.
const ROW_H: f32 = 24.0;
/// Extra rows built above and below the visible area.
const OVERSCAN: usize = 12;
const GUTTER_W: f32 = 40.0;
const MARKER_W: f32 = 12.0;
const ACTIONS_W: f32 = 56.0;

pub fn scroll_id() -> scrollable::Id {
    scrollable::Id::new("code-scroll")
}

pub fn view(app: &App) -> Element<'_, Message> {
    match &app.code {
        Some(doc) => panel(app, doc),
        None => column![].into(),
    }
}

fn icon_button<'a>(icon: Svg<'a>, message: Message) -> Element<'a, Message> {
    button(icons::tint(icon, 16.0, icons::muted()))
        .padding(7)
        .on_press(message)
        .style(style::ghost_button)
        .into()
}

fn panel<'a>(app: &'a App, doc: &'a CodeDoc) -> Element<'a, Message> {
    let (dir, name) = match doc.rel.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", doc.rel.as_str()),
    };

    // ---- header ----
    let header = row![
        icons::tint(icons::file_text(), 18.0, icons::accent()),
        column![
            text(name).size(15).font(style::bold()),
            text(if dir.is_empty() {
                "(project root)"
            } else {
                dir
            })
            .size(11)
            .style(style::dim),
        ]
        .spacing(1),
        horizontal_space(),
        icon_button(icons::refresh_cw(), Message::ReloadCode),
        icon_button(icons::x(), Message::CloseCode),
    ]
    .spacing(10)
    .align_y(Vertical::Center);

    // ---- apply toolbar ----
    let pending = doc.edits.len();
    let summary: Element<'a, Message> = if pending == 0 {
        text("Click the bulb on a line to ask the model for a change.")
            .size(12)
            .style(style::dim)
            .into()
    } else {
        text(format!("{pending} AI change(s) pending"))
            .size(12)
            .style(style::success)
            .into()
    };

    let toolbar = row![
        container(summary).width(Length::Fill),
        button(icons::label(
            icons::undo_2(),
            "Undo apply",
            12,
            icons::muted()
        ))
        .padding([6, 10])
        .on_press_maybe(doc.previous.is_some().then_some(Message::UndoApply))
        .style(style::ghost_button),
        button(text("Discard").size(12))
            .padding([6, 10])
            .on_press_maybe((pending > 0).then_some(Message::DiscardEdits))
            .style(style::ghost_button),
        button(icons::label(
            icons::check(),
            format!("Apply ({pending})"),
            13,
            icons::on_primary()
        ))
        .padding([7, 14])
        .on_press_maybe((pending > 0).then_some(Message::ApplyEdits))
        .style(style::primary_button),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    // ---- code rows (only the visible window is built) ----
    let rows = doc.rows();
    let total = rows.len();
    let first = ((app.code_scroll_y / ROW_H) as usize)
        .saturating_sub(OVERSCAN)
        .min(total);
    let shown = (app.code_view_h / ROW_H).ceil() as usize + OVERSCAN * 2;
    let last = (first + shown).min(total);

    let width = doc.text_width();
    let mut lines = column![Space::with_height(Length::Fixed(first as f32 * ROW_H))];
    for row_ref in &rows[first..last] {
        lines = lines.push(code_row(app, row_ref, width));
    }
    lines = lines.push(Space::with_height(Length::Fixed(
        (total - last) as f32 * ROW_H,
    )));

    let area = container(
        scrollable(lines)
            .id(scroll_id())
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::default(),
                horizontal: scrollable::Scrollbar::default(),
            })
            .on_scroll(Message::CodeScrolled)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .padding(4)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(style::code_area);

    let mut page = column![header, toolbar, area].spacing(10);
    if let Some(bar) = suggest_bar(app, doc) {
        page = page.push(bar);
    }

    container(page)
        .padding(14)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style::card)
        .into()
}

fn code_row<'a>(app: &'a App, r: &RowRef<'_>, width: f32) -> Element<'a, Message> {
    let kind = r.kind;
    let selected = app.suggest_line == Some(r.line) && kind != RowKind::Added;
    let busy = app.suggesting.is_some();

    let number = r.number.map(|n| n.to_string()).unwrap_or_default();
    let gutter = text(number)
        .size(12)
        .font(Font::MONOSPACE)
        .style(style::dim)
        .width(Length::Fixed(GUTTER_W))
        .align_x(Horizontal::Right);

    let marker = match kind {
        RowKind::Added => text("+").style(style::success),
        RowKind::Removed => text("-").style(style::danger_text),
        RowKind::Normal => text(" "),
    }
    .size(13)
    .font(Font::MONOSPACE)
    .width(Length::Fixed(MARKER_W));

    // Suggest (bulb) on every line; revert on lines that have a pending change.
    let actions: Element<'a, Message> = if r.show_actions {
        let bulb_color = if selected {
            icons::accent()
        } else {
            icons::muted()
        };
        let mut buttons = row![
            button(icons::tint(icons::lightbulb(), 13.0, bulb_color))
                .padding([5, 6])
                .on_press_maybe((!busy).then_some(Message::SuggestLine(r.line)))
                .style(style::ghost_button)
        ];
        if kind == RowKind::Removed {
            buttons = buttons.push(
                button(icons::tint(icons::undo_2(), 13.0, icons::muted()))
                    .padding([5, 6])
                    .on_press(Message::RevertLine(r.line))
                    .style(style::ghost_button),
            );
        }
        container(buttons).width(Length::Fixed(ACTIONS_W)).into()
    } else {
        Space::with_width(Length::Fixed(ACTIONS_W)).into()
    };

    let code = text(code::display(r.src))
        .size(13)
        .font(Font::MONOSPACE)
        .width(Length::Fixed(width));

    container(
        row![gutter, marker, actions, code]
            .spacing(4)
            .align_y(Vertical::Center),
    )
    .height(Length::Fixed(ROW_H))
    .padding([0, 6])
    .style(move |theme: &Theme| match kind {
        RowKind::Added => style::row_added(theme),
        RowKind::Removed => style::row_removed(theme),
        RowKind::Normal if selected => style::row_selected(theme),
        RowKind::Normal => container::Style::default(),
    })
    .into()
}

/// Shown after clicking a bulb: ask the model what to do with that line.
fn suggest_bar<'a>(app: &'a App, doc: &'a CodeDoc) -> Option<Element<'a, Message>> {
    let line = app.suggest_line?;
    let current = doc.lines.get(line)?;
    let waiting = app.suggesting.is_some();

    let top = row![
        icons::tint(icons::lightbulb(), 15.0, icons::accent()),
        text(format!("Line {}", line + 1))
            .size(13)
            .font(style::bold()),
        text(short(current.trim(), 48))
            .size(12)
            .font(Font::MONOSPACE)
            .style(style::dim),
        horizontal_space(),
        icon_button(icons::x(), Message::CancelSuggest),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let action_label = if waiting {
        "Asking model..."
    } else {
        "Suggest"
    };
    let input = row![
        text_input(
            "How should this line change? (empty = improve it)",
            &app.suggest_input
        )
        .on_input(Message::SuggestInputChanged)
        .on_submit(Message::SubmitSuggestion)
        .padding(9)
        .size(13)
        .style(style::field),
        button(icons::label(
            icons::lightbulb(),
            action_label,
            13,
            icons::on_primary()
        ))
        .padding([8, 14])
        .on_press_maybe((!waiting).then_some(Message::SubmitSuggestion))
        .style(style::primary_button),
    ]
    .spacing(6);

    let footer = text(format!("Uses {}", app.cfg.executor_model))
        .size(11)
        .style(style::dim);

    Some(
        container(column![top, input, footer].spacing(8))
            .padding(10)
            .width(Length::Fill)
            .style(style::card)
            .into(),
    )
}
