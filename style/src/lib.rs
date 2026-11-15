//! Virgo AI look and feel: deep navy surfaces, one gold accent.
//! Palette: #14213D navy, #FCA311 gold, #E5E5E5 text.
//!
//! Modern look = layered surfaces, soft shadows, large radii, a thin scrollbar,
//! a gradient primary button and round icon buttons. Colours are unchanged.

use iced::font::{self, Weight};
use iced::theme::Palette;
use iced::widget::{button, container, pick_list, scrollable, text, text_editor, text_input};
use iced::{Background, Border, Color, Font, Gradient, Shadow, Theme, gradient};

// ---------------------------------------------------------------------------
// Spacing and radius scale (use these instead of ad-hoc numbers)
// ---------------------------------------------------------------------------

pub const SP_1: f32 = 4.0;
pub const SP_2: f32 = 8.0;
pub const SP_3: f32 = 12.0;
pub const SP_4: f32 = 16.0;
pub const SP_6: f32 = 24.0;

pub const R_CONTROL: f32 = 8.0;
pub const R_PANEL: f32 = 12.0;

/// Width of the centred reading column in the chat.
pub const READING_WIDTH: f32 = 780.0;

// ---------------------------------------------------------------------------
// Colours (the only place colours are defined)
// ---------------------------------------------------------------------------

pub fn bg() -> Color {
    Color::from_rgb8(0x1E, 0x1E, 0x1E)
}
pub fn sidebar_bg() -> Color {
    bg()
}
pub fn surface() -> Color {
    Color::from_rgb8(0x2B, 0x2B, 0x2B)
}
pub fn surface_2() -> Color {
    Color::from_rgb8(0x32, 0x32, 0x32)
}
pub fn live() -> Color {
    Color::from_rgb8(0x9C, 0xC2, 0xFF)
}
pub fn gold() -> Color {
    live()
} // old call sites now use the accent
pub fn gold_hi() -> Color {
    Color::from_rgb8(0xBB, 0xD6, 0xFF)
}
pub fn on_gold() -> Color {
    bg()
}
fn line() -> Color {
    Color::from_rgb8(0x3A, 0x3A, 0x3A)
}

pub fn alpha(color: Color, a: f32) -> Color {
    Color { a, ..color }
}

pub fn palette() -> Palette {
    Palette {
        background: bg(),
        text: Color::from_rgb8(0xE5, 0xE5, 0xE5),
        primary: gold(),
        success: Color::from_rgb8(0x5F, 0xD3, 0xA4),
        danger: Color::from_rgb8(0xF0, 0x62, 0x6A),
    }
}

pub fn theme() -> Theme {
    Theme::custom("Virgo".to_string(), palette())
}

/// Needs `Inter-Bold.ttf` loaded in main.rs.
pub fn bold() -> Font {
    Font {
        family: font::Family::Name("Inter"),
        weight: Weight::Bold,
        ..Font::DEFAULT
    }
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Secondary, lower-contrast text.
pub fn dim(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(alpha(theme.palette().text, 0.60)),
    }
}

pub fn success(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().success),
    }
}

pub fn danger_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().danger),
    }
}

/// Brand colour text (gold).
pub fn accent(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().primary),
    }
}

// ---------------------------------------------------------------------------
// Containers
// ---------------------------------------------------------------------------

fn layer(background: Color, border: Color, radius: f32, shadow: Shadow) -> container::Style {
    container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            color: border,
            width: 1.0,
            radius: radius.into(),
        },
        shadow,
        ..container::Style::default()
    }
}

pub fn sidebar(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(sidebar_bg())),
        ..container::Style::default()
    }
}

/// Main panels: raised surface, hairline border, soft shadow.
pub fn card(_t: &Theme) -> container::Style {
    layer(surface(), line(), R_PANEL, Shadow::default())
}

/// A recessed box inside the sidebar or a panel.
pub fn inset(_theme: &Theme) -> container::Style {
    layer(
        alpha(Color::WHITE, 0.03),
        line(),
        R_CONTROL,
        Shadow::default(),
    )
}

pub fn user_bubble(_t: &Theme) -> container::Style {
    container::Style {
        text_color: Some(Color::from_rgb8(0xE0, 0xE0, 0xE0)),
        ..layer(surface_2(), line(), 14.0, Shadow::default())
    }
}

pub fn plan_card(_theme: &Theme) -> container::Style {
    layer(
        alpha(gold(), 0.06),
        alpha(gold(), 0.25),
        R_PANEL,
        Shadow::default(),
    )
}

pub fn error_card(theme: &Theme) -> container::Style {
    let d = theme.palette().danger;
    layer(alpha(d, 0.10), alpha(d, 0.45), R_CONTROL, Shadow::default())
}

/// The collapsible reasoning panel: quiet, secondary.
pub fn thinking_box(_theme: &Theme) -> container::Style {
    layer(
        alpha(Color::WHITE, 0.03),
        line(),
        R_CONTROL,
        Shadow::default(),
    )
}

/// Fenced code blocks inside replies.
pub fn code_block(_theme: &Theme) -> container::Style {
    layer(bg(), line(), R_CONTROL, Shadow::default())
}

