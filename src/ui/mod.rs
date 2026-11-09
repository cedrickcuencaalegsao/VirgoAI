pub mod chat;
pub mod code_panel;
pub mod markdown;
pub mod memory;
pub mod nav;
pub mod project_panel;
pub mod settings;
pub mod style;

/// Shorten text so it fits on one line, adding `...` when cut.
pub fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max).collect();
        format!("{head}...")
    }
}