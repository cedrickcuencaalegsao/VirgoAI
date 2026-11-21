//! Memory screen: searchable note list, an interactive map of how notes
//! connect, and an inspector / editor for the selected note.

use iced::alignment::{Horizontal, Vertical};
use iced::widget::{
    Space, button, canvas, column, container, horizontal_space, row, scrollable, text, text_editor,
    text_input,
};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, event, mouse};

use memory::graph::{Graph, Node};

use super::short;
use app::{App, Message};

pub fn view(app: &App) -> Element<'_, Message> {
    row![notes_panel(app), map_panel(app), side_panel(app)]
        .spacing(14)
        .height(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// Left: search, filters, note cards
// ---------------------------------------------------------------------------

/// Indexes of the notes that pass the search text and the group filter.
fn visible(app: &App) -> Vec<usize> {
    let hits = app.memory_graph.matches(&app.memory_query);
    app.memory_graph
        .nodes
        .iter()
        .enumerate()
        .filter(|(i, n)| hits[*i] && app.memory_filter.map_or(true, |g| n.group == g))
        .map(|(i, _)| i)
        .collect()
}

fn notes_panel(app: &App) -> Element<'_, Message> {
    let header = row![
        column![
            text("Memory").size(22).font(style::bold()),
            text("Markdown notes on your SSD")
                .size(12)
                .style(style::dim),
        ]
        .spacing(2),
        horizontal_space(),
        button(icon_manager::tint(
            icon_manager::refresh_cw(),
            16.0,
            icon_manager::muted()
        ))
        .padding(8)
        .on_press(Message::ReloadNotes)
        .style(style::ghost_button),
    ]
    .align_y(Vertical::Center);

    let search = text_input("Search notes...", &app.memory_query)
        .on_input(Message::MemoryQueryChanged)
        .padding(9)
        .size(13)
        .style(style::field);

    let mut chips = row![chip(
        "All",
        app.memory_filter.is_none(),
        Message::MemoryFilter(None)
    )]
    .spacing(6);
    for (i, name) in app.memory_graph.groups.iter().enumerate().take(4) {
        chips = chips.push(chip(
            name.as_str(),
            app.memory_filter == Some(i),
            Message::MemoryFilter(Some(i)),
        ));
    }

    let new_note = row![
        text_input("new-note-name", &app.new_note_name)
            .on_input(Message::NewNoteNameChanged)
            .on_submit(Message::CreateNote)
            .padding(9)
            .size(13)
            .style(style::field),
        button(icon_manager::label(
            icon_manager::plus(),
            "Add",
            13,
            icon_manager::on_primary()
        ))
        .padding([9, 14])
        .on_press(Message::CreateNote)
        .style(style::primary_button),
    ]
    .spacing(6);

    let ids = visible(app);
    let list: Element<'_, Message> = if ids.is_empty() {
        text("No notes match.").size(12).style(style::dim).into()
    } else {
        scrollable(column(ids.into_iter().map(|i| note_card(app, i))).spacing(6))
            .style(style::scroll)
            .height(Length::Fill)
            .into()
    };

    container(column![header, search, chips, new_note, list].spacing(12))
        .padding(16)
        .width(Length::Fixed(290.0))
        .height(Length::Fill)
        .style(style::card)
        .into()
}

fn chip(label: &str, active: bool, message: Message) -> Element<'_, Message> {
    super::tab(label, active, message)
}

fn dot<'a>(color: Color) -> Element<'a, Message> {
    container(Space::new(Length::Fixed(9.0), Length::Fixed(9.0)))
        .style(style::dot(color))
        .into()
}

fn note_card(app: &App, i: usize) -> Element<'_, Message> {
    let node = &app.memory_graph.nodes[i];
    let selected = app.selected_note.as_deref() == Some(node.name.as_str());
    let sub = if node.preview.is_empty() {
        node.name.clone()
    } else {
        node.preview.clone()
    };

    let body = row![
        dot(style::group_color(node.group)),
        column![
            text(short(&node.title, 26)).size(14).font(style::bold()),
            text(short(&sub, 34)).size(11).style(style::dim),
        ]
        .spacing(3),
        horizontal_space(),
        text(format!("{}w", node.words)).size(11).style(style::dim),
    ]
    .spacing(10)
    .align_y(Vertical::Center);

    button(body)
        .width(Length::Fill)
        .padding([10, 12])
        .on_press(Message::SelectNote(node.name.clone()))
        .style(style::note_card(selected))
        .into()
}

// ---------------------------------------------------------------------------
// Middle: the map
// ---------------------------------------------------------------------------