/// The scrolling code area.
pub fn code_area(_theme: &Theme) -> container::Style {
    layer(bg(), line(), 14.0, Shadow::default())
}

pub fn row_added(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(alpha(theme.palette().success, 0.15))),
        ..container::Style::default()
    }
}

pub fn row_removed(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(alpha(theme.palette().danger, 0.14))),
        ..container::Style::default()
    }
}

pub fn row_selected(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(alpha(gold(), 0.13))),
        ..container::Style::default()
    }
}

/// The floating chat composer (2px gold border).
pub fn composer(_t: &Theme) -> container::Style {
    layer(surface(), line(), 14.0, Shadow::default())
}

/// Slim pill, e.g. for the status line.
// pub fn pill(_theme: &Theme) -> container::Style {
//     layer(alpha(Color::WHITE, 0.04), line(), 999.0, Shadow::default())
// }

/// 3px gold bar at the left of the active sidebar item.
pub fn indicator(active: bool) -> impl Fn(&Theme) -> container::Style {
    move |_theme: &Theme| container::Style {
        background: active.then(|| Background::Color(gold())),
        border: Border {
            radius: 2.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

// ---------------------------------------------------------------------------
// Scrollbars
// ---------------------------------------------------------------------------

/// Thin, translucent scrollbar with no track.
pub fn scroll(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.container = container::Style::default();
    style.gap = None;
    for rail in [&mut style.vertical_rail, &mut style.horizontal_rail] {
        rail.background = None;
        rail.border = Border::default();
        rail.scroller.color = alpha(Color::WHITE, 0.14);
        rail.scroller.border = Border {
            radius: 4.0.into(),
            ..Border::default()
        };
    }
    style
}

pub fn dim_color() -> Color {
    Color::from_rgb8(0x84, 0x84, 0x84)
}

pub fn rail(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg())),
        border: Border {
            color: line(),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}

pub fn status_bar(t: &Theme) -> container::Style {
    rail(t)
}

pub fn overlay_scrim(_t: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(alpha(Color::BLACK, 0.55))),
        ..container::Style::default()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StageState {
    Idle,
    Live,
    Done,
}

pub fn stage(state: StageState) -> impl Fn(&Theme) -> container::Style {
    move |_t: &Theme| {
        let (fill, outline) = match state {
            StageState::Live => (alpha(live(), 0.14), alpha(live(), 0.6)),
            StageState::Done => (Color::TRANSPARENT, line()),
            StageState::Idle => (Color::TRANSPARENT, Color::TRANSPARENT),
        };
        layer(fill, outline, 999.0, Shadow::default())
    }
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

/// Sidebar navigation and list items.
pub fn nav_item(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme: &Theme, status: button::Status| {
        let text_color = theme.palette().text;

        let (background, color) = match (active, status) {
            (true, _) => (Some(alpha(gold(), 0.12)), gold()),
            (false, button::Status::Hovered | button::Status::Pressed) => {
                (Some(alpha(Color::WHITE, 0.06)), text_color)
            }
            _ => (None, alpha(text_color, 0.78)),
        };

        button::Style {
            background: background.map(Background::Color),
            text_color: color,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: R_CONTROL.into(),
            },
            shadow: Shadow::default(),
        }
    }
}

/// Gold button with a subtle gradient (same two golds as before, blended).
pub fn primary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Disabled => Background::Color(alpha(gold(), 0.30)),
        button::Status::Hovered | button::Status::Pressed => {
            Background::Gradient(Gradient::Linear(
                gradient::Linear::new(std::f32::consts::FRAC_PI_2)
                    .add_stop(0.0, gold_hi())
                    .add_stop(1.0, gold_hi()),
            ))
        }
        button::Status::Active => Background::Gradient(Gradient::Linear(
            gradient::Linear::new(std::f32::consts::FRAC_PI_2)
                .add_stop(0.0, gold_hi())
                .add_stop(1.0, gold()),
        )),
    };
    let text_color = match status {
        button::Status::Disabled => alpha(Color::WHITE, 0.45),
        _ => on_gold(),
    };

    button::Style {
        background: Some(background),
        text_color,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: R_CONTROL.into(),
        },
        shadow: Shadow::default(),
    }
}

/// Round, icon-only send button.
pub fn send_button(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Disabled => alpha(gold(), 0.25),
        button::Status::Hovered | button::Status::Pressed => gold_hi(),
        button::Status::Active => gold(),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: on_gold(),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 999.0.into(),
        },
        shadow: Shadow::default(),
    }
}

/// Round, icon-only stop button.
pub fn stop_button(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = danger_button(theme, status);
    style.border.radius = 999.0.into();
    style
}

pub fn danger_button(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    let (background, text_color) = match status {
        button::Status::Active => (p.danger.base.color, p.danger.base.text),
        button::Status::Hovered | button::Status::Pressed => {
            (p.danger.strong.color, p.danger.strong.text)
        }
        button::Status::Disabled => (
            alpha(p.danger.base.color, 0.35),
            alpha(p.danger.base.text, 0.6),
        ),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: R_CONTROL.into(),
        },
        shadow: Shadow::default(),
    }
}

