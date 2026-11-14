use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

impl ChatMsg {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Options {
    pub num_ctx: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMsg>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub think: Option<bool>,
    pub options: Options,
    pub keep_alive: String,
}

/// One line of the `/api/chat` stream.
#[derive(Debug, Deserialize)]
pub struct ChatChunk {
    #[serde(default)]
    pub message: Option<ChunkMessage>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ChunkMessage {
    #[serde(default)]
    pub content: String,
    /// Filled when the request has `think: true` and the model supports it.
    #[serde(default)]
    pub thinking: String,
}

#[derive(Debug, Deserialize)]
pub struct TagsResponse {
    pub models: Vec<TagModel>,
}

#[derive(Debug, Deserialize)]
pub struct TagModel {
    pub name: String,
}

/// What the UI receives while a reply streams in.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A piece of the model's reasoning.
    Thinking(String),
    /// A piece of the visible answer.
    Token(String),
    Done,
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Ollama,
    OpenAi,
    Anthropic,
    Xai,
}

impl Provider {
    pub fn prefix(self) -> &'static str {
        match self {
            Provider::Ollama => "",
            Provider::OpenAi => "openai:",
            Provider::Anthropic => "anthropic:",
            Provider::Xai => "xai:",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Provider::Ollama => "Ollama",
            Provider::OpenAi => "OpenAI",
            Provider::Anthropic => "Anthropic",
            Provider::Xai => "xAI",
        }
    }
    pub fn default_url(self) -> &'static str {
        match self {
            Provider::Ollama => "http://localhost::11434",
            Provider::OpenAi => "https://api.openai.com/v1",
            Provider::Anthropic => "https://api.anthropic.com/v1",
            Provider::Xai => "https://api.x.ai/v1",
        }
    }
    pub fn split(model: &str) -> (Provider, &str) {
        for p in [Provider::OpenAi, Provider::Anthropic, Provider::Xai] {
            if let Some(rest) = model.strip_prefix(p.prefix()) {
                return (p, rest);
            }
        }
        (Provider::Ollama, model)
    }
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    pub provider: Provider,
    pub base_url: String,
    pub api_key: String,
}
