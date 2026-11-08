use std::error::Error as _;
use std::pin::Pin;

use futures::{Stream, StreamExt};

use super::types::{ChatChunk, ChatRequest, StreamEvent, TagsResponse};

type ByteStream = Pin<Box<dyn Stream<Item = Result<Vec<u8>, reqwest::Error>> + Send>>;

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

fn cannot_reach(base: &str, err: &reqwest::Error) -> String {
    format!("Cannot reach Ollama at {}: {}", endpoint(base, ""), describe(err))
}

/// Turn a non-2xx response into a readable error message.
async fn ensure_ok(resp: reqwest::Response) -> Result<reqwest::Response, String> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let message = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or(body);
    Err(format!("Ollama returned {status}: {message}"))
}

/// `GET /api/tags`: models installed in Ollama.
pub async fn list_models(base_url: String) -> Result<Vec<String>, String> {
    let resp = http()
        .get(endpoint(&base_url, "api/tags"))
        .send()
        .await
        .map_err(|e| cannot_reach(&base_url, &e))?;
    let resp = ensure_ok(resp).await?;
    let tags: TagsResponse = resp.json().await.map_err(|e| describe(&e))?;
    let mut names: Vec<String> = tags.models.into_iter().map(|m| m.name).collect();
    names.sort();
    Ok(names)
}

/// Load models into memory by sending a request with no messages.
pub async fn warm_up(
    base_url: String,
    models: Vec<String>,
    keep_alive: String,
) -> Result<String, String> {
    for model in &models {
        let body = serde_json::json!({ "model": model, "keep_alive": keep_alive });
        let resp = http()
            .post(endpoint(&base_url, "api/chat"))
            .json(&body)
            .send()
            .await
            .map_err(|e| cannot_reach(&base_url, &e))?;
        ensure_ok(resp).await?;
    }
    Ok(format!("Loaded: {}", models.join(", ")))
}

// ---------------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------------

enum StreamState {
    Init(Box<(String, ChatRequest)>),
    Running { bytes: ByteStream, buf: Vec<u8> },
    Finished,
}

/// Streaming chat call. Ollama sends one JSON object per line (NDJSON); network
/// chunks can split lines, so we buffer bytes until a full line is available.
/// The request is only sent when the stream is first polled.
pub fn chat_stream(base_url: String, req: ChatRequest) -> impl Stream<Item = StreamEvent> {
    futures::stream::unfold(StreamState::Init(Box::new((base_url, req))), next_event)
}

async fn open_stream(base_url: &str, req: &ChatRequest) -> Result<ByteStream, String> {
    let resp = http()
        .post(endpoint(base_url, "api/chat"))
        .json(req)
        .send()
        .await
        .map_err(|e| cannot_reach(base_url, &e))?;
    let resp = ensure_ok(resp).await?;
    Ok(Box::pin(resp.bytes_stream().map(|r| r.map(|b| b.to_vec()))))
}

fn parse_line(line: &[u8]) -> Option<StreamEvent> {
    let text = std::str::from_utf8(line).ok()?.trim();
    if text.is_empty() {
        return None;
    }
    let chunk: ChatChunk = match serde_json::from_str(text) {
        Ok(chunk) => chunk,
        Err(e) => return Some(StreamEvent::Error(format!("Bad response line: {e}"))),
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

async fn next_event(mut state: StreamState) -> Option<(StreamEvent, StreamState)> {
    loop {
        match state {
            StreamState::Init(boxed) => {
                let (base_url, req) = *boxed;
                match open_stream(&base_url, &req).await {
                    Ok(bytes) => {
                        state = StreamState::Running { bytes, buf: Vec::new() };
                    }
                    Err(e) => return Some((StreamEvent::Error(e), StreamState::Finished)),
                }
            }
            StreamState::Running { mut bytes, mut buf } => {
                // Emit any complete line already buffered.
                if let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                    let line: Vec<u8> = buf.drain(..=pos).collect();
                    match parse_line(&line) {
                        Some(StreamEvent::Done) => {
                            return Some((StreamEvent::Done, StreamState::Finished))
                        }
                        Some(StreamEvent::Error(e)) => {
                            return Some((StreamEvent::Error(e), StreamState::Finished))
                        }
                        Some(event) => {
                            return Some((event, StreamState::Running { bytes, buf }))
                        }
                        None => {
                            state = StreamState::Running { bytes, buf };
                            continue;
                        }
                    }
                }

                // Otherwise read more bytes from the network.
                match bytes.next().await {
                    Some(Ok(chunk)) => {
                        buf.extend_from_slice(&chunk);
                        state = StreamState::Running { bytes, buf };
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