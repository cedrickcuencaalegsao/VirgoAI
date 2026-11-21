//! The hero. Rows are exactly ROW_H tall (23px content + a 1px rule), so
//! windowing and scroll maths stay exact. Changed rows: 14% fill + 2px edge.

use iced::alignment::{Horizontal, Vertical};
use iced::font::{Style as FontStyle, Weight};
use iced::widget::text_editor::{Binding, Status};
use iced::widget::{
    Row, Space, Svg, button, column, container, horizontal_space, mouse_area, row, scrollable,
    text, text_input,
};
use iced::widget::{rich_text, span, text_editor};
use iced::{Color, keyboard};
use iced::{Element, Length, Theme};

use super::short;
use app::{App, Message};
use code::{self, CodeDoc, RowKind, RowRef};

const ROW_H: f32 = 24.0;
const OVERSCAN: usize = 12;
const GUTTER_W: f32 = 40.0;
const MARKER_W: f32 = 12.0;
const ACTIONS_W: f32 = 84.0;

pub fn view(app: &App) -> Element<'_, Message> {
    match &app.code {
        Some(doc) => panel(app, doc),
        None => column![].into(),
    }
}

fn icon_button<'a>(icon: Svg<'a>, message: Message) -> Element<'a, Message> {
    button(icon_manager::tint(icon, 16.0, icon_manager::muted()))
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

    let header = row![
        icon_manager::tint(icon_manager::file_text(), 16.0, icon_manager::muted()),
        column![
            text(name).size(style::T_15).font(style::semibold()),
            text(if dir.is_empty() { "project root" } else { dir })
                .size(style::T_11)
                .style(style::dim),
        ]
        .spacing(1),
        horizontal_space(),
        icon_button(icon_manager::pencil(), Message::ToggleEdit),
        icon_button(icon_manager::refresh_cw(), Message::ReloadCode),
        icon_button(icon_manager::x(), Message::CloseCode),
    ]
    .spacing(style::SP_3)
    .align_y(Vertical::Center);

    if app.is_editing() {
        return edit_panel(app, doc, header.into());
    }

    // ---- toolbar: "3 changes in main.rs" ... Reject all, Accept ----
    let pending = doc.change_count();
    let live = doc.is_live();

    let summary: Element<'a, Message> = if live {
        text("AI is editing this file")
            .size(style::T_12)
            .style(style::dim)
            .into()
    } else if pending > 0 {
        text(format!(
            "{pending} change{} in {name}",
            if pending == 1 { "" } else { "s" }
        ))
        .size(style::T_13)
        .into()
    } else {
        text("Hover a line and click the bulb to ask for a change.")
            .size(style::T_12)
            .style(style::dim)
            .into()
    };

    let mut toolbar = row![container(summary).width(Length::Fill)]
        .spacing(style::SP_2)
        .align_y(Vertical::Center);

    if app.is_working() && live {
        toolbar = toolbar.push(
            button(text("Stop").size(style::T_12))
                .padding([6, 10])
                .on_press(Message::Stop)
                .style(style::ghost_button),
        );
    }
    if doc.can_undo() {
        toolbar = toolbar.push(
            button(icon_manager::label(
                icon_manager::undo_2(),
                "Undo",
                12,
                icon_manager::muted(),
            ))
            .padding([6, 10])
            .on_press(Message::UndoApply)
            .style(style::ghost_button),
        );
    }
    if pending > 0 && !live {
        toolbar = toolbar
            .push(
                button(text("Reject all").size(style::T_12))
                    .padding([6, 10])
                    .on_press(Message::DiscardEdits)
                    .style(style::ghost_button),
            )
            .push(
                button(text("Accept").size(style::T_13))
                    .padding([6, 14])
                    .on_press(Message::ApplyEdits)
                    .style(style::primary_button),
            );
    }

    // ---- rows (only the visible window is built) ----
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
            .id(shared::code_scroll_id())
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::default(),
                horizontal: scrollable::Scrollbar::default(),
            })
            .on_scroll(Message::CodeScrolled)
            .style(style::scroll)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(style::code_area);

    let mut page = column![header, toolbar, area].spacing(style::SP_3);
    if let Some(bar) = suggest_bar(app, doc) {
        page = page.push(bar);
    }

    container(page)
        .padding(style::SP_4)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn code_row<'a>(app: &'a App, r: &RowRef<'_>, width: f32) -> Element<'a, Message> {
    let kind = r.kind;
    let selected = app.suggest_line == Some(r.line) && kind != RowKind::Added;
    let hovered = app.hover_line == Some(r.line) && kind != RowKind::Added;
    let busy = app.suggesting.is_some();

    let number = r.number.map(|n| n.to_string()).unwrap_or_default();
    let gutter = text(number)
        .size(style::T_12)
        .font(style::mono())
        .style(style::dim)
        .width(Length::Fixed(GUTTER_W))
        .align_x(Horizontal::Right);

    let marker = match kind {
        RowKind::Added => text("+").style(style::success),
        RowKind::Removed => text("-").style(style::danger_text),
        RowKind::Normal => text(" "),
    }
    .size(style::T_13)
    .font(style::mono())
    .width(Length::Fixed(MARKER_W));

    // Actions column keeps its width, so nothing shifts when the bulb appears.
    let actions: Element<'a, Message> = if let (Some(h), RowKind::Removed) = (r.hunk, kind) {
        if h == code::LIVE_ID {
            container(text("...").size(style::T_12).style(style::dim))
                .width(Length::Fixed(ACTIONS_W))
                .into()
        } else {
            container(row![
                button(icon_manager::tint(
                    icon_manager::check(),
                    13.0,
                    style::success_color()
                ))
                .padding([5, 6])
                .on_press(Message::AcceptHunk(h))
                .style(style::ghost_button),
                button(icon_manager::tint(
                    icon_manager::x(),
                    13.0,
                    icon_manager::muted()
                ))
                .padding([5, 6])
                .on_press(Message::RejectHunk(h))
                .style(style::ghost_button),
            ])
            .width(Length::Fixed(ACTIONS_W))
            .into()
        }
    } else if r.show_actions {
        let mut buttons: Row<'a, Message> = Row::new();
        if selected || hovered {
            let color = if selected {
                style::text_color()
            } else {
                icon_manager::muted()
            };
            buttons = buttons.push(
                button(icon_manager::tint(icon_manager::lightbulb(), 13.0, color))
                    .padding([5, 6])
                    .on_press_maybe((!busy).then_some(Message::SuggestLine(r.line)))
                    .style(style::ghost_button),
            );
        }
        if kind == RowKind::Removed {
            buttons = buttons.push(
                button(icon_manager::tint(
                    icon_manager::undo_2(),
                    13.0,
                    icon_manager::muted(),
                ))
                .padding([5, 6])
                .on_press(Message::RevertLine(r.line))
                .style(style::ghost_button),
            );
        }
        container(buttons).width(Length::Fixed(ACTIONS_W)).into()
    } else {
        Space::with_width(Length::Fixed(ACTIONS_W)).into()
    };

    let code: Element<'a, Message> = match r.spans {
        Some(spans) if !spans.is_empty() => colored(spans, width),
        _ => text(code::display(r.src))
            .size(style::T_13)
            .font(style::mono())
            .width(Length::Fixed(width))
            .into(),
    };

    let edge: Element<'a, Message> = match kind {
        RowKind::Added => container(Space::new(Length::Fixed(2.0), Length::Fill))
            .style(style::edge(style::success_color()))
            .into(),
        RowKind::Removed => container(Space::new(Length::Fixed(2.0), Length::Fill))
            .style(style::edge(style::danger_color()))
            .into(),
        RowKind::Normal => Space::with_width(Length::Fixed(2.0)).into(),
    };

    let content = row![
        edge,
        row![gutter, marker, actions, code]
            .spacing(4)
            .padding([0, 6])
            .align_y(Vertical::Center),
    ]
    .height(Length::Fill)
    .align_y(Vertical::Center);

    let body = container(content)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_H - 1.0))
        .style(move |theme: &Theme| match kind {
            RowKind::Added => style::row_added(theme),
            RowKind::Removed => style::row_removed(theme),
            RowKind::Normal => container::Style::default(),
        });

    // The current line is marked by a 1px line under it, not a fill.
    let rule_style: fn(&Theme) -> container::Style = if selected {
        style::divider
    } else {
        style::clear
    };
    let rule = container(Space::new(Length::Fill, Length::Fixed(1.0)))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(rule_style);

    mouse_area(column![body, rule])
        .on_enter(Message::HoverLine(Some(r.line)))
        .on_exit(Message::HoverLeave(r.line))
        .into()
}

