//! Virgo AI look and feel: one custom dark theme plus small style helpers.

use iced::font::Weight;
use iced::theme::Palette;
use iced::widget::{button, container, pick_list, text, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme};

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

pub fn palette() -> Palette {
    Palette {
        background: Color::from_rgb8(0x0E, 0x10, 0x16),
        text: Color::from_rgb8(0xE8, 0xEA, 0xF0),
        primary: Color::from_rgb8(0x8B, 0x7C, 0xFF), // soft violet
        success: Color::from_rgb8(0x4A, 0xDE, 0x9B),
        danger: Color::from_rgb8(0xFF, 0x6B, 0x6B),
    }
}

pub fn theme() -> Theme {
    Theme::custom("Virgo".to_string(), palette())
}

fn alpha(color: Color, a: f32) -> Color {
    Color { a, ..color }
}

pub fn bold() -> Font {
    Font { weight: Weight::Bold, ..Font::DEFAULT }
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Secondary, lower-contrast text.
pub fn dim(theme: &Theme) -> text::Style {
    text::Style { color: Some(alpha(theme.palette().text, 0.6)) }
}

/// Brand colour text.
pub fn accent(theme: &Theme) -> text::Style {
    text::Style { color: Some(theme.palette().primary) }
}

// ---------------------------------------------------------------------------
// Containers
// ---------------------------------------------------------------------------

fn surface(bg: Color, border: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border { color: border, width: 1.0, radius: radius.into() },
        ..container::Style::default()
    }
}

pub fn sidebar(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(Background::Color(p.background.weak.color)),
        ..container::Style::default()
    }
}

pub fn card(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    surface(
        p.background.weak.color,
        alpha(p.background.strong.color, 0.55),
        16.0,
    )
}

pub fn user_bubble(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        text_color: Some(p.primary.base.text),
        ..surface(p.primary.base.color, Color::TRANSPARENT, 16.0)
    }
}

pub fn plan_card(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    surface(
        alpha(p.primary.base.color, 0.07),
        alpha(p.primary.base.color, 0.35),
        16.0,
    )
}

pub fn error_card(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    surface(
        alpha(p.danger.base.color, 0.10),
        alpha(p.danger.base.color, 0.50),
        12.0,
    )
}

/// The collapsible reasoning panel.
pub fn thinking_box(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    surface(
        p.background.base.color,
        alpha(p.background.strong.color, 0.4),
        10.0,
    )
}

/// Fenced code blocks inside replies.
pub fn code_block(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    surface(
        p.background.base.color,
        alpha(p.background.strong.color, 0.5),
        10.0,
    )
}

pub fn composer(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    surface(
        p.background.weak.color,
        alpha(p.primary.base.color, 0.45),
        18.0,
    )
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

/// Sidebar navigation and list items.
pub fn nav_item(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme: &Theme, status: button::Status| {
        let p = theme.extended_palette();
        let text_color = theme.palette().text;
        let primary = p.primary.base.color;

        let (background, color) = match (active, status) {
            (true, _) => (Some(alpha(primary, 0.18)), primary),
            (false, button::Status::Hovered | button::Status::Pressed) => {
                (Some(alpha(p.background.strong.color, 0.5)), text_color)
            }
            _ => (None, alpha(text_color, 0.75)),
        };

        button::Style {
            background: background.map(Background::Color),
            text_color: color,
            border: Border { color: Color::TRANSPARENT, width: 0.0, radius: 10.0.into() },
            shadow: Shadow::default(),
        }
    }
}

pub fn primary_button(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    let (background, text_color) = match status {
        button::Status::Active => (p.primary.base.color, p.primary.base.text),
        button::Status::Hovered | button::Status::Pressed => {
            (p.primary.strong.color, p.primary.strong.text)
        }
        button::Status::Disabled => (
            alpha(p.primary.base.color, 0.35),
            alpha(p.primary.base.text, 0.6),
        ),
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
    let p = theme.extended_palette();
    let text_color = theme.palette().text;

    let (background, color) = match status {
        button::Status::Hovered | button::Status::Pressed => {
            (Some(alpha(p.background.strong.color, 0.5)), text_color)
        }
        button::Status::Disabled => (None, alpha(text_color, 0.3)),
        button::Status::Active => (None, alpha(text_color, 0.7)),
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
    let p = theme.extended_palette();
    let primary = p.primary.base.color;
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => alpha(primary, 0.28),
        _ => alpha(primary, 0.15),
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: theme.palette().text,
        border: Border { color: alpha(primary, 0.4), width: 1.0, radius: 12.0.into() },
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
    style.border.radius = 10.0.into();
    style
}

pub fn pick(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let mut style = pick_list::default(theme, status);
    style.border.radius = 10.0.into();
    style
}