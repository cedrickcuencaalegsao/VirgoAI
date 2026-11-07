pub mod pipeline;

/// Who a chat bubble belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    /// The planner model's output.
    Plan,
    /// The executor model's output.
    Assistant,
    Error,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

impl ChatMessage {
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self { role, content: content.into() }
    }
}
