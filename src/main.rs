mod agent;
mod app;
mod chats;
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
        .window_size(iced::Size::new(1280.0, 780.0))
        .run_with(app::App::new)
}