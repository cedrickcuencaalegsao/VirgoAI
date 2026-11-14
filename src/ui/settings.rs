use iced::alignment::Vertical;
use iced::widget::{button, checkbox, column, container, row, scrollable, text, text_input};
use iced::{Element, Length};

use super::style;
use crate::app::{App, Message};
use config::Config;

fn labeled<'a>(label: &'a str, input: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let input: Element<'a, Message> = input.into();
    row![text(label).size(14).width(Length::Fixed(200.0)), input]
        .spacing(10)
        .align_y(Vertical::Center)
        .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let cfg = &app.cfg;

    let form = column![
        column![
            text("Settings").size(22).font(style::bold()),
            text("Connection, models and memory")
                .size(12)
                .style(style::dim),
        ]
        .spacing(2),
        labeled(
            "Ollama URL",
            text_input("http://localhost:11434", &cfg.ollama_url)
                .on_input(Message::SettingsUrlChanged)
                .padding(9)
                .style(style::field),
        ),
                text("External AI providers").size(15).font(style::bold()),
        text(
            "Keys are saved in config.toml (empty = disabled). After saving, their models \
             appear in the Planner/Executor pickers as openai:, anthropic: and xai:."
        )
        .size(11)
        .style(style::dim),
        labeled(
            "OpenAI (ChatGPT) key",
            text_input("sk-...", &cfg.openai_key)
                .on_input(Message::SettingsOpenAiKeyChanged)
                .secure(true)
                .padding(9)
                .style(style::field),
        ),
        labeled(
            "Anthropic (Claude) key",
            text_input("sk-ant-...", &cfg.anthropic_key)
                .on_input(Message::SettingsAnthropicKeyChanged)
                .secure(true)
                .padding(9)
                .style(style::field),
        ),
        labeled(
            "xAI (Grok) key",
            text_input("xai-...", &cfg.xai_key)
                .on_input(Message::SettingsXaiKeyChanged)
                .secure(true)
                .padding(9)
                .style(style::field),
        ),
        labeled(
            "Memory folder",
            text_input("(empty = default folder)", &cfg.memory_dir)
                .on_input(Message::SettingsMemoryDirChanged)
                .padding(9)
                .style(style::field),
        ),
        labeled(
            "Context size (num_ctx)",
            text_input("8192", &app.num_ctx_input)
                .on_input(Message::SettingsNumCtxChanged)
                .padding(9)
                .style(style::field),
        ),
        labeled(
            "Keep models loaded",
            text_input("10m", &cfg.keep_alive)
                .on_input(Message::SettingsKeepAliveChanged)
                .padding(9)
                .style(style::field),
        ),
        checkbox(
            "Plan first, then execute (two-model pipeline)",
            cfg.use_planner
        )
        .on_toggle(Message::ToggleUsePlanner),
        checkbox(
            "Planner think mode (needs a thinking model such as qwen3)",
            cfg.planner_think
        )
        .on_toggle(Message::TogglePlannerThink),
        checkbox(
            "Executor think mode (only if the executor supports thinking)",
            cfg.executor_think
        )
        .on_toggle(Message::ToggleExecutorThink),
        checkbox(
            "Apply model edits to files automatically (you can always revert)",
            cfg.auto_apply
        )
        .on_toggle(Message::ToggleAutoApply),
        row![
            button(text("Save settings").size(14))
                .padding([9, 18])
                .on_press(Message::SaveSettings)
                .style(style::primary_button),
            button(text("Warm up models").size(14))
                .padding([9, 18])
                .on_press(Message::WarmUp)
                .style(style::ghost_button),
        ]
        .spacing(8),
        text(format!("Config file: {}", Config::path().display()))
            .size(12)
            .style(style::dim),
        text(format!("Memory folder: {}", app.store.dir().display()))
            .size(12)
            .style(style::dim),
    ]
    .spacing(16)
    .max_width(760);

    container(scrollable(form))
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(style::card)
        .into()
}
