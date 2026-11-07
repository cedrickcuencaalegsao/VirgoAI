use iced::alignment::Vertical;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

use crate::app::{App, Message};
use crate::config::Config;

fn labeled<'a>(label: &'a str, input: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let input: Element<'a, Message> = input.into();
    row![text(label).width(Length::Fixed(190.0)), input]
        .spacing(10)
        .align_y(Vertical::Center)
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let cfg = &app.cfg;

    let form = column![
        text("Settings").size(20),
        labeled(
            "Ollama URL",
            text_input("http://localhost:11434", &cfg.ollama_url)
                .on_input(Message::SettingsUrlChanged)
                .padding(8),
        ),
        labeled(
            "Memory folder",
            text_input("(empty = default folder)", &cfg.memory_dir)
                .on_input(Message::SettingsMemoryDirChanged)
                .padding(8),
        ),
        labeled(
            "Context size (num_ctx)",
            text_input("8192", &app.num_ctx_input)
                .on_input(Message::SettingsNumCtxChanged)
                .padding(8),
        ),
        labeled(
            "Keep models loaded",
            text_input("10m", &cfg.keep_alive)
                .on_input(Message::SettingsKeepAliveChanged)
                .padding(8),
        ),
        checkbox("Plan first, then execute (two-model pipeline)", cfg.use_planner)
            .on_toggle(Message::ToggleUsePlanner),
        checkbox(
            "Planner think mode (needs a thinking model such as qwen3)",
            cfg.planner_think
        )
        .on_toggle(Message::TogglePlannerThink),
        row![
            button("Save settings").on_press(Message::SaveSettings),
            button("Warm up models")
                .on_press(Message::WarmUp)
                .style(button::secondary),
        ]
        .spacing(8),
        text(format!("Config file: {}", Config::path().display())).size(12),
        text(format!("Memory folder: {}", app.store.dir().display())).size(12),
    ]
    .spacing(14)
    .max_width(720);

    container(scrollable(form))
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(container::bordered_box)
        .into()
}
