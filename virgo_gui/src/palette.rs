use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use iced::widget::{button, column, container, text, text_input};
use iced::{Element, Length, Padding};

use super::short;
use app::{App, Command, Message};

pub fn input_id() -> text_input::Id {
    text_input::Id::new("palette-input")
}

fn commands(app: &App) -> Vec<(String, Message)> {
    let mut out: Vec<(String, Message)> = vec![
        ("New chat".into(), Message::PaletteRun(Command::NewChat)),
        (
            "Open projects".into(),
            Message::PaletteRun(Command::OpenProjects),
        ),
        (
            "Open memory".into(),
            Message::PaletteRun(Command::OpenMemory),
        ),
        (
            "Open settings".into(),
            Message::PaletteRun(Command::OpenSettings),
        ),
        (
            "Toggle side panel".into(),
            Message::PaletteRun(Command::TogglePanel),
        ),
        (
            "Toggle terminal".into(),
            Message::PaletteRun(Command::ToggleTerminal),
        ),
        (
            "Toggle edit mode".into(),
            Message::PaletteRun(Command::ToggleEditMode),
        ),
        (
            "Refresh models".into(),
            Message::PaletteRun(Command::RefreshModels),
        ),
        (
            "Revert last AI edit".into(),
            Message::PaletteRun(Command::RevertLast),
        ),
    ];
    for w in &app.workspaces {
        out.push((
            format!("Project: {}", w.name),
            Message::OpenWorkspace(w.slug.clone()),
        ));
    }
    for c in app.chats.iter().take(30) {
        out.push((
            format!("Chat: {}", short(&c.title, 50)),
            Message::OpenChat(c.id.clone()),
        ));
    }
    out
}

pub fn view(app: &App) -> Element<'_, Message> {
    let matcher = SkimMatcherV2::default();
    let query = app.palette_query.trim();

    let mut hits: Vec<(i64, String, Message)> = commands(app)
        .into_iter()
        .filter_map(|(label, msg)| {
            if query.is_empty() {
                Some((0, label, msg))
            } else {
                matcher
                    .fuzzy_match(&label, query)
                    .map(|score| (score, label, msg))
            }
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0)); // stable: ties keep their order

    let first = hits.first().map(|h| h.2.clone());

    let rows: Vec<Element<'_, Message>> = hits
        .into_iter()
        .take(8)
        .map(|(_, label, msg)| {
            button(text(label).size(14))
                .width(Length::Fill)
                .padding([9, 12])
                .on_press(msg)
                .style(style::nav_item(false))
                .into()
        })
        .collect();

    let list: Element<'_, Message> = if rows.is_empty() {
        text("Nothing matches. Try another word.")
            .size(13)
            .style(style::dim)
            .into()
    } else {
        column(rows).spacing(2).into()
    };

    let input = text_input("Type a command, project or chat name", &app.palette_query)
        .id(input_id())
        .on_input(Message::PaletteQuery)
        .on_submit_maybe(first)
        .padding(12)
        .size(15)
        .style(style::field);

    let panel = container(column![input, list].spacing(8))
        .padding(10)
        .width(Length::Fixed(520.0))
        .style(style::card);

    container(panel)
        .center_x(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 110.0,
            ..Padding::ZERO
        })
        .style(style::overlay_scrim)
        .into()
}
