//! Projects: one folder per project under `<app dir>/projects/<slug>/`.
//! Each conversation is saved there as a markdown note, so the folder works
//! as the long-term memory of that project. `project.md` describes the project.

use std::cmp::Reverse;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::agent::Role;
use crate::chats::Chat;
use crate::config::app_dir;
use crate::memory::store::{today, MemoryStore, Note};

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

/// All projects, sorted by name. The display name is the `# Heading` of project.md.
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
            let name = fs::read_to_string(e.path().join("project.md"))
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

/// Create the folder and a starter project.md. Returns the slug.
pub fn create(name: &str) -> io::Result<String> {
    let name = name.trim();
    let slug = slugify(name)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "enter a project name"))?;
    let path = dir(&slug)?;
    if path.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "that project already exists"));
    }
    fs::create_dir_all(&path)?;
    fs::write(
        path.join("project.md"),
        format!(
            "# {name}\n\n## Description\n- What this project is (this note is always sent to the models).\n\n## Features\n- \n\n## Decisions\n- \n"
        ),
    )?;
    Ok(slug)
}

pub fn delete(slug: &str) -> io::Result<()> {
    fs::remove_dir_all(dir(slug)?)
}

/// Note names in a project: project.md first, then conversations, newest first.
pub fn notes(slug: &str) -> Vec<String> {
    let mut names = store(slug).map(|s| s.list()).unwrap_or_default();
    names.sort_by_key(|n| {
        (
            n != "project.md",
            Reverse(chat_id(n).and_then(|id| id.parse::<u128>().ok())),
        )
    });
    names
}

/// "clear-button-1731100000000.md" -> "1731100000000"
pub fn chat_id(note: &str) -> Option<String> {
    let stem = note.strip_suffix(".md")?;
    let (_, id) = stem.rsplit_once('-')?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_digit())).then(|| id.to_string())
}

fn chat_file(title: &str, id: &str) -> String {
    let slug = slugify(title).unwrap_or_else(|| "chat".into());
    let short: String = slug.chars().take(40).collect();
    format!("{}-{id}.md", short.trim_matches('-'))
}

/// Write (or rewrite) the conversation as a markdown note in the project folder.
pub fn save_chat(slug: &str, chat: &Chat) -> io::Result<()> {
    let path = dir(slug)?;
    if !path.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "project folder is gone"));
    }
    MemoryStore::new(path).write(&chat_file(&chat.title, &chat.id), &chat_markdown(chat))?;
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

/// Keep `## You / Plan / Virgo` as the only level-2 headings, so each turn is one section.
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

/// The project's notes as memory notes (named `project/<file>`), for retrieval.
/// The conversation being written right now is left out: it is already in the history.
pub fn load_notes(slug: &str, exclude_chat: Option<&str>) -> Vec<Note> {
    let Ok(store) = store(slug) else {
        return Vec::new();
    };
    store
        .load_all()
        .into_iter()
        .filter(|n| match (exclude_chat, chat_id(&n.name)) {
            (Some(current), Some(id)) => id != current,
            _ => true,
        })
        .map(|n| Note { name: format!("project/{}", n.name), content: n.content })
        .collect()
}