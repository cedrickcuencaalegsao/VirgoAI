use iced::widget::{column, container, row, stack};
use iced::{Element, Length};

use app::{App, Message};
use virgo_gui::{hdivider, vdivider};

fn main() -> iced::Result {
    iced::application("Virgo AI", App::update, view)
        .subscription(App::subscription)
        .theme(App::theme)
        .font(&include_bytes!("../assets/fonts/Inter-Regular.ttf")[..])
        .font(&include_bytes!("../assets/fonts/Inter-SemiBold.ttf")[..])
        .font(&include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf")[..])
        .default_font(iced::Font::with_name("Inter"))
        .antialiasing(true)
        .window_size(iced::Size::new(1480.0, 820.0))
        .run_with(App::new)
}

fn view(app: &App) -> Element<'_, Message> {
    let page: Element<'_, Message> = match app.screen {
        app::Screen::Chat => {
            let chat_width = if app.code.is_some() {
                Length::FillPortion(2)
            } else {
                Length::Fill
            };

            let mut panels = row![container(virgo_gui::chat::view(app))
                .width(chat_width)
                .height(Length::Fill)]
            .height(Length::Fill);

            if app.code.is_some() {
                panels = panels.push(vdivider()).push(
                    container(virgo_gui::code_panel::view(app))
                        .width(Length::FillPortion(3))
                        .height(Length::Fill),
                );
            }
            if app.show_panel {
                panels = panels.push(vdivider()).push(virgo_gui::dock::view(app));
            }
            let panels: Element<'_, Message> = panels.into();

            if app.show_terminal {
                column![
                    container(panels).height(Length::Fill),
                    hdivider(),
                    virgo_gui::terminal_panel::view(app)
                ]
                .height(Length::Fill)
                .into()
            } else {
                panels
            }
        }
        app::Screen::Memory => virgo_gui::memory::view(app),
        app::Screen::Settings => virgo_gui::settings::view(app),
        app::Screen::Projects | app::Screen::Project => virgo_gui::workspaces::view(app),
    };

    let center = column![
        virgo_gui::top_bar::view(app),
        container(page).height(Length::Fill),
        hdivider(),
        virgo_gui::status_bar::view(app),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    let shell = row![virgo_gui::rail::view(app), vdivider(), center].height(Length::Fill);

    if app.palette_open {
        stack![shell, virgo_gui::palette::view(app)].into()
    } else {
        shell.into()
    }
}
