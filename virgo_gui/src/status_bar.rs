use iced::alignment::Vertical;
use iced::widget::{container, horizontal_space, progress_bar, row, text};
use iced::{Background, Border, Element, Length, Theme};

use super::short;
use app::{App, Message};

fn meter_style(warn: bool) -> impl Fn(&Theme) -> progress_bar::Style {
    move |_t: &Theme| progress_bar::Style {
        background: Background::Color(style::line()),
        bar: Background::Color(if warn {
            style::danger_color()
        } else {
            style::dim_color()
        }),
        border: Border::default(),
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

    let meter = row![
        text(format!("Context ~{used} / {limit}"))
            .size(style::T_12)
            .style(style::dim),
        container(progress_bar(0.0..=1.0, ratio).style(meter_style(ratio > 0.85)))
            .width(Length::Fixed(80.0))
            .height(Length::Fixed(2.0)),
    ]
    .spacing(style::SP_2)
    .align_y(Vertical::Center);

    container(
        row![
            text(short(&app.status, 90))
                .size(style::T_12)
                .style(style::dim),
            horizontal_space(),
            meter
        ]
        .spacing(style::SP_4)
        .align_y(Vertical::Center),
    )
    .padding([6, 16])
    .width(Length::Fill)
    .style(style::status_bar)
    .into()
}