fn suggest_bar<'a>(app: &'a App, doc: &'a CodeDoc) -> Option<Element<'a, Message>> {
    let line = app.suggest_line?;
    let current = doc.lines.get(line)?;
    let waiting = app.suggesting.is_some();

    let top = row![
        text(format!("Line {}", line + 1))
            .size(style::T_13)
            .font(style::semibold()),
        text(short(current.trim(), 48))
            .size(style::T_12)
            .font(style::mono())
            .style(style::dim),
        horizontal_space(),
        icon_button(icon_manager::x(), Message::CancelSuggest),
    ]
    .spacing(style::SP_2)
    .align_y(Vertical::Center);

    let input = row![
        text_input(
            "How should this line change? (empty = improve it)",
            &app.suggest_input
        )
        .on_input(Message::SuggestInputChanged)
        .on_submit(Message::SubmitSuggestion)
        .padding(9)
        .size(style::T_13)
        .style(style::field),
        // Ghost, so the pane keeps a single signal element (Accept).
        button(
            text(if waiting {
                "Asking model..."
            } else {
                "Suggest"
            })
            .size(style::T_13)
        )
        .padding([8, 14])
        .on_press_maybe((!waiting).then_some(Message::SubmitSuggestion))
        .style(style::ghost_button),
    ]
    .spacing(6);

    Some(
        container(
            column![
                top,
                input,
                text(format!("Uses {}", app.cfg.executor_model))
                    .size(style::T_11)
                    .style(style::dim)
            ]
            .spacing(style::SP_2),
        )
        .padding(style::SP_3)
        .width(Length::Fill)
        .style(style::card)
        .into(),
    )
}

