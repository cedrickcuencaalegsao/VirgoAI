//! Virgo AI look: flat and calm. No gradients, no shadows, no all caps.
//! bg #17191C  surface #1E2125  line #2C3136  text #DADFE3
//! dim #848C94  signal #E3A857  success #6FBF8F  danger #E5737B

use iced::font::{self, Weight};
use iced::theme::Palette;
use iced::widget::{button, container, pick_list, scrollable, text, text_editor, text_input};
use iced::{Background, Border, Color, Font, Shadow, Theme};

// ---- scales ---------------------------------------------------------------
pub const SP_1: f32 = 4.0;
pub const SP_2: f32 = 8.0;
pub const SP_3: f32 = 12.0;
pub const SP_4: f32 = 16.0;
pub const SP_6: f32 = 24.0;

/// Controls (buttons, fields, bubbles). Panes and code blocks use 0.
pub const R_CONTROL: f32 = 6.0;
/// Only for dots and status marks.
pub const R_ROUND: f32 = 999.0;

pub const READING_WIDTH: f32 = 680.0;

pub const T_11: f32 = 11.0;
pub const T_12: f32 = 12.0;
pub const T_13: f32 = 13.0;
pub const T_15: f32 = 15.0;
pub const T_20: f32 = 20.0;

// ---- colours (the only place they are defined) -------------------------------
pub fn bg() -> Color {
    Color::from_rgb8(0x17, 0x19, 0x1C)
}
pub fn surface() -> Color {
    Color::from_rgb8(0x1E, 0x21, 0x25)
}
pub fn line() -> Color {
    Color::from_rgb8(0x2C, 0x31, 0x36)
}
pub fn text_color() -> Color {
    Color::from_rgb8(0xDA, 0xDF, 0xE3)
}
pub fn dim_color() -> Color {
    Color::from_rgb8(0x84, 0x8C, 0x94)
}
pub fn signal() -> Color {
    Color::from_rgb8(0xE3, 0xA8, 0x57)
}
pub fn success_color() -> Color {
    Color::from_rgb8(0x6F, 0xBF, 0x8F)
}
pub fn danger_color() -> Color {
    Color::from_rgb8(0xE5, 0x73, 0x7B)
}
/// Text on a signal-coloured button.
pub fn on_signal() -> Color {
    bg()
}

pub fn alpha(color: Color, a: f32) -> Color {
    Color { a, ..color }
}

fn lighten(c: Color, t: f32) -> Color {
    Color {
        r: c.r + (1.0 - c.r) * t,
        g: c.g + (1.0 - c.g) * t,
        b: c.b + (1.0 - c.b) * t,
        a: c.a,
    }
}

/// Pull a colour toward grey (used to quiet syntax colours).
pub fn mute(c: Color) -> Color {
    let g = 0.30 * c.r + 0.59 * c.g + 0.11 * c.b;
    let k = 0.60;
    Color {
        r: g + (c.r - g) * k,
        g: g + (c.g - g) * k,
        b: g + (c.b - g) * k,
        a: c.a,
    }
}

pub fn palette() -> Palette {
    Palette {
        background: bg(),
        text: text_color(),
        primary: signal(),
        success: success_color(),
        danger: danger_color(),
    }
}

pub fn theme() -> Theme {
    Theme::custom("Virgo".to_string(), palette())
}

// ---- fonts -------------------------------------------------------------------
/// Needs Inter-SemiBold.ttf loaded in main.rs.
pub fn semibold() -> Font {
    Font {
        family: font::Family::Name("Inter"),
        weight: Weight::Semibold,
        ..Font::DEFAULT
    }
}
/// Kept so old call sites keep compiling: now semibold, not bold.
pub fn bold() -> Font {
    semibold()
}
/// Code, diffs, terminal only.
/// The screenshots show code in a proportional font (a 4-space indent is ~10px),
/// so "JetBrains Mono" is not being matched. System monospace always works;
/// switch back once the family name below is confirmed.
pub fn mono() -> Font {
    Font::MONOSPACE
}

// ---- text --------------------------------------------------------------------
pub fn dim(_t: &Theme) -> text::Style {
    text::Style {
        color: Some(dim_color()),
    }
}
pub fn success(_t: &Theme) -> text::Style {
    text::Style {
        color: Some(success_color()),
    }
}
pub fn danger_text(_t: &Theme) -> text::Style {
    text::Style {
        color: Some(danger_color()),
    }
}
pub fn accent(_t: &Theme) -> text::Style {
    text::Style {
        color: Some(signal()),
    }
}

// ---- containers -------------------------------------------------------------
fn fill(c: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(c)),
        ..container::Style::default()
    }
}

fn boxed(background: Option<Color>, border: Color, width: f32, radius: f32) -> container::Style {
    container::Style {
        background: background.map(Background::Color),
        border: Border {
            color: border,
            width,
            radius: radius.into(),
        },
        shadow: Shadow::default(),
        ..container::Style::default()
    }
}

