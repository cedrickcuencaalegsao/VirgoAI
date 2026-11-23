//! Code panel, laid out like an editor: tab strip, breadcrumb row, a tight
//! gutter, current-line highlight and a status line.
//!
//! Rows are exactly ROW_H tall, so windowing and scroll maths stay exact.
//! ROW_H must match `ROW_H` in `app/src/live.rs`.

use iced::alignment::{Horizontal, Vertical};
use iced::font::{Style as FontStyle, Weight};
use iced::widget::text_editor::{Binding, Status};
use iced::widget::{
    Row, Space, Svg, button, column, container, horizontal_space, mouse_area, rich_text, row,
    scrollable, span, text, text_editor, text_input,
};
use iced::{Border, Color, Element, Length, Theme, keyboard};

use super::short;
use app::{App, Message};
use code::{self, CodeDoc, RowKind, RowRef};

const ROW_H: f32 = 22.0;
const OVERSCAN: usize = 12;
const TAB_H: f32 = 36.0;
/// Left margin that holds the bulb, or accept / reject on a changed row.
const SLOT_W: f32 = 40.0;
const NUM_W: f32 = 36.0;
const MARKER_W: f32 = 14.0;

pub fn view(app: &App) -> Element<'_, Message> {
    match &app.code {
        Some(doc) => panel(app, doc),
        None => column![].into(),
    }
}

fn icon_button<'a>(icon: Svg<'a>, message: Message) -> Element<'a, Message> {
    button(icon_manager::tint(icon, 15.0, icon_manager::muted()))
        .padding(6)
        .on_press(message)
        .style(style::ghost_button)
        .into()
}

fn bright(_t: &Theme) -> text::Style {
    text::Style {
        color: Some(style::text_color()),
    }
}

// ---------------------------------------------------------------------------
// Chrome: tab strip, breadcrumb, status line
// ---------------------------------------------------------------------------

/// One open file as a tab. The active tab shares the editor background and has a
/// 1px signal line on top, the strip behind it is the darker surface.
fn tab_strip<'a>(app: &'a App, name: &'a str) -> Element<'a, Message> {
    let dirty = app.is_editing() && app.edit_dirty;
    let close: Element<'a, Message> = if dirty {
        button(text("\u{25CF}").size(style::T_12).style(style::accent))
            .padding([4, 6])
            .on_press(Message::CloseCode)
            .style(style::ghost_button)
            .into()
    } else {
        button(icon_manager::tint(
            icon_manager::x(),
            13.0,
            icon_manager::muted(),
        ))
        .padding([4, 5])
        .on_press(Message::CloseCode)
        .style(style::ghost_button)
        .into()
    };

    let tab = column![
        container(Space::new(Length::Fill, Length::Fixed(1.0)))
            .width(Length::Fill)
            .style(style::underline(true)),
        container(
            row![
                icon_manager::tint(icon_manager::file_text(), 14.0, icon_manager::accent()),
                text(name).size(style::T_13),
                close,
            ]
            .spacing(8)
            .align_y(Vertical::Center),
        )
        .padding([0, 8])
        .height(Length::Fixed(TAB_H - 1.0))
        .align_y(Vertical::Center)
        .style(style::code_area),
    ];

    container(row![tab, horizontal_space()])
        .width(Length::Fill)
        .style(style::card)
        .into()
}

/// `src > main.rs` on the left, `right` (controls) on the right.
fn breadcrumb<'a>(doc: &'a CodeDoc, right: Row<'a, Message>) -> Element<'a, Message> {
    let mut crumbs = row![].spacing(4).align_y(Vertical::Center);
    let parts: Vec<&str> = doc.rel.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        crumbs = crumbs.push(if last {
            text(*part).size(style::T_12).style(bright)
        } else {
            text(*part).size(style::T_12).style(style::dim)
        });
        if !last {
            crumbs = crumbs.push(icon_manager::tint(
                icon_manager::chevron_right(),
                11.0,
                icon_manager::muted(),
            ));
        }
    }

    container(
        row![crumbs, horizontal_space(), right.align_y(Vertical::Center)]
            .spacing(style::SP_2)
            .align_y(Vertical::Center),
    )
    .padding([4, 12])
    .width(Length::Fill)
    .into()
}