/// Syntax colours, muted so changed rows read first.
fn colored<'a>(spans: &[syntax::Span], width: f32) -> Element<'a, Message> {
    let pieces: Vec<_> = spans
        .iter()
        .map(|s| {
            let mut font = style::mono();
            if s.bold {
                font.weight = Weight::Bold;
            }
            if s.italic {
                font.style = FontStyle::Italic;
            }
            span(s.text.replace('\t', "    "))
                .color(style::mute(Color::from_rgb8(
                    s.color[0], s.color[1], s.color[2],
                )))
                .font(font)
        })
        .collect();
    container(rich_text(pieces).size(style::T_13).font(style::mono()))
        .width(Length::Fixed(width))
        .into()
}

fn edit_panel<'a>(
    app: &'a App,
    doc: &'a CodeDoc,
    header: Element<'a, Message>,
) -> Element<'a, Message> {
    let dirty = app.edit_dirty;
    let state: fn(&Theme) -> text::Style = if dirty { style::accent } else { style::dim };

    let toolbar = row![
        text(if dirty { "Unsaved changes" } else { "Editing" })
            .size(style::T_12)
            .style(state),
        horizontal_space(),
        button(icon_manager::label(
            icon_manager::sparkles(),
            "Format",
            12,
            icon_manager::muted()
        ))
        .padding([6, 10])
        .on_press(Message::FormatCode)
        .style(style::ghost_button),
        button(icon_manager::label(
            icon_manager::undo_2(),
            "Revert",
            12,
            icon_manager::muted()
        ))
        .padding([6, 10])
        .on_press_maybe(dirty.then_some(Message::RevertEdit))
        .style(style::ghost_button),
        button(icon_manager::label(
            icon_manager::save(),
            "Save",
            13,
            icon_manager::on_primary()
        ))
        .padding([7, 14])
        .on_press_maybe(dirty.then_some(Message::SaveCode))
        .style(style::primary_button),
        button(text("Done").size(style::T_12))
            .padding([6, 10])
            .on_press(Message::ToggleEdit)
            .style(style::ghost_button),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let editor = text_editor(&app.edit_content)
        .on_action(Message::EditAction)
        .highlight(
            &syntax::token_for(&doc.rel),
            iced::highlighter::Theme::Base16Eighties,
        )
        .key_binding(|kp| {
            let tab = matches!(kp.key, keyboard::Key::Named(keyboard::key::Named::Tab));
            if tab && matches!(kp.status, Status::Focused) {
                Some(Binding::Sequence(vec![Binding::Insert(' '); 4]))
            } else {
                Binding::from_key_press(kp)
            }
        })
        .font(style::mono())
        .size(style::T_13)
        .padding(12)
        .height(Length::Fill)
        .style(style::code_editor);

    container(column![header, toolbar, editor].spacing(style::SP_3))
        .padding(style::SP_4)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
