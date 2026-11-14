use std::error::Error as _;
use std::pin::Pin;

use futures::{Stream, StreamExt};
use reqwest::RequestBuilder;
use serde_json::{json, Value};

use super::types::{ChatChunk, ChatRequest, Endpoint, Provider, StreamEvent, TagsResponse};

type ByteStream = Pin<Box<dyn Stream<Item = Result<Vec<u8>, reqwest::Error>> + Send>>;

const ANTHROPIC_VERSION: &str = "2023-06-01";
const ANTHROPIC_MAX_TOKENS: u32 = 8192;

/// Models from every configured provider, plus one error per provider that failed.
#[derive(Debug, Clone, Default)]
pub struct Models {
    pub names: Vec<String>,
    pub errors: Vec<String>,
}

fn http() -> reqwest::Client {
    reqwest::Client::new()
}

/// Clean up the configured URL: drop stray whitespace or invisible characters,
/// default the scheme to `http`, remove stray `:` or `/` right after `://`
/// (e.g. `http://:localhost:11434`), and remove trailing slashes.
pub fn normalize_base(base: &str) -> String {
    let cleaned: String = base
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();

    let (scheme, rest) = match cleaned.split_once("://") {
        Some((scheme, rest)) if !scheme.is_empty() => (scheme.to_string(), rest.to_string()),
        Some((_, rest)) => ("http".to_string(), rest.to_string()),
        None => ("http".to_string(), cleaned.clone()),
    };

    let rest = rest
        .trim_start_matches(|c| c == ':' || c == '/')
        .trim_end_matches('/');
    format!("{scheme}://{rest}")
}

fn endpoint(base: &str, path: &str) -> String {
    format!("{}/{}", normalize_base(base), path.trim_start_matches('/'))
}

fn authorize(ep: &Endpoint, req: RequestBuilder) -> RequestBuilder {
    match ep.provider {
        Provider::Ollama => req,
        Provider::OpenAi | Provider::Xai => req.bearer_auth(ep.api_key.trim()),
        Provider::Anthropic => req
            .header("x-api-key", ep.api_key.trim())
            .header("anthropic-version", ANTHROPIC_VERSION),
    }
}

fn post(ep: &Endpoint, path: &str) -> RequestBuilder {
    authorize(ep, http().post(endpoint(&ep.base_url, path)))
}

fn get(ep: &Endpoint, path: &str) -> RequestBuilder {
    authorize(ep, http().get(endpoint(&ep.base_url, path)))
}

fn check_key(ep: &Endpoint) -> Result<(), String> {
    if ep.provider != Provider::Ollama && ep.api_key.trim().is_empty() {
        Err(format!(
            "No API key set for {}. Add it in Settings.",
            ep.provider.label()
        ))
    } else {
        Ok(())
    }
}

fn chat_path(p: Provider) -> &'static str {
    match p {
        Provider::Ollama => "api/chat",
        Provider::OpenAi | Provider::Xai => "chat/completions",
        Provider::Anthropic => "messages",
    }
}

/// reqwest hides the real cause behind messages like "builder error".
/// Walk the error chain so the UI shows the root cause too.
fn describe(err: &reqwest::Error) -> String {
    let mut message = err.to_string();
    let mut source = err.source();
    while let Some(inner) = source {
        message.push_str(": ");
        message.push_str(&inner.to_string());
        source = inner.source();
    }
    message
}

fn cannot_reach(ep: &Endpoint, err: &reqwest::Error) -> String {
    format!(
        "Cannot reach {} at {}: {}",
        ep.provider.label(),
        endpoint(&ep.base_url, ""),
        describe(err)
    )
}

/// Turn a non-2xx response into a readable error message.
/// Handles `{"error": "text"}` (Ollama) and `{"error": {"message": "text"}}` (others).
async fn ensure_ok(ep: &Endpoint, resp: reqwest::Response) -> Result<reqwest::Response, String> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let message = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| match v.get("error")? {
            Value::String(s) => Some(s.clone()),
            e => e.get("message").and_then(|m| m.as_str()).map(String::from),
        })
        .unwrap_or(body);
    Err(format!("{} returned {status}: {message}", ep.provider.label()))
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

fn is_chat_model(p: Provider, id: &str) -> bool {
    const SKIP: &[&str] = &[
        "embedding", "moderation", "tts", "whisper", "transcribe", "realtime", "audio", "image",
        "dall-e", "davinci", "babbage", "search",
    ];
    if SKIP.iter().any(|s| id.contains(s)) {
        return false;
    }
    match p {
        Provider::OpenAi => {
            id.starts_with("gpt")
                || id.starts_with("chatgpt")
                || id.starts_with("o1")
                || id.starts_with("o3")
                || id.starts_with("o4")
        }
        _ => true,
    }
}