/// Bottom line: cursor-ish position, language, and a one-line hint.
fn status_line<'a>(app: &'a App, doc: &'a CodeDoc) -> Element<'a, Message> {
    let position = match app.hover_line.or(app.suggest_line) {
        Some(l) => format!("Ln {}", l + 1),
        None => format!("{} lines", doc.lines.len()),
    };
    let lang = if doc.lang().is_empty() {
        "Plain text"
    } else {
        doc.lang()
    };

    container(
        row![
            text(position).size(style::T_11).style(style::dim),
            text(lang).size(style::T_11).style(style::dim),
            horizontal_space(),
            text("Click the bulb on a line to edit it with AI")
                .size(style::T_11)
                .style(style::dim),
        ]
        .spacing(style::SP_4)
        .align_y(Vertical::Center),
    )
    .padding([4, 12])
    .width(Length::Fill)
    .style(style::card)
    .into()
}

// ---------------------------------------------------------------------------
// Read view
// ---------------------------------------------------------------------------

fn panel<'a>(app: &'a App, doc: &'a CodeDoc) -> Element<'a, Message> {
    let name = doc.rel.rsplit('/').next().unwrap_or(doc.rel.as_str());
    let strip = tab_strip(app, name);

    if app.is_editing() {
        return edit_panel(app, doc, strip);
    }

    // ---- changes: only shown when there is something to decide ----
    let pending = doc.change_count();
    let live = doc.is_live();

    let mut controls = row![].spacing(style::SP_1);
    if live {
        controls = controls.push(
            text("AI is editing this file")
                .size(style::T_12)
                .style(style::dim),
        );
        if app.is_working() {
            controls = controls.push(
                button(text("Stop").size(style::T_12))
                    .padding([4, 10])
                    .on_press(Message::Stop)
                    .style(style::ghost_button),
            );
        }
    } else if pending > 0 {
        controls = controls
            .push(
                text(format!(
                    "{pending} change{}",
                    if pending == 1 { "" } else { "s" }
                ))
                .size(style::T_12),
            )
            .push(
                button(text("Reject all").size(style::T_12))
                    .padding([4, 10])
                    .on_press(Message::DiscardEdits)
                    .style(style::ghost_button),
            )
            .push(
                button(text("Accept").size(style::T_12))
                    .padding([4, 12])
                    .on_press(Message::ApplyEdits)
                    .style(style::primary_button),
            );
    }
    if doc.can_undo() {
        controls = controls.push(
            button(icon_manager::label(
                icon_manager::undo_2(),
                "Undo",
                12,
                icon_manager::muted(),
            ))
            .padding([4, 10])
            .on_press(Message::UndoApply)
            .style(style::ghost_button),
        );
    }
    controls = controls
        .push(icon_button(icon_manager::pencil(), Message::ToggleEdit))
        .push(icon_button(icon_manager::refresh_cw(), Message::ReloadCode));

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

    let mut page = column![strip, breadcrumb(doc, controls), area];
    if let Some(bar) = suggest_bar(app, doc) {
        page = page.push(bar);
    }
    page = page.push(status_line(app, doc));

    container(page)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn code_row<'a>(app: &'a App, r: &RowRef<'_>, width: f32) -> Element<'a, Message> {
    let kind = r.kind;
    let selected = app.suggest_line == Some(r.line) && kind != RowKind::Added;
    let hovered = app.hover_line == Some(r.line) && kind != RowKind::Added;
    let current = selected || hovered;
    let busy = app.suggesting.is_some();

    // Line number: dim, bright on the current line.
    let number_style: fn(&Theme) -> text::Style = if current { bright } else { style::dim };
    let gutter = text(r.number.map(|n| n.to_string()).unwrap_or_default())
        .size(style::T_12)
        .font(style::mono())
        .style(number_style)
        .width(Length::Fixed(NUM_W))
        .align_x(Horizontal::Right);

    let marker = match kind {
        RowKind::Added => text("+").style(style::success),
        RowKind::Removed => text("-").style(style::danger_text),
        RowKind::Normal => text(" "),
    }
    .size(style::T_13)
    .font(style::mono())
    .width(Length::Fixed(MARKER_W))
    .align_x(Horizontal::Right);

    // Left margin keeps its width, so nothing shifts when the bulb appears.
    let slot: Element<'a, Message> = if let (Some(h), RowKind::Removed) = (r.hunk, kind) {
        if h == code::LIVE_ID {
            container(text("...").size(style::T_12).style(style::dim))
                .width(Length::Fixed(SLOT_W))
                .align_x(Horizontal::Right)
                .into()
        } else {
            container(row![
                button(icon_manager::tint(
                    icon_manager::check(),
                    12.0,
                    style::success_color()
                ))
                .padding([5, 3])
                .on_press(Message::AcceptHunk(h))
                .style(style::ghost_button),
                button(icon_manager::tint(
                    icon_manager::x(),
                    12.0,
                    icon_manager::muted()
                ))
                .padding([5, 3])
                .on_press(Message::RejectHunk(h))
                .style(style::ghost_button),
            ])
            .width(Length::Fixed(SLOT_W))
            .align_x(Horizontal::Right)
            .into()
        }
    } else if r.show_actions {
        let mut buttons: Row<'a, Message> = Row::new();
        if current {
            let color = if selected {
                style::text_color()
            } else {
                icon_manager::muted()
            };
            buttons = buttons.push(
                button(icon_manager::tint(icon_manager::lightbulb(), 12.0, color))
                    .padding([5, 3])
                    .on_press_maybe((!busy).then_some(Message::SuggestLine(r.line)))
                    .style(style::ghost_button),
            );
        }
        if kind == RowKind::Removed {
            buttons = buttons.push(
                button(icon_manager::tint(
                    icon_manager::undo_2(),
                    12.0,
                    icon_manager::muted(),
                ))
                .padding([5, 3])
                .on_press(Message::RevertLine(r.line))
                .style(style::ghost_button),
            );
        }
        container(buttons)
            .width(Length::Fixed(SLOT_W))
            .align_x(Horizontal::Right)
            .into()
    } else {
        Space::with_width(Length::Fixed(SLOT_W)).into()
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

    let content = row![edge, slot, gutter, marker, code]
        .height(Length::Fill)
        .align_y(Vertical::Center);

    // The current line is a faint full-width band, like an editor.
    let body = container(content)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_H))
        .style(move |theme: &Theme| match kind {
            RowKind::Added => style::row_added(theme),
            RowKind::Removed => style::row_removed(theme),
            RowKind::Normal if current => style::current_line(selected)(theme),
            RowKind::Normal => container::Style::default(),
        });

    mouse_area(body)
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

