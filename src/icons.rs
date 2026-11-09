//! Lucide icons, embedded at compile time from `assets/icons/*.svg`.
//!
//! Add more with `make add-icon <lucide-name>`: the Makefile downloads the SVG
//! and appends an `icon_fn!` line at the bottom of this file. Then call it as
//! `icons::<name_with_underscores>()`, for example `icons::folder_open()`.

#![allow(dead_code)]

use iced::alignment::Vertical;
use iced::widget::{row, svg, text, Svg};
use iced::{Color, Element, Length, Theme};

use crate::ui::style;

macro_rules! icon_fn {
    ($name:ident, $file:literal) => {
        pub fn $name<'a>() -> Svg<'a> {
            svg(svg::Handle::from_memory(
                &include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/icons/",
                    $file
                ))[..],
            ))
        }
    };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Size an icon and give it a colour (Lucide strokes use `currentColor`).
pub fn tint<'a>(icon: Svg<'a>, size: f32, color: Color) -> Svg<'a> {
    icon.width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_theme: &Theme, _status: svg::Status| svg::Style { color: Some(color) })
}

/// An icon followed by a text label, for buttons.
pub fn label<'a, Message: 'a>(
    icon: Svg<'a>,
    caption: impl text::IntoFragment<'a>,
    size: u16,
    color: Color,
) -> Element<'a, Message> {
    row![tint(icon, f32::from(size) + 3.0, color), text(caption).size(size)]
        .spacing(6)
        .align_y(Vertical::Center)
        .into()
}

pub fn muted() -> Color {
    Color { a: 0.7, ..style::palette().text }
}

pub fn accent() -> Color {
    style::palette().primary
}

/// Text colour on primary (violet) buttons.
pub fn on_primary() -> Color {
    style::theme().extended_palette().primary.base.text
}

/// Text colour on danger (red) buttons.
pub fn on_danger() -> Color {
    style::theme().extended_palette().danger.base.text
}

// ---------------------------------------------------------------------------
// Registered icons. `make add-icon` appends new lines below.
// ---------------------------------------------------------------------------

icon_fn!(folder_open, "folder-open.svg");
icon_fn!(folder, "folder.svg");
icon_fn!(file_text, "file-text.svg");
icon_fn!(chevron_right, "chevron-right.svg");
icon_fn!(chevron_down, "chevron-down.svg");
icon_fn!(refresh_cw, "refresh-cw.svg");
icon_fn!(x, "x.svg");
icon_fn!(panel_right, "panel-right.svg");
icon_fn!(check, "check.svg");
icon_fn!(plus, "plus.svg");
icon_fn!(message_square, "message-square.svg");
icon_fn!(database, "database.svg");
icon_fn!(sliders_horizontal, "sliders-horizontal.svg");
icon_fn!(copy, "copy.svg");
icon_fn!(send, "send.svg");
icon_fn!(square, "square.svg");
icon_fn!(trash_2, "trash-2.svg");
icon_fn!(paperclip, "paperclip.svg");
icon_fn!(eye, "eye.svg");
icon_fn!(lightbulb, "lightbulb.svg");
icon_fn!(undo_2, "undo-2.svg");icon_fn!(folder_kanban, "folder-kanban.svg");
