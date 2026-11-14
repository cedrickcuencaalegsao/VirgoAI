mod app;
mod icons;
mod ui;

fn main() -> iced::Result {
    iced::application("Virgo AI", app::App::update, app::App::view)
        .subscription(app::App::subscription)
        .theme(app::App::theme)
        // Put Inter-Regular.ttf and Inter-Bold.ttf in assets/fonts/
        .font(&include_bytes!("../assets/fonts/Inter-Regular.ttf")[..])
        .font(&include_bytes!("../assets/fonts/Inter-Bold.ttf")[..])
        .default_font(iced::Font::with_name("Inter"))
        .antialiasing(true)
        .window_size(iced::Size::new(1480.0, 820.0))
        .run_with(app::App::new)
}