mod agent;
mod app;
mod chats;
mod config;
mod memory;
mod ollama;
mod ui;

fn main() -> iced::Result {
    iced::application("Virgo AI", app::App::update, app::App::view)
        .subscription(app::App::subscription)
        .theme(app::App::theme)
        .antialiasing(true)
        .window_size(iced::Size::new(1180.0, 760.0))
        .run_with(app::App::new)
}