/// Models of one provider. Remote names come back with their prefix (`openai:...`).
pub async fn list_models(ep: Endpoint) -> Result<Vec<String>, String> {
    check_key(&ep)?;

    let mut req = match ep.provider {
        Provider::Ollama => get(&ep, "api/tags"),
        _ => get(&ep, "models"),
    };
    if ep.provider == Provider::Anthropic {
        req = req.query(&[("limit", "100")]);
    }
    let resp = req.send().await.map_err(|e| cannot_reach(&ep, &e))?;
    let resp = ensure_ok(&ep, resp).await?;

    let mut names: Vec<String> = match ep.provider {
        Provider::Ollama => {
            let tags: TagsResponse = resp.json().await.map_err(|e| describe(&e))?;
            tags.models.into_iter().map(|m| m.name).collect()
        }
        p => {
            let v: Value = resp.json().await.map_err(|e| describe(&e))?;
            v["data"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m["id"].as_str())
                .filter(|id| is_chat_model(p, id))
                .map(|id| format!("{}{id}", p.prefix()))
                .collect()
        }
    };
    names.sort();
    Ok(names)
}

/// Ask every endpoint. A failing provider does not hide the others.
pub async fn list_all_models(endpoints: Vec<Endpoint>) -> Models {
    let mut out = Models::default();
    for ep in endpoints {
        match list_models(ep).await {
            Ok(names) => out.names.extend(names),
            Err(e) => out.errors.push(e),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Request bodies
// ---------------------------------------------------------------------------

fn chat_body(ep: &Endpoint, req: &ChatRequest, stream: bool) -> Value {
    let (_, name) = Provider::split(&req.model);

    match ep.provider {
        Provider::Ollama => {
            let mut body = serde_json::to_value(req).unwrap_or_else(|_| json!({}));
            body["stream"] = json!(stream);
            body
        }
        Provider::OpenAi | Provider::Xai => {
            let messages: Vec<Value> = req
                .messages
                .iter()
                .map(|m| json!({ "role": m.role, "content": m.content }))
                .collect();
            json!({ "model": name, "messages": messages, "stream": stream })
        }
        Provider::Anthropic => {
            // Claude takes the system prompt separately and wants the chat to start with a user turn.
            let system = req
                .messages
                .iter()
                .filter(|m| m.role == "system")
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let messages: Vec<Value> = req
                .messages
                .iter()
                .filter(|m| m.role != "system")
                .skip_while(|m| m.role != "user")
                .map(|m| json!({ "role": m.role, "content": m.content }))
                .collect();
            let mut body = json!({
                "model": name,
                "max_tokens": ANTHROPIC_MAX_TOKENS,
                "messages": messages,
                "stream": stream,
            });
            if !system.is_empty() {
                body["system"] = json!(system);
            }
            body
        }
    }
}

/// Non-streaming chat call: returns the full reply text.
pub async fn chat_once(ep: Endpoint, req: ChatRequest) -> Result<String, String> {
    check_key(&ep)?;
    let resp = post(&ep, chat_path(ep.provider))
        .json(&chat_body(&ep, &req, false))
        .send()
        .await
        .map_err(|e| cannot_reach(&ep, &e))?;
    let resp = ensure_ok(&ep, resp).await?;
    let v: Value = resp.json().await.map_err(|e| describe(&e))?;

    match ep.provider {
        Provider::Ollama => {
            let chunk: ChatChunk = serde_json::from_value(v).map_err(|e| e.to_string())?;
            if let Some(err) = chunk.error {
                return Err(err);
            }
            Ok(chunk.message.map(|m| m.content).unwrap_or_default())
        }
        Provider::OpenAi | Provider::Xai => Ok(v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default()
            .to_string()),
        Provider::Anthropic => Ok(v["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("")),
    }
}

/// Load a local model into memory by sending a request with no messages.
pub async fn warm_up(
    ep: Endpoint,
    models: Vec<String>,
    keep_alive: String,
) -> Result<String, String> {
    for model in &models {
        let body = json!({ "model": model, "keep_alive": keep_alive });
        let resp = post(&ep, "api/chat")
            .json(&body)
            .send()
            .await
            .map_err(|e| cannot_reach(&ep, &e))?;
        ensure_ok(&ep, resp).await?;
    }
    Ok(format!("Loaded: {}", models.join(", ")))
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

enum StreamState {
    Init(Box<(Endpoint, ChatRequest)>),
    Running { provider: Provider, bytes: ByteStream, buf: Vec<u8> },
    Finished,
}

/// Streaming chat call. Ollama sends one JSON object per line (NDJSON), the other
/// providers send server-sent events (`data: {...}` lines). Network chunks can split
/// lines, so we buffer bytes until a full line is available.
/// The request is only sent when the stream is first polled.
pub fn chat_stream(ep: Endpoint, req: ChatRequest) -> impl Stream<Item = StreamEvent> {
    futures::stream::unfold(StreamState::Init(Box::new((ep, req))), next_event)
}

async fn open_stream(ep: &Endpoint, req: &ChatRequest) -> Result<ByteStream, String> {
    check_key(ep)?;
    let resp = post(ep, chat_path(ep.provider))
        .json(&chat_body(ep, req, true))
        .send()
        .await
        .map_err(|e| cannot_reach(ep, &e))?;
    let resp = ensure_ok(ep, resp).await?;
    Ok(Box::pin(resp.bytes_stream().map(|r| r.map(|b| b.to_vec()))))
}

fn bad_line(e: serde_json::Error) -> Option<StreamEvent> {
    Some(StreamEvent::Error(format!("Bad response line: {e}")))
}

fn api_error(v: &Value) -> Option<String> {
    let e = v.get("error")?;
    if e.is_null() {
        return None;
    }
    Some(
        e["message"]
            .as_str()
            .or_else(|| e.as_str())
            .unwrap_or("unknown API error")
            .to_string(),
    )
}

fn parse_ollama(text: &str) -> Option<StreamEvent> {
    let chunk: ChatChunk = match serde_json::from_str(text) {
        Ok(chunk) => chunk,
        Err(e) => return bad_line(e),
    };
    if let Some(err) = chunk.error {
        return Some(StreamEvent::Error(err));
    }
    if chunk.done {
        return Some(StreamEvent::Done);
    }

    let message = chunk.message.unwrap_or_default();
    if !message.content.is_empty() {
        Some(StreamEvent::Token(message.content))
    } else if !message.thinking.is_empty() {
        Some(StreamEvent::Thinking(message.thinking))
    } else {
        None
    }
}

/// OpenAI and xAI: `data: {"choices":[{"delta":{"content":"..."}}]}` and `data: [DONE]`.
/// Grok reasoning models send their reasoning as `reasoning_content`.
fn parse_openai(text: &str) -> Option<StreamEvent> {
    let data = text.strip_prefix("data:")?.trim();
    if data == "[DONE]" {
        return Some(StreamEvent::Done);
    }
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => return bad_line(e),
    };
    if let Some(msg) = api_error(&v) {
        return Some(StreamEvent::Error(msg));
    }
    let delta = &v["choices"][0]["delta"];
    if let Some(t) = delta["content"].as_str().filter(|s| !s.is_empty()) {
        return Some(StreamEvent::Token(t.to_string()));
    }
    delta["reasoning_content"]
        .as_str()
        .or_else(|| delta["reasoning"].as_str())
        .filter(|s| !s.is_empty())
        .map(|t| StreamEvent::Thinking(t.to_string()))
}

/// Anthropic: `data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"..."}}`.
fn parse_anthropic(text: &str) -> Option<StreamEvent> {
    let data = text.strip_prefix("data:")?.trim();
    let v: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(e) => return bad_line(e),
    };
    match v["type"].as_str()? {
        "content_block_delta" => {
            let delta = &v["delta"];
            match delta["type"].as_str()? {
                "text_delta" => delta["text"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|t| StreamEvent::Token(t.to_string())),
                "thinking_delta" => delta["thinking"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|t| StreamEvent::Thinking(t.to_string())),
                _ => None,
            }
        }
        "message_stop" => Some(StreamEvent::Done),
        "error" => Some(StreamEvent::Error(
            api_error(&v).unwrap_or_else(|| "unknown API error".into()),
        )),
        _ => None,
    }
}

fn parse_line(provider: Provider, line: &[u8]) -> Option<StreamEvent> {
    let text = std::str::from_utf8(line).ok()?.trim();
    if text.is_empty() {
        return None;
    }
    match provider {
        Provider::Ollama => parse_ollama(text),
        Provider::OpenAi | Provider::Xai => parse_openai(text),
        Provider::Anthropic => parse_anthropic(text),
    }
}

async fn next_event(mut state: StreamState) -> Option<(StreamEvent, StreamState)> {
    loop {
        match state {
            StreamState::Init(boxed) => {
                let (ep, req) = *boxed;
                match open_stream(&ep, &req).await {
                    Ok(bytes) => {
                        state = StreamState::Running {
                            provider: ep.provider,
                            bytes,
                            buf: Vec::new(),
                        };
                    }
                    Err(e) => return Some((StreamEvent::Error(e), StreamState::Finished)),
                }
            }
            StreamState::Running { provider, mut bytes, mut buf } => {
                // Emit any complete line already buffered.
                if let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                    let line: Vec<u8> = buf.drain(..=pos).collect();
                    match parse_line(provider, &line) {
                        Some(StreamEvent::Done) => {
                            return Some((StreamEvent::Done, StreamState::Finished))
                        }
                        Some(StreamEvent::Error(e)) => {
                            return Some((StreamEvent::Error(e), StreamState::Finished))
                        }
                        Some(event) => {
                            return Some((event, StreamState::Running { provider, bytes, buf }))
                        }
                        None => {
                            state = StreamState::Running { provider, bytes, buf };
                            continue;
                        }
                    }
                }

                // Otherwise read more bytes from the network.
                match bytes.next().await {
                    Some(Ok(chunk)) => {
                        buf.extend_from_slice(&chunk);
                        state = StreamState::Running { provider, bytes, buf };
                    }
                    Some(Err(e)) => {
                        return Some((StreamEvent::Error(describe(&e)), StreamState::Finished))
                    }
                    None => return Some((StreamEvent::Done, StreamState::Finished)),
                }
            }
            StreamState::Finished => return None,
        }
    }
}