fn map_panel(app: &App) -> Element<'_, Message> {
    let graph = &app.memory_graph;

    let header = row![
        column![
            text("Memory map").size(18).font(style::bold()),
            text("Hover a note to see its connections. Click to open it.")
                .size(12)
                .style(style::dim),
        ]
        .spacing(2),
        horizontal_space(),
        legend(graph),
    ]
    .align_y(Vertical::Center);

    let body: Element<'_, Message> = if graph.nodes.is_empty() {
        container(
            text("Your memory is empty. Add a note and it will appear here.")
                .size(13)
                .style(style::dim),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    } else {
        canvas(MapProgram {
            graph,
            selected: app.selected_note.as_deref().and_then(|n| graph.index_of(n)),
            hover: app.memory_hover,
            matches: graph.matches(&app.memory_query),
            filter: app.memory_filter,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    };

    container(
        column![
            header,
            container(body)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(style::map_area),
        ]
        .spacing(12),
    )
    .padding(16)
    .width(Length::FillPortion(5))
    .height(Length::Fill)
    .style(style::card)
    .into()
}

fn legend(graph: &Graph) -> Element<'_, Message> {
    row(graph.groups.iter().enumerate().take(5).map(|(i, name)| {
        let item: Element<'_, Message> = row![
            dot(style::group_color(i)),
            text(name.as_str()).size(11).style(style::dim),
        ]
        .spacing(5)
        .align_y(Vertical::Center)
        .into();
        item
    }))
    .spacing(12)
    .into()
}

const PAD: f32 = 64.0;

/// Layout coordinates (-1..1) to canvas pixels.
fn place(n: &Node, size: Size) -> Point {
    let w = (size.width - PAD * 2.0).max(1.0);
    let h = (size.height - PAD * 2.0).max(1.0);
    Point::new(PAD + (n.x * 0.5 + 0.5) * w, PAD + (n.y * 0.5 + 0.5) * h)
}

/// Busier and longer notes get bigger nodes.
fn radius(n: &Node) -> f32 {
    10.0 + (n.degree as f32).min(8.0) * 1.5 + (n.words as f32).sqrt().min(18.0) * 0.25
}

struct MapProgram<'a> {
    graph: &'a Graph,
    selected: Option<usize>,
    hover: Option<usize>,
    matches: Vec<bool>,
    filter: Option<usize>,
}

impl MapProgram<'_> {
    /// Node passes the search and the group filter (otherwise it is dimmed).
    fn lit(&self, i: usize) -> bool {
        self.matches[i] && self.filter.map_or(true, |g| self.graph.nodes[i].group == g)
    }

    fn hit(&self, p: Point, size: Size) -> Option<usize> {
        self.graph
            .nodes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, n)| {
                let c = place(n, size);
                let r = radius(n) + 4.0;
                let (dx, dy) = (c.x - p.x, c.y - p.y);
                dx * dx + dy * dy <= r * r
            })
            .map(|(i, _)| i)
    }
}

impl canvas::Program<Message> for MapProgram<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> (event::Status, Option<Message>) {
        let Some(pos) = cursor.position_in(bounds) else {
            let clear = self.hover.is_some().then_some(Message::MemoryHover(None));
            return (event::Status::Ignored, clear);
        };
        let hit = self.hit(pos, bounds.size());

        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(i) = hit {
                    return (event::Status::Captured, Some(Message::MemoryNodeClicked(i)));
                }
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if hit != self.hover {
                    return (event::Status::Captured, Some(Message::MemoryHover(hit)));
                }
            }
            _ => {}
        }
        (event::Status::Ignored, None)
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let size = bounds.size();
        let mut frame = canvas::Frame::new(renderer, size);
        let nodes = &self.graph.nodes;
        let gold = style::signal();
        let text_color = style::palette().text;

        // Dotted background grid (one path, one fill).
        let grid = canvas::Path::new(|b| {
            let mut x = 14.0;
            while x < size.width {
                let mut y = 14.0;
                while y < size.height {
                    b.rectangle(Point::new(x, y), Size::new(1.5, 1.5));
                    y += 28.0;
                }
                x += 28.0;
            }
        });
        frame.fill(&grid, style::alpha(Color::WHITE, 0.07));

        // Edges first, so nodes sit on top. Edges of the hovered/selected node glow.
        let focus = self.hover.or(self.selected);
        for e in &self.graph.edges {
            let a = place(&nodes[e.a], size);
            let b = place(&nodes[e.b], size);
            let active = focus.is_some_and(|f| f == e.a || f == e.b);
            let (alpha, width) = if active {
                (0.90, 2.2)
            } else if e.explicit {
                (0.38, 1.4)
            } else {
                (0.14, 1.0)
            };
            frame.stroke(
                &canvas::Path::line(a, b),
                canvas::Stroke::default()
                    .with_color(style::alpha(gold, alpha))
                    .with_width(width),
            );
        }

        // Nodes and labels.
        for (i, n) in nodes.iter().enumerate() {
            let c = place(n, size);
            let r = radius(n);
            let color = style::group_color(n.group);
            let lit = self.lit(i);
            let is_hub = self.graph.hub == Some(i);
            let emphasised = focus == Some(i);

            if emphasised {
                frame.fill(
                    &canvas::Path::circle(c, r + 12.0),
                    style::alpha(color, 0.12),
                );
                frame.fill(&canvas::Path::circle(c, r + 6.0), style::alpha(color, 0.20));
            }
            frame.fill(
                &canvas::Path::circle(c, r),
                style::alpha(color, if lit { 1.0 } else { 0.22 }),
            );
            frame.stroke(
                &canvas::Path::circle(c, r),
                canvas::Stroke::default()
                    .with_color(style::alpha(Color::WHITE, if is_hub { 0.9 } else { 0.18 }))
                    .with_width(if is_hub { 2.0 } else { 1.0 }),
            );
            frame.fill_text(canvas::Text {
                content: n.title.clone(),
                position: Point::new(c.x, c.y + r + 8.0),
                color: style::alpha(text_color, if lit { 0.92 } else { 0.30 }),
                size: 12.0.into(),
                horizontal_alignment: Horizontal::Center,
                vertical_alignment: Vertical::Top,
                ..canvas::Text::default()
            });
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match cursor
            .position_in(bounds)
            .and_then(|p| self.hit(p, bounds.size()))
        {
            Some(_) => mouse::Interaction::Pointer,
            None => mouse::Interaction::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Right: overview or inspector / editor
// ---------------------------------------------------------------------------

fn side_panel(app: &App) -> Element<'_, Message> {
    let body = match &app.selected_note {
        Some(name) => inspector(app, name.as_str()),
        None => overview(app),
    };
    container(body)
        .padding(16)
        .width(Length::FillPortion(4))
        .height(Length::Fill)
        .style(style::card)
        .into()
}

fn stat<'a>(label: &'a str, value: String) -> Element<'a, Message> {
    container(
        column![
            text(value).size(20).font(style::bold()),
            text(label).size(11).style(style::dim),
        ]
        .spacing(2),
    )
    .padding([10, 14])
    .width(Length::Fill)
    .style(style::stat_pill)
    .into()
}

