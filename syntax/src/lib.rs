//! Syntax highlighting: file text -> one list of colored spans per line.

use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Files longer than this are shown uncolored (keeps big files fast).
pub const MAX_LINES: usize = 6000;
const THEME: &str = "base16-eighties.dark";

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub color: [u8; 3],
    pub bold: bool,
    pub italic: bool,
}

fn syntaxes() -> &'static SyntaxSet {
    static S: OnceLock<SyntaxSet> = OnceLock::new();
    S.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn themes() -> &'static ThemeSet {
    static T: OnceLock<ThemeSet> = OnceLock::new();
    T.get_or_init(ThemeSet::load_defaults)
}

/// Extension used to look up the language. The bundled syntaxes have no
/// TypeScript, so those files borrow the JavaScript grammar.
pub fn token_for(rel: &str) -> String {
    let ext = rel.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match ext {
        "ts" | "tsx" | "jsx" | "mjs" | "cjs" => "js",
        "zsh" | "bash" => "sh",
        "yml" => "yaml",
        other => other,
    }
    .to_string()
}

/// Highlight a whole file. `None` when the language is unknown or the file is too long.
pub fn highlight(rel: &str, text: &str) -> Option<Vec<Vec<Span>>> {
    if text.lines().count() > MAX_LINES {
        return None;
    }
    let ss = syntaxes();
    let syntax = ss.find_syntax_by_token(&token_for(rel))?;
    let theme = themes().themes.get(THEME)?;
    let mut h = HighlightLines::new(syntax, theme);

    let mut out = Vec::new();
    for line in LinesWithEndings::from(text) {
        let ranges = h.highlight_line(line, ss).ok()?;
        out.push(
            ranges
                .into_iter()
                .map(|(style, piece)| Span {
                    text: piece.trim_end_matches(['\n', '\r']).to_string(),
                    color: [style.foreground.r, style.foreground.g, style.foreground.b],
                    bold: style.font_style.contains(FontStyle::BOLD),
                    italic: style.font_style.contains(FontStyle::ITALIC),
                })
                .filter(|s| !s.text.is_empty())
                .collect(),
        );
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_is_colored_line_by_line() {
        let lines = highlight("a.rs", "fn main() {}\nlet x = 1;\n").unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].len() > 1); // several tokens, not one plain span
    }

    #[test]
    fn unknown_language_is_none() {
        assert!(highlight("notes.zzz", "hello").is_none());
    }
}