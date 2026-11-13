//! Actions proposed by the model (file edits, deletes, shell commands), applying
//! them to disk with verification, and undoing them.

use std::fs;
use std::io::Write;
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
    Delete(String),
    Run(String),
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// An opening fence: 3 or more backticks followed by an info string.
fn open_fence(line: &str) -> Option<(usize, &str)> {
    let t = line.trim();
    let n = t.chars().take_while(|c| *c == '`').count();
    (n >= 3).then(|| (n, t[n..].trim()))
}

/// A closing fence: only backticks, at least as many as the opening fence.
fn is_close(line: &str, n: usize) -> bool {
    let t = line.trim();
    t.len() >= n && t.chars().all(|c| c == '`')
}

fn parse(reply: &str, require_closed: bool) -> Vec<Action> {
    let mut actions = Vec::new();
    let mut lines = reply.lines();

    while let Some(line) = lines.next() {
        let Some((n, info)) = open_fence(line) else { continue };

        let mut body: Vec<&str> = Vec::new();
        let mut closed = false;
        for l in lines.by_ref() {
            if is_close(l, n) {
                closed = true;
                break;
            }
            body.push(l);
        }
        // While streaming, a block that is still being written must not be applied.
        if require_closed && !closed {
            break;
        }

        if let Some(path) = info.strip_prefix("edit ") {
            let hunks = parse_hunks(&body);
            if !hunks.is_empty() {
                actions.push(Action::Edit { path: path.trim().to_string(), hunks });
            }
        } else if let Some(path) = info.strip_prefix("delete ") {
            if !path.trim().is_empty() {
                actions.push(Action::Delete(path.trim().to_string()));
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

/// All actions in a finished reply (a truncated last block is still read, but
/// only hunks that were fully written are returned).
pub fn parse_actions(reply: &str) -> Vec<Action> {
    parse(reply, false)
}

/// Only actions whose fenced block is already closed: safe to apply while the
/// reply is still streaming. Always a prefix of `parse_actions` on the same text.
pub fn completed_actions(partial: &str) -> Vec<Action> {
    parse(partial, true)
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

fn clean(rel: &str) -> &str {
    rel.trim().trim_matches('`').trim_start_matches("./")
}

/// Only plain relative paths inside the project (no `..`, no absolute, no `.git`).
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = clean(rel);
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

/// The real (symlink-resolved) parent of `path` must be inside the real root.
fn confined(root: &Path, path: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|e| format!("project folder: {e}"))?;
    let parent = path
        .parent()
        .ok_or("invalid path")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if parent.starts_with(&root) {
        Ok(())
    } else {
        Err("path leaves the project folder".into())
    }
}

/// Write to a temp file in the same folder, flush it to disk, then rename over
/// the target. A crash can never leave a half-written file behind.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent folder"))?;
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?
        .to_string_lossy();
    let tmp = dir.join(format!(".{name}.virgo-tmp"));

    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        if let Ok(meta) = fs::metadata(path) {
            let _ = fs::set_permissions(&tmp, meta.permissions());
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
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

/// Apply edits to one file and return a change only after the content has been
/// read back from disk and matches what was written.
pub fn apply_edit(root: &Path, rel: &str, hunks: &[Hunk]) -> Result<FileChange, String> {
    let path = safe_join(root, rel)?;
    let rel = clean(rel).to_string();

    let before = if path.exists() {
        Some(fs::read_to_string(&path).map_err(|_| format!("{rel} is not a UTF-8 text file"))?)
    } else {
        None
    };
    let after = apply_hunks(before.as_deref().unwrap_or(""), hunks)
        .map_err(|e| format!("{rel}: {e}"))?;
    if before.as_deref() == Some(after.as_str()) {
        return Err(format!("{rel}: the edit changes nothing"));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{rel}: {e}"))?;
    }
    confined(root, &path).map_err(|e| format!("{rel}: {e}"))?;
    write_atomic(&path, &after).map_err(|e| format!("could not write {rel}: {e}"))?;

    let on_disk = fs::read_to_string(&path)
        .map_err(|e| format!("{rel}: written but cannot be read back: {e}"))?;
    if on_disk != after {
        return Err(format!("{rel}: verification failed, the file on disk differs"));
    }
    Ok(FileChange { rel, before, after: Some(after) })
}

/// Delete one text file; reports success only once it is really gone.
pub fn apply_delete(root: &Path, rel: &str) -> Result<FileChange, String> {
    let path = safe_join(root, rel)?;
    let rel = clean(rel).to_string();
    if !path.is_file() {
        return Err(format!("{rel}: no such file"));
    }
    confined(root, &path).map_err(|e| format!("{rel}: {e}"))?;
    let before =
        fs::read_to_string(&path).map_err(|_| format!("{rel} is not a UTF-8 text file"))?;
    fs::remove_file(&path).map_err(|e| format!("could not delete {rel}: {e}"))?;
    if path.exists() {
        return Err(format!("{rel}: still exists after delete"));
    }
    Ok(FileChange { rel, before: Some(before), after: None })
}

// ---------------------------------------------------------------------------
// Undo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FileChange {
    pub rel: String,
    /// `None` means the file did not exist before.
    pub before: Option<String>,
    /// `None` means the file was deleted.
    pub after: Option<String>,
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
            if fs::read_to_string(&path).ok() != file.after {
                skipped.push(file.rel.clone());
                continue;
            }
            let result = match &file.before {
                Some(old) => write_atomic(&path, old),
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("virgo-ws-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn hunk(s: &str, r: &str) -> Hunk {
        Hunk { search: s.into(), replace: r.into() }
    }

    #[test]
    fn modify_existing_file_and_read_back() {
        let root = tmp("modify");
        fs::write(root.join("a.rs"), "fn a() {}\nfn b() {}\n").unwrap();
        let change = apply_edit(&root, "a.rs", &[hunk("fn a() {}", "fn a() { 1 }")]).unwrap();
        assert_eq!(fs::read_to_string(root.join("a.rs")).unwrap(), "fn a() { 1 }\nfn b() {}\n");
        assert_eq!(change.before.as_deref(), Some("fn a() {}\nfn b() {}\n"));
        assert!(!root.join(".a.rs.virgo-tmp").exists());
    }

    #[test]
    fn create_file_in_new_folder() {
        let root = tmp("create");
        apply_edit(&root, "src/auth.rs", &[hunk("", "pub fn auth() {}")]).unwrap();
        assert_eq!(fs::read_to_string(root.join("src/auth.rs")).unwrap(), "pub fn auth() {}\n");
    }

    #[test]
    fn multiple_files_persist_progressively() {
        let root = tmp("multi");
        fs::write(root.join("x"), "1").unwrap();
        fs::write(root.join("y"), "2").unwrap();
        apply_edit(&root, "x", &[hunk("1", "one")]).unwrap();
        assert_eq!(fs::read_to_string(root.join("x")).unwrap(), "one"); // visible before y is touched
        assert_eq!(fs::read_to_string(root.join("y")).unwrap(), "2");
        apply_edit(&root, "y", &[hunk("2", "two")]).unwrap();
        assert_eq!(fs::read_to_string(root.join("y")).unwrap(), "two");
    }

    #[test]
    fn delete_removes_only_that_file_and_can_be_reverted() {
        let root = tmp("delete");
        fs::write(root.join("gone.txt"), "bye").unwrap();
        fs::write(root.join("stay.txt"), "hi").unwrap();
        let change = apply_delete(&root, "gone.txt").unwrap();
        assert!(!root.join("gone.txt").exists());
        assert!(root.join("stay.txt").exists());
        let mut set = ChangeSet::new();
        set.record(change);
        let (restored, skipped) = set.revert(&root);
        assert_eq!((restored.len(), skipped.len()), (1, 0));
        assert_eq!(fs::read_to_string(root.join("gone.txt")).unwrap(), "bye");
    }

    #[test]
    fn invalid_paths_are_rejected() {
        let root = tmp("paths");
        for bad in ["../x", "/etc/passwd", ".git/config", "", "a/../../b"] {
            assert!(apply_edit(&root, bad, &[hunk("", "x")]).is_err(), "{bad}");
        }
    }

    #[test]
    fn failed_write_is_an_error_not_a_success() {
        let root = tmp("fail");
        fs::write(root.join("file"), "x").unwrap();
        // "file" is a regular file, so "file/inner.txt" cannot be created.
        assert!(apply_edit(&root, "file/inner.txt", &[hunk("", "y")]).is_err());
        assert_eq!(fs::read_to_string(root.join("file")).unwrap(), "x");
    }

    #[test]
    fn noop_edit_is_not_reported_as_modified() {
        let root = tmp("noop");
        fs::write(root.join("a"), "same").unwrap();
        assert!(apply_edit(&root, "a", &[hunk("same", "same")]).is_err());
    }

    #[test]
    fn missing_search_text_fails_and_leaves_file_alone() {
        let root = tmp("nosearch");
        fs::write(root.join("a"), "real").unwrap();
        assert!(apply_edit(&root, "a", &[hunk("not there", "x")]).is_err());
        assert_eq!(fs::read_to_string(root.join("a")).unwrap(), "real");
    }

    #[test]
    fn two_projects_stay_isolated() {
        let (a, b) = (tmp("iso-a"), tmp("iso-b"));
        fs::write(a.join("f"), "A").unwrap();
        fs::write(b.join("f"), "B").unwrap();
        apply_edit(&a, "f", &[hunk("A", "A2")]).unwrap();
        assert_eq!(fs::read_to_string(a.join("f")).unwrap(), "A2");
        assert_eq!(fs::read_to_string(b.join("f")).unwrap(), "B");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_cannot_escape_the_project() {
        let (root, outside) = (tmp("sym-root"), tmp("sym-out"));
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        assert!(apply_edit(&root, "link/evil.txt", &[hunk("", "x")]).is_err());
        assert!(!outside.join("evil.txt").exists());
    }

    #[test]
    fn four_backtick_fences_are_parsed() {
        let reply = "Sure.\n````edit a.rs\n<<<<<<< SEARCH\nx\n=======\ny\n>>>>>>> REPLACE\n````\n";
        assert_eq!(
            parse_actions(reply),
            vec![Action::Edit { path: "a.rs".into(), hunks: vec![hunk("x", "y")] }]
        );
    }

    #[test]
    fn streaming_only_returns_closed_blocks() {
        let one = "```edit a.rs\n<<<<<<< SEARCH\nx\n=======\ny\n>>>>>>> REPLACE\n```\n";
        let partial = format!("{one}```edit b.rs\n<<<<<<< SEARCH\nq\n=======\nr\n>>>>>>> REPLACE\n");
        assert_eq!(completed_actions(&partial).len(), 1);
        let done = format!("{partial}```\n");
        assert_eq!(completed_actions(&done).len(), 2);
        assert_eq!(parse_actions(&done).len(), 2);
    }

    #[test]
    fn delete_and_run_blocks_are_parsed() {
        let reply = "```delete old.rs\n```\n```run\ncargo check\n```\n";
        assert_eq!(
            parse_actions(reply),
            vec![Action::Delete("old.rs".into()), Action::Run("cargo check".into())]
        );
    }
}