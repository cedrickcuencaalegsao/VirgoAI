//! Actions proposed by the model (file edits, shell commands), applying them
//! to disk, and undoing them.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq)]
pub struct Hunk {
    pub search: String,
    pub replace: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Edit { path: String, hunks: Vec<Hunk> },
    Run(String),
}

/// Find ```edit <path> and ```run blocks in a finished reply.
pub fn parse_actions(reply: &str) -> Vec<Action> {
    let mut actions = Vec::new();
    let mut lines = reply.lines();

    while let Some(line) = lines.next() {
        let Some(info) = line.trim().strip_prefix("```") else { continue };
        let info = info.trim();

        let mut body: Vec<&str> = Vec::new();
        for l in lines.by_ref() {
            if l.trim() == "```" {
                break;
            }
            body.push(l);
        }

        if let Some(path) = info.strip_prefix("edit ") {
            let hunks = parse_hunks(&body);
            if !hunks.is_empty() {
                actions.push(Action::Edit { path: path.trim().to_string(), hunks });
            }
        } else if info == "run" {
            let command = body.join("\n").trim().to_string();
            if !command.is_empty() {
                actions.push(Action::Run(command));
            }
        }
    }
    actions
}

fn parse_hunks(body: &[&str]) -> Vec<Hunk> {
    #[derive(PartialEq)]
    enum State {
        Out,
        Search,
        Replace,
    }
    let mut state = State::Out;
    let (mut search, mut replace): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let mut hunks = Vec::new();

    for line in body {
        let t = line.trim_end();
        match state {
            State::Out => {
                if t.starts_with("<<<<<<<") {
                    state = State::Search;
                    search.clear();
                    replace.clear();
                }
            }
            State::Search => {
                if t.starts_with("=======") {
                    state = State::Replace;
                } else {
                    search.push(line);
                }
            }
            State::Replace => {
                if t.starts_with(">>>>>>>") {
                    hunks.push(Hunk { search: search.join("\n"), replace: replace.join("\n") });
                    state = State::Out;
                } else {
                    replace.push(line);
                }
            }
        }
    }
    hunks
}

// ---------------------------------------------------------------------------
// Applying
// ---------------------------------------------------------------------------

/// Only plain relative paths inside the project (no `..`, no absolute, no `.git`).
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim().trim_matches('`').trim_start_matches("./");
    let path = Path::new(rel);
    let plain = path.components().all(|c| matches!(c, Component::Normal(_)));
    if rel.is_empty() || !plain {
        return Err(format!("unsafe path: {rel}"));
    }
    if path.components().any(|c| c.as_os_str() == ".git") {
        return Err("editing .git is not allowed".into());
    }
    Ok(root.join(path))
}

pub fn apply_hunks(original: &str, hunks: &[Hunk]) -> Result<String, String> {
    let crlf = original.contains("\r\n");
    let mut text = original.replace("\r\n", "\n");
    for (i, hunk) in hunks.iter().enumerate() {
        text = replace_once(&text, hunk).map_err(|e| format!("block {}: {e}", i + 1))?;
    }
    Ok(if crlf { text.replace('\n', "\r\n") } else { text })
}

fn replace_once(text: &str, hunk: &Hunk) -> Result<String, String> {
    // Empty SEARCH = create a new file.
    if hunk.search.trim().is_empty() {
        if !text.trim().is_empty() {
            return Err("empty SEARCH is only allowed for new files".into());
        }
        let mut new = hunk.replace.clone();
        if !new.ends_with('\n') {
            new.push('\n');
        }
        return Ok(new);
    }

    // 1) exact match
    match text.matches(&hunk.search).count() {
        1 => return Ok(text.replacen(&hunk.search, &hunk.replace, 1)),
        n if n > 1 => {
            return Err("SEARCH matches more than once; include more surrounding lines".into())
        }
        _ => {}
    }

    // 2) whitespace-tolerant, line by line
    let t_lines: Vec<&str> = text.split('\n').collect();
    let s_lines: Vec<&str> = hunk.search.split('\n').collect();
    if s_lines.len() > t_lines.len() {
        return Err("SEARCH text not found".into());
    }
    let hits: Vec<usize> = (0..=t_lines.len() - s_lines.len())
        .filter(|&i| s_lines.iter().enumerate().all(|(k, s)| t_lines[i + k].trim() == s.trim()))
        .collect();

    match hits.as_slice() {
        [i] => {
            let mut out: Vec<&str> = t_lines[..*i].to_vec();
            if !hunk.replace.is_empty() {
                out.extend(hunk.replace.split('\n'));
            }
            out.extend(&t_lines[i + s_lines.len()..]);
            Ok(out.join("\n"))
        }
        [] => Err("SEARCH text not found in the file".into()),
        _ => Err("SEARCH matches more than once; include more surrounding lines".into()),
    }
}

pub fn apply_edit(root: &Path, rel: &str, hunks: &[Hunk]) -> Result<FileChange, String> {
    let path = safe_join(root, rel)?;
    let before = if path.exists() {
        Some(fs::read_to_string(&path).map_err(|_| format!("{rel} is not a UTF-8 text file"))?)
    } else {
        None
    };
    let after = apply_hunks(before.as_deref().unwrap_or(""), hunks)
        .map_err(|e| format!("{rel}: {e}"))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, &after).map_err(|e| format!("could not write {rel}: {e}"))?;

    Ok(FileChange { rel: rel.trim().trim_start_matches("./").to_string(), before, after })
}

// ---------------------------------------------------------------------------
// Undo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FileChange {
    pub rel: String,
    /// `None` means the file did not exist before.
    pub before: Option<String>,
    pub after: String,
}

#[derive(Debug, Clone)]
pub struct ChangeSet {
    pub id: u64,
    pub files: Vec<FileChange>,
    pub reverted: bool,
}

impl ChangeSet {
    pub fn new() -> Self {
        Self { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), files: Vec::new(), reverted: false }
    }

    /// Keep the first `before` and the latest `after` for each file.
    pub fn record(&mut self, change: FileChange) {
        match self.files.iter_mut().find(|f| f.rel == change.rel) {
            Some(existing) => existing.after = change.after,
            None => self.files.push(change),
        }
    }

    /// Restore the files. Returns (restored, skipped). A file edited by hand
    /// since the AI change is skipped, so your own work is never overwritten.
    pub fn revert(&self, root: &Path) -> (Vec<String>, Vec<String>) {
        let (mut restored, mut skipped) = (Vec::new(), Vec::new());
        for file in self.files.iter().rev() {
            let Ok(path) = safe_join(root, &file.rel) else {
                skipped.push(file.rel.clone());
                continue;
            };
            if fs::read_to_string(&path).ok().as_deref() != Some(file.after.as_str()) {
                skipped.push(file.rel.clone());
                continue;
            }
            let result = match &file.before {
                Some(old) => fs::write(&path, old),
                None => fs::remove_file(&path),
            };
            if result.is_ok() {
                restored.push(file.rel.clone());
            } else {
                skipped.push(file.rel.clone());
            }
        }
        (restored, skipped)
    }
}