mod agent;
mod app;
mod chats;
mod code;
mod config;
mod icons;
mod memory;
mod ollama;
mod project;
mod ui;

fn main() -> iced::Result {
    iced::application("Virgo AI", app::App::update, app::App::view)
        .subscription(app::App::subscription)
        .theme(app::App::theme)
        .antialiasing(true)
        .window_size(iced::Size::new(1480.0, 820.0))
        .run_with(app::App::new)
}