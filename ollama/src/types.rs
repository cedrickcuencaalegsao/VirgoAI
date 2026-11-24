use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    /// `None` keeps the model's own default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// >1.0 discourages repeating recent tokens.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    /// How many recent tokens the penalty looks at.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_last_n: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Hard cap on generated tokens (thinking tokens count too).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_predict: Option<i32>,
}

impl Options {
    /// Anti-repetition defaults are on for every call.
    pub fn new(num_ctx: u32, temperature: Option<f32>) -> Self {
        Self {
            num_ctx,
            temperature,
            repeat_penalty: Some(1.1),
            repeat_last_n: Some(256),
            top_k: Some(40),
            top_p: Some(0.9),
            num_predict: None,
        }
    }

    /// Stop a runaway reply after `n` tokens.
    pub fn max_tokens(mut self, n: i32) -> Self {
        self.num_predict = Some(n);
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMsg>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub think: Option<bool>,
    /// A JSON schema the reply must follow (Ollama structured output).
    /// Other providers only use it as "reply with JSON".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<Value>,
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
            // Was "http://localhost::11434" (double colon).
            Provider::Ollama => "http://localhost:11434",
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
