mod agent;
mod app;
mod config;
mod memory;
mod ollama;
mod ui;

fn main() -> iced::Result {
    iced::application("Virgo AI", app::App::update, app::App::view)
        .theme(app::App::theme)
        .window_size(iced::Size::new(1100.0, 720.0))
        .run_with(app::App::new)
}
