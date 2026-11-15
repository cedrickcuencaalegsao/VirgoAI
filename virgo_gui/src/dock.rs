use iced::alignment::Vertical;
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Element, Length};

use super::{project_panel, short};
use app::{App, Dock, Message, Screen};

fn tab(label: &'static str, active: bool, message: Message) -> Element<'static, Message> {
    button(text(label).size(12))
        .padding([5, 12])
        .on_press(message)
        .style(style::filter_chip(active))
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    // The Terminal tab opens the bottom pane, so it never renders here.
    let dock = if app.dock == Dock::Terminal {
        Dock::Files
    } else {
        app.dock
    };

    let tabs = row![
        tab("Files", dock == Dock::Files, Message::SetDock(Dock::Files)),
        tab(
            "Changes",
            dock == Dock::Changes,
            Message::SetDock(Dock::Changes)
        ),
        tab(
            "Memory",
            dock == Dock::Memory,
            Message::SetDock(Dock::Memory)
        ),
        tab("Terminal", app.show_terminal, Message::ToggleTerminal),
    ]
    .spacing(6)
    .align_y(Vertical::Center);

    let body: Element<'_, Message> = match dock {
        Dock::Changes => changes(app),
        Dock::Memory => memory(app),
        _ => project_panel::view(app),
    };

    column![tabs, body]
        .spacing(10)
        .width(Length::Fixed(300.0))
        .height(Length::Fill)
        .into()
}

fn changes(app: &App) -> Element<'_, Message> {
    let items: Vec<Element<'_, Message>> = app
        .changes
        .iter()
        .rev()
        .map(|set| {
            let names: Vec<&str> = set.files.iter().map(|f| f.rel.as_str()).collect();
            let label = format!(
                "{} file(s): {}",
                set.files.len(),
                short(&names.join(", "), 34)
            );

            let action: Element<'_, Message> = if set.reverted {
                text("Reverted").size(12).style(style::dim).into()
            } else {
                button(icon_manager::label(
                    icon_manager::undo_2(),
                    "Revert",
                    12,
                    icon_manager::muted(),
                ))
                .padding([5, 10])
                .on_press(Message::RevertChange(set.id))
                .style(style::ghost_button)
                .into()
            };

            container(column![text(label).size(13), action].spacing(6))
                .padding(10)
                .width(Length::Fill)
                .style(style::inset)
                .into()
        })
        .collect();

    let content: Element<'_, Message> = if items.is_empty() {
        text("No AI changes yet. Edits the model writes to your files appear here, each with a Revert button.")
            .size(12)
            .style(style::dim)
            .into()
    } else {
        scrollable(column(items).spacing(8))
            .height(Length::Fill)
            .into()
    };

    container(column![text("Changes").size(18).font(style::bold()), content].spacing(12))
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style::card)
        .into()
}

fn memory(app: &App) -> Element<'_, Message> {
    let items: Vec<Element<'_, Message>> = app
        .notes
        .iter()
        .map(|name| {
            button(text(short(name.trim_end_matches(".md"), 32)).size(13))
                .width(Length::Fill)
                .padding([6, 10])
                .on_press(Message::Navigate(Screen::Memory))
                .style(style::nav_item(false))
                .into()
        })
        .collect();

    let content: Element<'_, Message> = if items.is_empty() {
        text("No notes yet. Open Memory to add one.")
            .size(12)
            .style(style::dim)
            .into()
    } else {
        scrollable(column(items).spacing(2))
            .height(Length::Fill)
            .into()
    };

    container(
        column![
            text("Memory").size(18).font(style::bold()),
            text("profile.md is sent with every message. Other notes are picked by keyword.")
                .size(11)
                .style(style::dim),
            content,
        ]
        .spacing(10),
    )
    .padding(16)
    .width(Length::Fill)
    .height(Length::Fill)
    .style(style::card)
    .into()
}
