pub mod actions;
pub mod chat;
pub mod code_panel;
pub mod dock;
pub mod markdown;
pub mod memory;
pub mod nav;
pub mod palette;
pub mod plan_panel;
pub mod project_panel;
pub mod rail;
pub mod settings;
pub mod sidebar;
pub mod status_bar;
pub mod terminal_panel;
pub mod top_bar;
pub mod workspaces;

/// Shorten text so it fits on one line, adding `...` when cut.
pub fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max).collect();
        format!("{head}...")
    }
}
