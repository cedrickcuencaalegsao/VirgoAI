pub mod pipeline;

use serde::{Deserialize, Serialize};

/// Who a chat bubble belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    /// The planner model's output.
    Plan,
    /// The executor model's output.
    Assistant,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
    /// The model's reasoning, kept separate from the answer.
    #[serde(default)]
    pub thinking: String,
    /// How long the model thought for, once it has finished.
    #[serde(default)]
    pub think_secs: Option<f32>,
    /// Whether the reasoning panel is expanded (UI state, not saved).
    #[serde(skip)]
    pub show_thinking: bool,
    /// Project files that were included with this message.
    #[serde(default)]
    pub files: Vec<String>,
}

impl ChatMessage {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            thinking: String::new(),
            think_secs: None,
            show_thinking: false,
            files: Vec::new(),
        }
    }

    pub fn with_files(mut self, files: Vec<String>) -> Self {
        self.files = files;
        self
    }
}