/// Quiet button for secondary actions.
pub fn ghost_button(theme: &Theme, status: button::Status) -> button::Style {
    let text_color = theme.palette().text;

    let (background, color) = match status {
        button::Status::Hovered | button::Status::Pressed => {
            (Some(alpha(Color::WHITE, 0.07)), text_color)
        }
        button::Status::Disabled => (None, alpha(text_color, 0.3)),
        button::Status::Active => (None, alpha(text_color, 0.72)),
    };

    button::Style {
        background: background.map(Background::Color),
        text_color: color,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 10.0.into(),
        },
        shadow: Shadow::default(),
    }
}

/// Small pill for attached files and suggestion chips.
pub fn chip(theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => alpha(gold(), 0.22),
        _ => alpha(gold(), 0.10),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: theme.palette().text,
        border: Border {
            color: alpha(gold(), 0.35),
            width: 1.0,
            radius: 999.0.into(),
        },
        shadow: Shadow::default(),
    }
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// The chat box: no border, the composer container draws it.
pub fn composer_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: 0.0.into(),
    };
    style
}

/// Borderless text area inside the composer.
pub fn composer_editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let mut style = text_editor::default(theme, status);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border {
        color: Color::TRANSPARENT,
        width: 0.0,
        radius: 0.0.into(),
    };
    style
}

/// Editable plan box: dark field, rounded, gold border when focused.
pub fn plan_editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let p = theme.palette();
    let border = match status {
        text_editor::Status::Focused => gold(),
        text_editor::Status::Hovered => alpha(gold(), 0.45),
        _ => alpha(Color::WHITE, 0.08),
    };
    text_editor::Style {
        background: Background::Color(bg()),
        border: Border {
            color: border,
            width: 1.5,
            radius: R_CONTROL.into(),
        },
        icon: alpha(p.text, 0.6),
        placeholder: alpha(p.text, 0.4),
        value: p.text,
        selection: alpha(gold(), 0.30),
    }
}

pub fn field(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.background = Background::Color(bg());
    let border = match status {
        text_input::Status::Focused => gold(),
        _ => alpha(Color::WHITE, 0.08),
    };
    style.border = Border {
        color: border,
        width: 1.0,
        radius: R_CONTROL.into(),
    };
    style
}

pub fn pick(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let mut style = pick_list::default(theme, status);
    style.background = Background::Color(bg());
    style.border = Border {
        color: alpha(Color::WHITE, 0.08),
        width: 1.0,
        radius: R_CONTROL.into(),
    };
    style
}

// ---------------------------------------------------------------------------
// Memory map
// ---------------------------------------------------------------------------

/// Colour of a note group (folder). Stable for a given group index.
pub fn group_color(i: usize) -> Color {
    const PALETTE: [(u8, u8, u8); 6] = [
        (0xFC, 0xA3, 0x11), // gold
        (0x5F, 0xD3, 0xA4), // mint
        (0x6C, 0xA8, 0xFF), // blue
        (0xC0, 0x84, 0xFC), // violet
        (0xF0, 0x62, 0x6A), // coral
        (0x4F, 0xD1, 0xE0), // cyan
    ];
    let (r, g, b) = PALETTE[i % PALETTE.len()];
    Color::from_rgb8(r, g, b)
}

/// Small round colour marker.
pub fn dot(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_theme: &Theme| container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: 999.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// The dark area the map is drawn on.
pub fn map_area(_theme: &Theme) -> container::Style {
    layer(bg(), line(), 16.0, Shadow::default())
}

/// Small number box (Notes / Links / Words).
pub fn stat_pill(_theme: &Theme) -> container::Style {
    layer(
        alpha(Color::WHITE, 0.04),
        line(),
        R_CONTROL,
        Shadow::default(),
    )
}

/// A note in the list: soft card, gold outline when selected.
pub fn note_card(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (fill, outline) = match (active, hovered) {
            (true, _) => (alpha(gold(), 0.12), alpha(gold(), 0.50)),
            (false, true) => (alpha(Color::WHITE, 0.07), alpha(Color::WHITE, 0.12)),
            (false, false) => (alpha(Color::WHITE, 0.03), line()),
        };
        button::Style {
            background: Some(Background::Color(fill)),
            text_color: theme.palette().text,
            border: Border {
                color: outline,
                width: 1.0,
                radius: R_CONTROL.into(),
            },
            shadow: Shadow::default(),
        }
    }
}

/// Filter pill above the note list.
pub fn filter_chip(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let (fill, text_color, outline) = if active {
            (gold(), on_gold(), Color::TRANSPARENT)
        } else if hovered {
            (alpha(Color::WHITE, 0.08), theme.palette().text, line())
        } else {
            (
                alpha(Color::WHITE, 0.04),
                alpha(theme.palette().text, 0.75),
                line(),
            )
        };
        button::Style {
            background: Some(Background::Color(fill)),
            text_color,
            border: Border {
                color: outline,
                width: 1.0,
                radius: 999.0.into(),
            },
            shadow: Shadow::default(),
        }
    }
}