pub fn clear(_t: &Theme) -> container::Style {
    container::Style::default()
}
pub fn sidebar(_t: &Theme) -> container::Style {
    fill(bg())
}
/// Flat surface pane: no border, no radius.
pub fn card(_t: &Theme) -> container::Style {
    fill(surface())
}
/// 1px separator. Give it a 1px width or height.
pub fn divider(_t: &Theme) -> container::Style {
    fill(line())
}
pub fn inset(_t: &Theme) -> container::Style {
    boxed(Some(bg()), Color::TRANSPARENT, 0.0, R_CONTROL)
}
pub fn user_bubble(_t: &Theme) -> container::Style {
    container::Style {
        text_color: Some(text_color()),
        ..boxed(Some(surface()), Color::TRANSPARENT, 0.0, R_CONTROL)
    }
}
pub fn error_card(_t: &Theme) -> container::Style {
    boxed(
        Some(alpha(danger_color(), 0.10)),
        Color::TRANSPARENT,
        0.0,
        R_CONTROL,
    )
}
pub fn thinking_box(_t: &Theme) -> container::Style {
    boxed(
        Some(alpha(Color::WHITE, 0.04)),
        Color::TRANSPARENT,
        0.0,
        R_CONTROL,
    )
}
/// Fenced code in replies: square, hairline.
pub fn code_block(_t: &Theme) -> container::Style {
    boxed(Some(bg()), line(), 1.0, 0.0)
}
pub fn code_area(_t: &Theme) -> container::Style {
    fill(bg())
}
pub fn row_added(_t: &Theme) -> container::Style {
    fill(alpha(success_color(), 0.14))
}
pub fn row_removed(_t: &Theme) -> container::Style {
    fill(alpha(danger_color(), 0.14))
}
/// Faint band behind the hovered line; stronger for the line with the suggestion bar open.
pub fn current_line(strong: bool) -> impl Fn(&Theme) -> container::Style {
    move |_t: &Theme| fill(alpha(Color::WHITE, if strong { 0.08 } else { 0.04 }))
}
/// 2px full-colour edge at the left of a changed row.
pub fn edge(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_t: &Theme| fill(color)
}
/// The chat composer: one 1px field.
pub fn composer(_t: &Theme) -> container::Style {
    boxed(Some(bg()), line(), 1.0, R_CONTROL)
}
pub fn indicator(active: bool) -> impl Fn(&Theme) -> container::Style {
    move |_t: &Theme| container::Style {
        background: active.then(|| Background::Color(signal())),
        ..container::Style::default()
    }
}
/// 2px underline under an active tab.
pub fn underline(active: bool) -> impl Fn(&Theme) -> container::Style {
    move |_t: &Theme| fill(if active { signal() } else { Color::TRANSPARENT })
}
/// One piece of the progress line under the top bar.
pub fn segment(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_t: &Theme| fill(color)
}
pub fn rail(_t: &Theme) -> container::Style {
    fill(surface())
}
pub fn status_bar(_t: &Theme) -> container::Style {
    fill(bg())
}
pub fn overlay_scrim(_t: &Theme) -> container::Style {
    fill(alpha(Color::BLACK, 0.55))
}
pub fn dot(color: Color) -> impl Fn(&Theme) -> container::Style {
    move |_t: &Theme| boxed(Some(color), Color::TRANSPARENT, 0.0, R_ROUND)
}
pub fn map_area(_t: &Theme) -> container::Style {
    boxed(Some(bg()), line(), 1.0, 0.0)
}
pub fn stat_pill(_t: &Theme) -> container::Style {
    boxed(
        Some(alpha(Color::WHITE, 0.04)),
        Color::TRANSPARENT,
        0.0,
        R_CONTROL,
    )
}

// ---- scrollbars -------------------------------------------------------------
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

// ---- buttons ----------------------------------------------------------------
fn flat(fill: Option<Color>, text: Color) -> button::Style {
    button::Style {
        background: fill.map(Background::Color),
        text_color: text,
        border: Border {
            radius: R_CONTROL.into(),
            ..Border::default()
        },
        shadow: Shadow::default(),
    }
}

fn hovered(s: button::Status) -> bool {
    matches!(s, button::Status::Hovered | button::Status::Pressed)
}

pub fn nav_item(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_t: &Theme, status: button::Status| match (active, hovered(status)) {
        (true, _) => flat(Some(alpha(Color::WHITE, 0.08)), text_color()),
        (false, true) => flat(Some(alpha(Color::WHITE, 0.06)), text_color()),
        _ => flat(None, alpha(text_color(), 0.78)),
    }
}

/// Flat list row (file tree): square, so the selection is a full-width band.
pub fn tree_item(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_t: &Theme, status: button::Status| {
        let mut s = match (active, hovered(status)) {
            (true, _) => flat(Some(alpha(Color::WHITE, 0.08)), text_color()),
            (false, true) => flat(Some(alpha(Color::WHITE, 0.05)), text_color()),
            _ => flat(None, alpha(text_color(), 0.78)),
        };
        s.border.radius = 0.0.into();
        s
    }
}

