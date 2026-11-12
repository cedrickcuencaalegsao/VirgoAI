//! Chat history: one JSON file per conversation in `<app dir>/chats/`.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use agent::{ChatMessage, Role};
use config::app_dir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    pub id: String,
    pub title: String,
    pub updated: u64, // keep whatever type you already have
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub project: Option<String>,
}

/// What the sidebar needs to list a conversation.
#[derive(Debug, Clone)]
pub struct ChatSummary {
    pub id: String,
    pub title: String,
    pub updated: u64,
    pub project: Option<String>,
}

/// Only the fields needed for listing, so message bodies are skipped.
#[derive(Deserialize)]
struct Header {
    id: String,
    title: String,
    #[serde(default)]
    updated: u64,
    #[serde(default)]
    project: Option<String>,
}

pub fn dir() -> PathBuf {
    app_dir().join("chats")
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A new unique id: milliseconds since the Unix epoch.
pub fn new_id() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
        .to_string()
}

/// Title = first line of the first user message, shortened.
pub fn title_from(messages: &[ChatMessage]) -> String {
    let first = messages
        .iter()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.trim())
        .unwrap_or("");
    let line = first.lines().next().unwrap_or("");
    if line.is_empty() {
        return "New chat".to_string();
    }

    let mut title: String = line.chars().take(40).collect();
    if line.chars().count() > 40 {
        title.push_str("...");
    }
    title
}

/// Ids become file names, so only allow safe characters.
fn path_for(id: &str) -> io::Result<PathBuf> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid chat id",
        ));
    }
    Ok(dir().join(format!("{id}.json")))
}

/// Save a conversation. Writes to a temp file first, then renames it, so a
/// crash mid-write cannot corrupt an existing chat.
pub fn save(chat: &Chat) -> io::Result<()> {
    let path = path_for(&chat.id)?;
    fs::create_dir_all(dir())?;

    let json = serde_json::to_string_pretty(chat).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json)?;
    fs::rename(tmp, path)
}

pub fn load(id: &str) -> io::Result<Chat> {
    let text = fs::read_to_string(path_for(id)?)?;
    serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn delete(id: &str) -> io::Result<()> {
    fs::remove_file(path_for(id)?)
}

/// All saved conversations, most recently updated first.
pub fn list() -> Vec<ChatSummary> {
    let Ok(entries) = fs::read_dir(dir()) else {
        return Vec::new();
    };

    let mut chats: Vec<ChatSummary> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let text = fs::read_to_string(e.path()).ok()?;
            let header: Header = serde_json::from_str(&text).ok()?;
            Some(ChatSummary {
                id: header.id,
                title: header.title,
                updated: header.updated,
                project: header.project,
            })
        })
        .collect();

    chats.sort_by(|a, b| b.updated.cmp(&a.updated));
    chats
}
/// Detach every conversation from a project (used when the project is deleted).
pub fn detach_project(slug: &str) {
    let Ok(entries) = fs::read_dir(dir()) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.extension().is_some_and(|x| x == "json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut chat) = serde_json::from_str::<Chat>(&text) else {
            continue;
        };
        if chat.project.as_deref() == Some(slug) {
            chat.project = None;
            let _ = save(&chat);
        }
    }
}
