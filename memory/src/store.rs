use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

/// A markdown note loaded from disk.
#[derive(Debug, Clone)]
pub struct Note {
    /// Path relative to the memory folder, e.g. `profile.md` or `notes/rust.md`.
    pub name: String,
    pub content: String,
}

/// Markdown files in one folder, used as the app's long-term memory.
#[derive(Debug, Clone)]
pub struct MemoryStore {
    dir: PathBuf,
}

impl MemoryStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Create the folder, and seed a `profile.md` the first time it is empty.
    pub fn ensure(&self) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        if self.list().is_empty() {
            fs::write(
                self.dir.join("profile.md"),
                "# Profile\n\n- Add facts about yourself here. This note is always sent to the model.\n",
            )?;
        }
        Ok(())
    }

    /// Relative names of all `.md` files (up to 3 folders deep), sorted.
    pub fn list(&self) -> Vec<String> {
        let mut names: Vec<String> = WalkDir::new(&self.dir)
            .max_depth(3)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "md")
            })
            .filter_map(|e| {
                e.path()
                    .strip_prefix(&self.dir)
                    .ok()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
            })
            .collect();
        names.sort();
        names
    }

    pub fn read(&self, name: &str) -> io::Result<String> {
        let (_, path) = self.resolve(name)?;
        fs::read_to_string(path)
    }

    /// Create or overwrite a note. Returns the normalized name.
    pub fn write(&self, name: &str, content: &str) -> io::Result<String> {
        let (rel, path) = self.resolve(name)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, content)?;
        Ok(rel)
    }

    /// Append a line (or block) to a note, creating it if needed.
    pub fn append(&self, name: &str, text: &str) -> io::Result<String> {
        let (rel, path) = self.resolve(name)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{text}")?;
        Ok(rel)
    }

    pub fn delete(&self, name: &str) -> io::Result<()> {
        let (_, path) = self.resolve(name)?;
        fs::remove_file(path)
    }

    /// Read every note (used by retrieval).
    pub fn load_all(&self) -> Vec<Note> {
        self.list()
            .into_iter()
            .filter_map(|name| {
                let content = self.read(&name).ok()?;
                Some(Note { name, content })
            })
            .collect()
    }

    fn resolve(&self, name: &str) -> io::Result<(String, PathBuf)> {
        let rel = normalize_name(name).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid note name")
        })?;
        let path = self.dir.join(&rel);
        Ok((rel, path))
    }
}

/// Turn user input into a safe relative note path ending in `.md`.
/// Rejects `.` and `..` segments so notes cannot escape the memory folder.
pub fn normalize_name(input: &str) -> Option<String> {
    let cleaned_input = input.trim().replace('\\', "/");
    let mut parts: Vec<String> = Vec::new();

    for segment in cleaned_input.split('/') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if segment == "." || segment == ".." {
            return None;
        }
        let clean: String = segment
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        parts.push(clean);
    }

    let last = parts.last_mut()?;
    if !last.to_lowercase().ends_with(".md") {
        last.push_str(".md");
    }
    Some(parts.join("/"))
}

/// Today's date (UTC) as `YYYY-MM-DD`, without pulling in a date crate.
pub fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Civil-from-days algorithm (Howard Hinnant).
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };

    format!("{y:04}-{m:02}-{d:02}")
}