/// Solid signal fill. Disabled is an outline, never a dimmed fill.
pub fn primary_button(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Disabled => button::Style {
            background: None,
            text_color: dim_color(),
            border: Border {
                color: line(),
                width: 1.0,
                radius: R_CONTROL.into(),
            },
            shadow: Shadow::default(),
        },
        button::Status::Hovered | button::Status::Pressed => {
            flat(Some(lighten(signal(), 0.08)), on_signal())
        }
        button::Status::Active => flat(Some(signal()), on_signal()),
    }
}

/// Icon-only send button. The icon colour (signal / dim) is set by the caller.
pub fn send_button(_t: &Theme, status: button::Status) -> button::Style {
    let fill = hovered(status).then(|| alpha(Color::WHITE, 0.06));
    flat(fill, text_color())
}

/// Small square stop button: danger text on transparent.
pub fn stop_button(_t: &Theme, status: button::Status) -> button::Style {
    let fill = hovered(status).then(|| alpha(danger_color(), 0.12));
    flat(fill, danger_color())
}

pub fn danger_button(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Active => flat(Some(danger_color()), on_signal()),
        button::Status::Hovered | button::Status::Pressed => {
            flat(Some(lighten(danger_color(), 0.08)), on_signal())
        }
        button::Status::Disabled => {
            flat(Some(alpha(danger_color(), 0.35)), alpha(on_signal(), 0.6))
        }
    }
}

/// Quiet button: 6% white fill on hover, nothing else.
pub fn ghost_button(_t: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => {
            flat(Some(alpha(Color::WHITE, 0.06)), text_color())
        }
        button::Status::Disabled => flat(None, alpha(text_color(), 0.3)),
        button::Status::Active => flat(None, alpha(text_color(), 0.72)),
    }
}

/// Plain text link-style button (file lists, suggestions).
pub fn chip(_t: &Theme, status: button::Status) -> button::Style {
    flat(
        hovered(status).then(|| alpha(Color::WHITE, 0.06)),
        text_color(),
    )
}

/// Text tab. Pair it with `underline(active)` below it.
pub fn tab_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_t: &Theme, status: button::Status| {
        let color = if active || hovered(status) {
            text_color()
        } else {
            dim_color()
        };
        flat(None, color)
    }
}

pub fn note_card(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_t: &Theme, status: button::Status| match (active, hovered(status)) {
        (true, _) => flat(Some(alpha(Color::WHITE, 0.08)), text_color()),
        (false, true) => flat(Some(alpha(Color::WHITE, 0.05)), text_color()),
        _ => flat(None, text_color()),
    }
}

// ---- inputs -----------------------------------------------------------------
fn focus_border(focused: bool) -> Color {
    if focused {
        alpha(text_color(), 0.45)
    } else {
        line()
    }
}

pub fn composer_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border::default();
    style
}

pub fn composer_editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let mut style = text_editor::default(theme, status);
    style.background = Background::Color(Color::TRANSPARENT);
    style.border = Border::default();
    style
}

pub fn plan_editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let mut s = text_editor::default(theme, status);
    s.background = Background::Color(bg());
    s.border = Border {
        color: focus_border(matches!(status, text_editor::Status::Focused)),
        width: 1.0,
        radius: R_CONTROL.into(),
    };
    s.selection = alpha(signal(), 0.25);
    s
}

pub fn field(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut s = text_input::default(theme, status);
    s.background = Background::Color(bg());
    s.border = Border {
        color: focus_border(matches!(status, text_input::Status::Focused)),
        width: 1.0,
        radius: R_CONTROL.into(),
    };
    s.selection = alpha(signal(), 0.25);
    s
}

pub fn pick(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let mut s = pick_list::default(theme, status);
    s.background = Background::Color(bg());
    s.border = Border {
        color: line(),
        width: 1.0,
        radius: R_CONTROL.into(),
    };
    s
}

/// The code editor: square, hairline.
pub fn code_editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let mut s = text_editor::default(theme, status);
    s.background = Background::Color(bg());
    s.border = Border {
        color: focus_border(matches!(status, text_editor::Status::Focused)),
        width: 1.0,
        radius: 0.0.into(),
    };
    s.selection = alpha(signal(), 0.25);
    s
}

// ---- memory map -------------------------------------------------------------
pub fn group_color(i: usize) -> Color {
    const PALETTE: [(u8, u8, u8); 6] = [
        (0xE3, 0xA8, 0x57),
        (0x6F, 0xBF, 0x8F),
        (0x6C, 0xA8, 0xFF),
        (0xC0, 0x84, 0xFC),
        (0xE5, 0x73, 0x7B),
        (0x4F, 0xD1, 0xE0),
    ];
    let (r, g, b) = PALETTE[i % PALETTE.len()];
    Color::from_rgb8(r, g, b)
}
