//! Projects: one folder per project under `<app dir>/projects/<slug>/`.
//!
//! ```text
//! <slug>/
//!   project.md        description + decisions (knowledge, always sent)
//!   instructions.md   project instructions (always sent as instructions)
//!   memory/*.md       reusable knowledge notes
//!   conversations/    readable markdown copies of this project's chats
//!   folder.txt        optional linked code folder
//! ```
//! The slug is the stable project id. The display name is the `# Heading` of project.md.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::agent::Role;
use crate::chats::Chat;
use crate::config::app_dir;
use crate::memory::store::{today, MemoryStore, Note};

pub const INSTRUCTIONS: &str = "instructions.md";
pub const PROJECT_NOTE: &str = "project.md";
const FOLDER_FILE: &str = "folder.txt";

#[derive(Debug, Clone)]
pub struct Workspace {
    pub slug: String,
    pub name: String,
}

pub fn root() -> PathBuf {
    app_dir().join("projects")
}

fn valid(slug: &str) -> bool {
    !slug.is_empty()
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn dir(slug: &str) -> io::Result<PathBuf> {
    if valid(slug) {
        Ok(root().join(slug))
    } else {
        Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid project name"))
    }
}

pub fn store(slug: &str) -> io::Result<MemoryStore> {
    Ok(MemoryStore::new(dir(slug)?))
}

pub fn exists(slug: &str) -> bool {
    dir(slug).map(|d| d.is_dir()).unwrap_or(false)
}

/// "My App!" -> "my-app"
pub fn slugify(name: &str) -> Option<String> {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    (!out.is_empty()).then_some(out)
}

/// All projects, sorted by name.
pub fn list() -> Vec<Workspace> {
    let Ok(entries) = fs::read_dir(root()) else {
        return Vec::new();
    };
    let mut out: Vec<Workspace> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let slug = e.file_name().to_string_lossy().to_string();
            if !valid(&slug) {
                return None;
            }
            let name = fs::read_to_string(e.path().join(PROJECT_NOTE))
                .ok()
                .and_then(|t| {
                    t.lines()
                        .find_map(|l| l.strip_prefix("# ").map(|s| s.trim().to_string()))
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| slug.clone());
            Some(Workspace { slug, name })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Create the project folder with project.md, an empty instructions.md and memory/.
pub fn create(name: &str) -> io::Result<String> {
    let name = name.trim();
    let slug = slugify(name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "enter a project name"))?;
    let path = dir(&slug)?;
    if path.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "that project already exists"));
    }
    fs::create_dir_all(path.join("memory"))?;
    fs::write(
        path.join(PROJECT_NOTE),
        format!(
            "# {name}\n\n## Description\n- What this project is.\n\n## Decisions\n- \n"
        ),
    )?;
    fs::write(path.join(INSTRUCTIONS), "")?;
    Ok(slug)
}

pub fn delete(slug: &str) -> io::Result<()> {
    fs::remove_dir_all(dir(slug)?)
}

// ---------------------------------------------------------------------------
// Instructions
// ---------------------------------------------------------------------------

/// Project instructions (empty string when there are none).
pub fn instructions(slug: &str) -> String {
    dir(slug)
        .ok()
        .and_then(|d| fs::read_to_string(d.join(INSTRUCTIONS)).ok())
        .unwrap_or_default()
}

pub fn save_instructions(slug: &str, text: &str) -> io::Result<()> {
    fs::write(dir(slug)?.join(INSTRUCTIONS), text)
}

// ---------------------------------------------------------------------------
// Linked folder
// ---------------------------------------------------------------------------

pub fn linked_folder(slug: &str) -> Option<String> {
    let text = fs::read_to_string(dir(slug).ok()?.join(FOLDER_FILE)).ok()?;
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

pub fn link_folder(slug: &str, path: &str) -> io::Result<()> {
    fs::write(dir(slug)?.join(FOLDER_FILE), path)
}

// ---------------------------------------------------------------------------
// Knowledge (markdown notes)
// ---------------------------------------------------------------------------

/// "clear-button-1731100000000.md" -> "1731100000000"
pub fn chat_id(note: &str) -> Option<String> {
    let stem = note.strip_suffix(".md")?;
    let (_, id) = stem.rsplit_once('-')?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_digit())).then(|| id.to_string())
}

/// Knowledge = project.md, memory/*.md and any other hand-made note.
/// Not knowledge: instructions.md, the conversations/ copies, and old chat
/// notes that earlier versions wrote at the project root.
fn is_knowledge(name: &str) -> bool {
    if name == INSTRUCTIONS || name.starts_with("conversations/") {
        return false;
    }
    !(!name.contains('/') && chat_id(name).is_some())
}

/// Knowledge note names: project.md first, then the rest alphabetically.
pub fn notes(slug: &str) -> Vec<String> {
    let mut names: Vec<String> = store(slug)
        .map(|s| s.list())
        .unwrap_or_default()
        .into_iter()
        .filter(|n| is_knowledge(n))
        .collect();
    names.sort_by_key(|n| (n != PROJECT_NOTE, n.clone()));
    names
}

/// The project's knowledge as memory notes (named `project/<file>`), for retrieval.
/// Only this project's knowledge is returned: never conversations, never other projects.
pub fn load_notes(slug: &str) -> Vec<Note> {
    let Ok(store) = store(slug) else {
        return Vec::new();
    };
    store
        .load_all()
        .into_iter()
        .filter(|n| is_knowledge(&n.name))
        .map(|n| Note { name: format!("project/{}", n.name), content: n.content })
        .collect()
}

/// Explicit memory workflow: append text to `memory/inbox.md` of the project.
pub fn append_memory(slug: &str, text: &str) -> io::Result<String> {
    store(slug)?.append("memory/inbox.md", text)
}

// ---------------------------------------------------------------------------
// Conversation copies
// ---------------------------------------------------------------------------

fn chat_file(title: &str, id: &str) -> String {
    let slug = slugify(title).unwrap_or_else(|| "chat".into());
    let short: String = slug.chars().take(40).collect();
    format!("{}-{id}.md", short.trim_matches('-'))
}

/// Write (or rewrite) the conversation as a markdown copy in `conversations/`.
pub fn save_chat(slug: &str, chat: &Chat) -> io::Result<()> {
    let path = dir(slug)?;
    if !path.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "project folder is gone"));
    }
    let name = format!("conversations/{}", chat_file(&chat.title, &chat.id));
    MemoryStore::new(path).write(&name, &chat_markdown(chat))?;
    Ok(())
}

pub fn chat_markdown(chat: &Chat) -> String {
    let mut out = format!(
        "# {}\n\n- Updated: {}\n- Chat id: {}\n",
        chat.title,
        today(),
        chat.id
    );
    for m in &chat.messages {
        let label = match m.role {
            Role::User => "You",
            Role::Plan => "Plan",
            Role::Assistant => "Virgo",
            Role::Error => continue,
        };
        let body = m.content.trim();
        if body.is_empty() {
            continue;
        }
        out.push_str(&format!("\n## {label}\n"));
        if m.role == Role::User && !m.files.is_empty() {
            out.push_str(&format!("Files: {}\n\n", m.files.join(", ")));
        }
        out.push_str(&demote_headings(body));
        out.push('\n');
    }
    out
}

/// Keep `## You / Plan / Virgo` as the only level-2 headings.
fn demote_headings(body: &str) -> String {
    let mut in_code = false;
    let mut out: Vec<String> = Vec::new();
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        if !in_code && line.starts_with('#') {
            out.push(format!("#### {}", line.trim_start_matches('#').trim()));
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}