//! A small markdown renderer for model replies: headings, bullet and numbered
//! lists, **bold**, `inline code`, horizontal rules, and fenced code blocks
//! with a Copy button. Unclosed code fences are handled, so streaming looks right.

use iced::widget::{
    button, column, container, horizontal_rule, horizontal_space, rich_text, row, span, text,
};
use iced::{Element, Font, Length};

use super::style;
use crate::app::Message;

enum Block {
    Heading { level: usize, text: String },
    Item { marker: String, text: String },
    Paragraph(String),
    Code { lang: String, code: String },
    Rule,
}

#[derive(Clone, Copy)]
enum Inline {
    Plain,
    Bold,
    Code,
}

/// Render markdown source as a column of widgets.
pub fn view<'a>(src: &str) -> Element<'a, Message> {
    column(parse(src).into_iter().map(block_view))
        .spacing(10)
        .into()
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn block_view<'a>(block: Block) -> Element<'a, Message> {
    match block {
        Block::Heading { level, text: content } => {
            let size = match level {
                1 => 26,
                2 => 21,
                3 => 18,
                _ => 16,
            };
            text(content).size(size).font(style::bold()).into()
        }
        Block::Item { marker, text: content } => row![
            text(marker).size(15).width(Length::Fixed(24.0)),
            inline_view(&content),
        ]
        .spacing(6)
        .into(),
        Block::Paragraph(content) => inline_view(&content),
        Block::Rule => horizontal_rule(1).into(),
        Block::Code { lang, code } => code_view(lang, code),
    }
}

fn code_view<'a>(lang: String, code: String) -> Element<'a, Message> {
    let label = if lang.is_empty() { "code".to_string() } else { lang };

    let header = row![
        text(label).size(11).style(style::dim),
        horizontal_space(),
        button(text("Copy").size(11))
            .padding([3, 10])
            .on_press(Message::Copy(code.clone()))
            .style(style::ghost_button),
    ]
    .align_y(iced::alignment::Vertical::Center);

    container(
        column![header, text(code).font(Font::MONOSPACE).size(13)].spacing(8),
    )
    .padding(12)
    .width(Length::Fill)
    .style(style::code_block)
    .into()
}

fn inline_view<'a>(src: &str) -> Element<'a, Message> {
    let spans: Vec<_> = inline(src)
        .into_iter()
        .map(|(piece, kind)| {
            let s = span(piece);
            match kind {
                Inline::Plain => s,
                Inline::Bold => s.font(style::bold()),
                Inline::Code => s.font(Font::MONOSPACE).color(style::palette().primary),
            }
        })
        .collect();
    rich_text(spans).size(15).into()
}

// ---------------------------------------------------------------------------
// Block parsing
// ---------------------------------------------------------------------------

fn parse(src: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut code: Vec<&str> = Vec::new();
    let mut lang = String::new();
    let mut in_code = false;

    for line in src.lines() {
        let trimmed = line.trim();

        if in_code {
            if trimmed.starts_with("```") {
                blocks.push(Block::Code {
                    lang: std::mem::take(&mut lang),
                    code: code.join("\n"),
                });
                code.clear();
                in_code = false;
            } else {
                code.push(line);
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("```") {
            flush(&mut para, &mut blocks);
            lang = rest.trim().to_string();
            in_code = true;
        } else if trimmed.is_empty() {
            flush(&mut para, &mut blocks);
        } else if let Some((level, rest)) = heading(trimmed) {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Heading { level, text: rest.to_string() });
        } else if is_rule(trimmed) {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Rule);
        } else if let Some(rest) = bullet(trimmed) {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Item { marker: "\u{2022}".to_string(), text: rest.to_string() });
        } else if let Some((marker, rest)) = numbered(trimmed) {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Item { marker, text: rest.to_string() });
        } else {
            para.push(line);
        }
    }

    // A code fence that is still open (the reply is streaming).
    if in_code {
        blocks.push(Block::Code { lang, code: code.join("\n") });
    }
    flush(&mut para, &mut blocks);
    blocks
}

fn flush(para: &mut Vec<&str>, blocks: &mut Vec<Block>) {
    if !para.is_empty() {
        blocks.push(Block::Paragraph(para.join("\n")));
        para.clear();
    }
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&level) && line[level..].starts_with(' ') {
        Some((level, line[level..].trim()))
    } else {
        None
    }
}

fn is_rule(line: &str) -> bool {
    line.len() >= 3
        && (line.chars().all(|c| c == '-')
            || line.chars().all(|c| c == '*')
            || line.chars().all(|c| c == '_'))
}

fn bullet(line: &str) -> Option<&str> {
    line.strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
}

fn numbered(line: &str) -> Option<(String, &str)> {
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let rest = &line[digits..];
    let rest = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") "))?;
    Some((format!("{}.", &line[..digits]), rest))
}

// ---------------------------------------------------------------------------
// Inline parsing: **bold** and `code`
// ---------------------------------------------------------------------------

fn inline(src: &str) -> Vec<(String, Inline)> {
    let chars: Vec<char> = src.chars().collect();
    let mut out: Vec<(String, Inline)> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '`' {
            if let Some(end) = find_seq(&chars, i + 1, &['`']) {
                push_plain(&mut out, &mut buf);
                out.push((chars[i + 1..end].iter().collect(), Inline::Code));
                i = end + 1;
                continue;
            }
        }
        if chars[i] == '*' && chars.get(i + 1) == Some(&'*') {
            if let Some(end) = find_seq(&chars, i + 2, &['*', '*']) {
                push_plain(&mut out, &mut buf);
                out.push((chars[i + 2..end].iter().collect(), Inline::Bold));
                i = end + 2;
                continue;
            }
        }
        buf.push(chars[i]);
        i += 1;
    }
    push_plain(&mut out, &mut buf);
    out
}

fn push_plain(out: &mut Vec<(String, Inline)>, buf: &mut String) {
    if !buf.is_empty() {
        out.push((std::mem::take(buf), Inline::Plain));
    }
}

/// Index of the first occurrence of `seq` at or after `from`.
fn find_seq(chars: &[char], from: usize, seq: &[char]) -> Option<usize> {
    chars
        .windows(seq.len())
        .enumerate()
        .skip(from)
        .find(|(_, window)| *window == seq)
        .map(|(index, _)| index)
}