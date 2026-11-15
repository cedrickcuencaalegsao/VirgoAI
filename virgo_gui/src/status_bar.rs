use iced::alignment::Vertical;
use iced::widget::{container, horizontal_space, progress_bar, row, text};
use iced::{Background, Border, Element, Length, Theme};

use super::short;
use app::{App, Message};

fn meter_style(warn: bool) -> impl Fn(&Theme) -> progress_bar::Style {
    move |_t: &Theme| {
        let bar = if warn {
            iced::Color::from_rgb8(0xF2, 0xB1, 0x34)
        } else {
            style::live()
        };
        progress_bar::Style {
            background: Background::Color(style::alpha(iced::Color::WHITE, 0.08)),
            bar: Background::Color(bar),
            border: Border {
                radius: 2.0.into(),
                ..Border::default()
            },
        }
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let used: usize = app
        .messages
        .iter()
        .map(|m| m.content.chars().count())
        .sum::<usize>()
        / 4;
    let limit = app.cfg.num_ctx as usize;
    let ratio = (used as f32 / limit.max(1) as f32).clamp(0.0, 1.0);

    let planner = if app.cfg.use_planner {
        format!("Planner {}", short(&app.cfg.planner_model, 22))
    } else {
        "Planner off".to_string()
    };

    let models = text(format!(
        "{planner}   Executor {}",
        short(&app.cfg.executor_model, 22)
    ))
    .size(12)
    .style(style::dim);

    let meter = row![
        text(format!("Context ~{used} / {limit}"))
            .size(12)
            .style(style::dim),
        container(progress_bar(0.0..=1.0, ratio).style(meter_style(ratio > 0.85)))
            .width(Length::Fixed(80.0))
            .height(Length::Fixed(4.0)),
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    container(
        row![
            text(short(&app.status, 90)).size(12).style(style::dim),
            horizontal_space(),
            models,
            meter,
        ]
        .spacing(18)
        .align_y(Vertical::Center),
    )
    .padding([6, 14])
    .width(Length::Fill)
    .style(style::status_bar)
    .into()
}