// ---------------------------------------------------------------------------
// Edit view
// ---------------------------------------------------------------------------

fn edit_panel<'a>(
    app: &'a App,
    doc: &'a CodeDoc,
    strip: Element<'a, Message>,
) -> Element<'a, Message> {
    let dirty = app.edit_dirty;
    let state: fn(&Theme) -> text::Style = if dirty { style::accent } else { style::dim };

    let controls = row![
        text(if dirty { "Unsaved changes" } else { "Editing" })
            .size(style::T_12)
            .style(state),
        button(icon_manager::label(
            icon_manager::sparkles(),
            "Format",
            12,
            icon_manager::muted()
        ))
        .padding([4, 10])
        .on_press(Message::FormatCode)
        .style(style::ghost_button),
        button(icon_manager::label(
            icon_manager::undo_2(),
            "Revert",
            12,
            icon_manager::muted()
        ))
        .padding([4, 10])
        .on_press_maybe(dirty.then_some(Message::RevertEdit))
        .style(style::ghost_button),
        button(icon_manager::label(
            icon_manager::save(),
            "Save",
            12,
            icon_manager::on_primary()
        ))
        .padding([4, 12])
        .on_press_maybe(dirty.then_some(Message::SaveCode))
        .style(style::primary_button),
        button(text("Done").size(style::T_12))
            .padding([4, 10])
            .on_press(Message::ToggleEdit)
            .style(style::ghost_button),
    ]
    .spacing(style::SP_1)
    .align_y(Vertical::Center);

    // Flat editor: the pane itself is the surface, no inner box.
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
        .padding([8, 16])
        .height(Length::Fill)
        .style(|theme: &Theme, status: Status| {
            let mut s = style::code_editor(theme, status);
            s.border = Border::default();
            s
        });

    container(column![
        strip,
        breadcrumb(doc, controls),
        editor,
        status_line(app, doc),
    ])
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}