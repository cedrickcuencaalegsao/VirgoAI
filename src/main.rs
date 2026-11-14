use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use app::{App, Message};

fn main() -> iced::Result {
    iced::application("Virgo AI", App::update, view)
        .subscription(App::subscription)
        .theme(App::theme)
        // Put Inter-Regular.ttf and Inter-Bold.ttf in assets/fonts/
        .font(&include_bytes!("../assets/fonts/Inter-Regular.ttf")[..])
        .font(&include_bytes!("../assets/fonts/Inter-Bold.ttf")[..])
        .default_font(iced::Font::with_name("Inter"))
        .antialiasing(true)
        .window_size(iced::Size::new(1480.0, 820.0))
        .run_with(App::new)
}

// -----------------------------------------------------------------------
// View
// -----------------------------------------------------------------------

fn view(app: &App) -> Element<'_, Message> {
    let page: Element<'_, Message> = match app.screen {
        app::Screen::Chat => {
            let chat_width = if app.code.is_some() {
                Length::FillPortion(2)
            } else {
                Length::Fill
            };

            let mut panels = row![
                container(virgo_gui::chat::view(app))
                    .width(chat_width)
                    .height(Length::Fill)
            ]
            .spacing(14)
            .height(Length::Fill);

            if app.code.is_some() {
                panels = panels.push(
                    container(virgo_gui::code_panel::view(app))
                        .width(Length::FillPortion(3))
                        .height(Length::Fill),
                );
            }

            if app.show_panel {
                panels = panels.push(virgo_gui::project_panel::view(app));
            }

            let panels: Element<'_, Message> = panels.into();

            if app.show_terminal {
                column![
                    container(panels).height(Length::Fill),
                    virgo_gui::terminal_panel::view(app)
                ]
                .spacing(14)
                .height(Length::Fill)
                .into()
            } else {
                panels
            }
        }

        app::Screen::Memory => virgo_gui::memory::view(app),
        app::Screen::Settings => virgo_gui::settings::view(app),
        app::Screen::Projects | app::Screen::Project => {
            virgo_gui::workspaces::view(app)
        }
    };

    let footer = text(app.status.as_str())
        .size(12)
        .style(style::dim);

    let content = column![container(page).height(Length::Fill), footer]
        .spacing(8)
        .padding(18)
        .width(Length::Fill);

    row![virgo_gui::nav::view(app), content]
        .height(Length::Fill)
        .into()
}