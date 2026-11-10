//! Virgo AI look and feel: deep navy surfaces, one gold accent.
//! Palette: #14213D navy, #FCA311 gold, #E5E5E5 text.

use iced::font::Weight;
use iced::theme::Palette;
use iced::widget::{button, container, pick_list, text, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme};

// ---------------------------------------------------------------------------
// Colours
// ---------------------------------------------------------------------------

pub fn bg() -> Color {
    Color::from_rgb8(0x0B, 0x13, 0x24)
}
pub fn sidebar_bg() -> Color {
    Color::from_rgb8(0x0F, 0x1A, 0x33)
}
pub fn surface() -> Color {
    Color::from_rgb8(0x14, 0x21, 0x3D)
}
pub fn gold() -> Color {
    Color::from_rgb8(0xFC, 0xA3, 0x11)
}
pub fn gold_hi() -> Color {
    Color::from_rgb8(0xFF, 0xB8, 0x3A)
}
/// Text colour on gold surfaces.
pub fn on_gold() -> Color {
    bg()
}

fn alpha(color: Color, a: f32) -> Color {
    Color { a, ..color }
}

fn line() -> Color {
    alpha(Color::WHITE, 0.07)
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

pub fn bold() -> Font {
    Font { weight: Weight::Bold, ..Font::DEFAULT }
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Secondary, lower-contrast text.
pub fn dim(theme: &Theme) -> text::Style {
    text::Style { color: Some(alpha(theme.palette().text, 0.62)) }
}

pub fn success(theme: &Theme) -> text::Style {
    text::Style { color: Some(theme.palette().success) }
}

pub fn danger_text(theme: &Theme) -> text::Style {
    text::Style { color: Some(theme.palette().danger) }
}

/// Brand colour text (gold).
pub fn accent(theme: &Theme) -> text::Style {
    text::Style { color: Some(theme.palette().primary) }
}

// ---------------------------------------------------------------------------
// Containers
// ---------------------------------------------------------------------------

fn surface_style(bg: Color, border: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border { color: border, width: 1.0, radius: radius.into() },
        ..container::Style::default()
    }
}

pub fn sidebar(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(sidebar_bg())),
        ..container::Style::default()
    }
}

pub fn card(_theme: &Theme) -> container::Style {
    surface_style(surface(), line(), 16.0)
}

pub fn user_bubble(_theme: &Theme) -> container::Style {
    container::Style {
        text_color: Some(on_gold()),
        ..surface_style(gold(), Color::TRANSPARENT, 16.0)
    }
}

pub fn plan_card(_theme: &Theme) -> container::Style {
    surface_style(alpha(gold(), 0.06), alpha(gold(), 0.30), 16.0)
}

pub fn error_card(theme: &Theme) -> container::Style {
    let d = theme.palette().danger;
    surface_style(alpha(d, 0.10), alpha(d, 0.50), 12.0)
}

/// The collapsible reasoning panel.
pub fn thinking_box(_theme: &Theme) -> container::Style {
    surface_style(bg(), line(), 10.0)
}

/// Fenced code blocks inside replies.
pub fn code_block(_theme: &Theme) -> container::Style {
    surface_style(bg(), line(), 10.0)
}

/// The scrolling code area.
pub fn code_area(_theme: &Theme) -> container::Style {
    surface_style(bg(), line(), 12.0)
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

pub fn composer(_theme: &Theme) -> container::Style {
    surface_style(surface(), alpha(gold(), 0.40), 18.0)
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

/// Sidebar navigation and list items.
pub fn nav_item(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme: &Theme, status: button::Status| {
        let text_color = theme.palette().text;

        let (background, color) = match (active, status) {
            (true, _) => (Some(alpha(gold(), 0.14)), gold()),
            (false, button::Status::Hovered | button::Status::Pressed) => {
                (Some(alpha(Color::WHITE, 0.06)), text_color)
            }
            _ => (None, alpha(text_color, 0.78)),
        };

        button::Style {
            background: background.map(Background::Color),
            text_color: color,
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 10.0.into() },
            shadow: Shadow::default(),
        }
    }
}

pub fn primary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Active => (gold(), on_gold()),
        button::Status::Hovered | button::Status::Pressed => (gold_hi(), on_gold()),
        button::Status::Disabled => (alpha(gold(), 0.30), alpha(Color::WHITE, 0.45)),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color,
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 12.0.into() },
        shadow: Shadow::default(),
    }
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
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 12.0.into() },
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
        border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 8.0.into() },
        shadow: Shadow::default(),
    }
}

/// Small pill for attached files above the composer.
pub fn chip(theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => alpha(gold(), 0.24),
        _ => alpha(gold(), 0.12),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: theme.palette().text,
        border: Border { color: alpha(gold(), 0.40), width: 1.0, radius: 12.0.into() },
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
    style.border = Border { color: Color::TRANSPARENT, width: 0.0, radius: 0.0.into() };
    style
}

pub fn field(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.background = Background::Color(bg());
    let border = match status {
        text_input::Status::Focused => gold(),
        _ => alpha(Color::WHITE, 0.10),
    };
    style.border = Border { color: border, width: 1.0, radius: 10.0.into() };
    style
}

pub fn pick(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let mut style = pick_list::default(theme, status);
    style.background = Background::Color(bg());
    style.border = Border {
        color: alpha(Color::WHITE, 0.10),
        width: 1.0,
        radius: 10.0.into(),
    };
    style
}