fn overview(app: &App) -> Element<'_, Message> {
    let g = &app.memory_graph;
    let words: usize = g.nodes.iter().map(|n| n.words).sum();
    let links = g.edges.iter().filter(|e| e.explicit).count();

    column![
        text("Overview").size(18).font(style::bold()),
        text("Select a note on the map or in the list to read and edit it.")
            .size(13)
            .style(style::dim),
        row![
            stat("Notes", g.nodes.len().to_string()),
            stat("Links", links.to_string()),
            stat("Words", words.to_string()),
        ]
        .spacing(8),
        text("Always sent to the model").size(14).font(style::bold()),
        text("profile.md and project/project.md go into every prompt. Other notes are picked by keyword.")
            .size(12)
            .style(style::dim),
        text("Tip: write [[note-name]] inside a note to connect it to another note on the map.")
            .size(12)
            .style(style::accent),
    ]
    .spacing(14)
    .into()
}

fn inspector<'a>(app: &'a App, name: &'a str) -> Element<'a, Message> {
    let graph = &app.memory_graph;
    let idx = graph.index_of(name);

    let group = idx
        .map(|i| graph.groups[graph.nodes[i].group].clone())
        .unwrap_or_default();

    let top = row![
        icon_manager::tint(icon_manager::file_text(), 18.0, icon_manager::accent()),
        column![
            text(short(name, 30)).size(16).font(style::bold()),
            text(group).size(11).style(style::dim),
        ]
        .spacing(1),
        horizontal_space(),
        button(icon_manager::tint(
            icon_manager::x(),
            14.0,
            icon_manager::muted()
        ))
        .padding(6)
        .on_press(Message::MemoryClose)
        .style(style::ghost_button),
    ]
    .spacing(10)
    .align_y(Vertical::Center);

    let stats: Element<'a, Message> = match idx {
        Some(i) => row![
            stat("Words", graph.nodes[i].words.to_string()),
            stat("Connections", graph.neighbors(i).len().to_string()),
        ]
        .spacing(8)
        .into(),
        None => Space::new(Length::Shrink, Length::Shrink).into(),
    };

    // Jump to connected notes.
    let mut links = row![].spacing(6).align_y(Vertical::Center);
    if let Some(i) = idx {
        for j in graph.neighbors(i).into_iter().take(4) {
            links = links.push(
                button(text(short(&graph.nodes[j].title, 16)).size(12))
                    .padding([4, 10])
                    .on_press(Message::MemoryNodeClicked(j))
                    .style(style::chip),
            );
        }
    }

    let actions = row![
        button(icon_manager::label(
            icon_manager::check(),
            "Save",
            13,
            icon_manager::on_primary()
        ))
        .padding([8, 18])
        .on_press(Message::SaveNote)
        .style(style::primary_button),
        button(icon_manager::label(
            icon_manager::trash_2(),
            "Delete",
            13,
            icon_manager::on_danger()
        ))
        .padding([8, 18])
        .on_press(Message::DeleteNote)
        .style(style::danger_button),
    ]
    .spacing(8);

    column![
        top,
        stats,
        links,
        text_editor(&app.editor)
            .on_action(Message::EditorAction)
            .height(Length::Fill),
        actions,
    ]
    .spacing(12)
    .height(Length::Fill)
    .into()
}
