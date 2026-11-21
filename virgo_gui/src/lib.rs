pub mod actions;
pub mod chat;
pub mod code_panel;
pub mod dock;
pub mod markdown;
pub mod memory;
pub mod nav;
pub mod palette;
pub mod plan_panel;
pub mod project_panel;
pub mod rail;
pub mod settings;
pub mod sidebar;
pub mod status_bar;
pub mod terminal_panel;
pub mod top_bar;
pub mod workspaces;

/// Shorten text so it fits on one line, adding `...` when cut.
pub fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max).collect();
        format!("{head}...")
    }
}

use iced::widget::{Space, button, column, container, text};
use iced::{Element, Length};

use app::Message;

pub fn vdivider<'a, M: 'a>() -> Element<'a, M> {
    container(Space::new(Length::Fixed(1.0), Length::Fill))
        .width(Length::Fixed(1.0))
        .height(Length::Fill)
        .style(style::divider)
        .into()
}

pub fn hdivider<'a, M: 'a>() -> Element<'a, M> {
    container(Space::new(Length::Fill, Length::Fixed(1.0)))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(style::divider)
        .into()
}

/// Plain text tab; the active one has a 2px signal underline.
pub fn tab<'a>(label: &'a str, active: bool, message: Message) -> Element<'a, Message> {
    column![
        button(text(label).size(style::T_12))
            .padding([5, 10])
            .on_press(message)
            .style(style::tab_button(active)),
        container(Space::new(Length::Fill, Length::Fixed(2.0)))
            .width(Length::Fill)
            .style(style::underline(active)),
    ]
    .into()
}
