//! The file open in the code panel, plus the AI line edits waiting to be applied.
//!
//! Edits are kept separate from the file: each one replaces a single original
//! line with zero or more new lines. Nothing touches the disk until `apply()`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use project::lang_for;

/// Files bigger than this are not opened in the code panel.
const MAX_BYTES: u64 = 1_000_000;
/// Long lines are cut off in the view (the file itself is never changed by this).
const MAX_DISPLAY_CHARS: usize = 300;
/// Approximate width of one monospace character at the code font size.
const CHAR_W: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Normal,
    /// New text from the model (green).
    Added,
    /// Original line that a suggestion replaces or deletes (red).
    Removed,
}

/// One displayed row. A line with an edit shows as one red row (the original)
/// followed by green rows (the replacement).
pub struct RowRef<'a> {
    pub kind: RowKind,
    /// Index of the original line this row belongs to (0-based).
    pub line: usize,
    /// Line number to show in the gutter (original numbering).
    pub number: Option<usize>,
    /// Whether to show the suggest / revert buttons on this row.
    pub show_actions: bool,
    pub src: &'a str,
}

pub struct CodeDoc {
    pub rel: String,
    pub path: PathBuf,
    pub lines: Vec<String>,
    /// Pending AI edits: original line index -> replacement lines (empty = delete).
    pub edits: BTreeMap<usize, Vec<String>>,
    /// File content before the last apply, so it can be undone once.
    pub previous: Option<String>,
    original_text: String,
    crlf: bool,
    trailing_newline: bool,
    max_chars: usize,
}

impl CodeDoc {
    pub fn open(root: &Path, rel: &str) -> Result<Self, String> {
        if rel.split('/').any(|part| part == "..") {
            return Err("invalid path".into());
        }
        let path = root.join(rel);
        let meta = fs::metadata(&path).map_err(|e| format!("{rel}: {e}"))?;
        if meta.len() > MAX_BYTES {
            return Err(format!("{rel} is larger than 1 MB, so it is not opened here"));
        }
        let text = fs::read_to_string(&path)
            .map_err(|_| format!("{rel} is not a UTF-8 text file"))?;
        Ok(Self::from_text(rel.to_string(), path, text))
    }

    fn from_text(rel: String, path: PathBuf, text: String) -> Self {
        let mut doc = Self {
            rel,
            path,
            lines: text.lines().map(String::from).collect(),
            edits: BTreeMap::new(),
            previous: None,
            crlf: text.contains("\r\n"),
            trailing_newline: text.ends_with('\n'),
            original_text: text,
            max_chars: 0,
        };
        doc.recompute_width();
        doc
    }

    /// Re-read the file from disk. Pending edits are dropped.
    pub fn reload(&mut self) -> Result<(), String> {
        let text = fs::read_to_string(&self.path)
            .map_err(|e| format!("Could not reload {}: {e}", self.rel))?;
        *self = Self::from_text(self.rel.clone(), self.path.clone(), text);
        Ok(())
    }

    // ---- edits -----------------------------------------------------------

        /// Turn a SEARCH/REPLACE block into pending line edits. The first matched
    /// line gets the replacement, the other matched lines are deleted.
    pub fn stage(&mut self, search: &str, replace: &str) -> Result<(), String> {
        fn end(s: &str) -> &str {
            s.trim_end()
        }
        fn both(s: &str) -> &str {
            s.trim()
        }

        let needle: Vec<&str> = search.lines().collect();
        if needle.is_empty() || needle.iter().all(|l| l.trim().is_empty()) {
            return Err("empty search block".into());
        }
        if needle.len() > self.lines.len() {
            return Err("search block is longer than the file".into());
        }

        let find = |norm: fn(&str) -> &str| -> Vec<usize> {
            (0..=self.lines.len() - needle.len())
                .filter(|&i| {
                    needle
                        .iter()
                        .enumerate()
                        .all(|(k, n)| norm(&self.lines[i + k]) == norm(n))
                })
                .collect()
        };

        // Exact (ignoring trailing spaces) first, then ignoring indentation.
        let mut hits = find(end);
        if hits.is_empty() {
            hits = find(both);
        }

        match hits.as_slice() {
            [] => Err("search block not found".into()),
            [start] => {
                let start = *start;
                let replacement: Vec<String> = replace.lines().map(String::from).collect();
                self.set_edit(start, replacement);
                for i in start + 1..start + needle.len() {
                    self.set_edit(i, Vec::new());
                }
                Ok(())
            }
            _ => Err("search block matches more than once".into()),
        }
    }

    pub fn set_edit(&mut self, line: usize, replacement: Vec<String>) {
        if line < self.lines.len() {
            self.edits.insert(line, replacement);
            self.recompute_width();
        }
    }

    pub fn clear_edit(&mut self, line: usize) {
        self.edits.remove(&line);
        self.recompute_width();
    }

    pub fn clear_all(&mut self) {
        self.edits.clear();
        self.recompute_width();
    }

    /// Write all pending edits to the file. Returns how many were applied.
    pub fn apply(&mut self) -> Result<usize, String> {
        if self.edits.is_empty() {
            return Ok(0);
        }
        self.ensure_unchanged_on_disk()?;

        let mut out: Vec<String> = Vec::with_capacity(self.lines.len());
        for (index, line) in self.lines.iter().enumerate() {
            match self.edits.get(&index) {
                None => out.push(line.clone()),
                Some(replacement) => out.extend(replacement.iter().cloned()),
            }
        }

        let new_text = self.join(&out);
        fs::write(&self.path, &new_text)
            .map_err(|e| format!("Could not write {}: {e}", self.rel))?;

        let count = self.edits.len();
        self.previous = Some(std::mem::replace(&mut self.original_text, new_text));
        self.lines = out;
        self.edits.clear();
        self.recompute_width();
        Ok(count)
    }

    /// Put the file back to how it was before the last apply.
    pub fn undo_apply(&mut self) -> Result<(), String> {
        let Some(previous) = self.previous.clone() else {
            return Err("Nothing to undo".into());
        };
        self.ensure_unchanged_on_disk()?;
        fs::write(&self.path, &previous)
            .map_err(|e| format!("Could not write {}: {e}", self.rel))?;
        *self = Self::from_text(self.rel.clone(), self.path.clone(), previous);
        Ok(())
    }

    // ---- display ---------------------------------------------------------

    pub fn rows(&self) -> Vec<RowRef<'_>> {
        let mut out = Vec::with_capacity(self.lines.len());
        for (index, line) in self.lines.iter().enumerate() {
            match self.edits.get(&index) {
                None => out.push(RowRef {
                    kind: RowKind::Normal,
                    line: index,
                    number: Some(index + 1),
                    show_actions: true,
                    src: line.as_str(),
                }),
                Some(replacement) => {
                    out.push(RowRef {
                        kind: RowKind::Removed,
                        line: index,
                        number: Some(index + 1),
                        show_actions: true,
                        src: line.as_str(),
                    });
                    for new_line in replacement {
                        out.push(RowRef {
                            kind: RowKind::Added,
                            line: index,
                            number: None,
                            show_actions: false,
                            src: new_line.as_str(),
                        });
                    }
                }
            }
        }
        out
    }

    /// Width of the code column, so long lines scroll sideways instead of wrapping.
    pub fn text_width(&self) -> f32 {
        (self.max_chars as f32 * CHAR_W + 24.0).max(320.0)
    }

    /// Numbered lines around `line`, with the target marked, for the model prompt.
    pub fn context_around(&self, line: usize, radius: usize) -> String {
        let start = line.saturating_sub(radius);
        let end = (line + radius + 1).min(self.lines.len());
        let mut out = String::new();
        for index in start..end {
            let marker = if index == line { ">>>" } else { "   " };
            out.push_str(&format!("{marker} {:>4} | {}\n", index + 1, self.lines[index]));
        }
        out
    }

    pub fn lang(&self) -> &str {
        lang_for(&self.rel)
    }

    // ---- helpers ---------------------------------------------------------

    fn recompute_width(&mut self) {
        let edited = self.edits.values().flatten();
        self.max_chars = self
            .lines
            .iter()
            .chain(edited)
            .map(|line| display_len(line))
            .max()
            .unwrap_or(0)
            .min(MAX_DISPLAY_CHARS + 1);
    }

    fn join(&self, lines: &[String]) -> String {
        let separator = if self.crlf { "\r\n" } else { "\n" };
        let mut text = lines.join(separator);
        if self.trailing_newline && !lines.is_empty() {
            text.push_str(separator);
        }
        text
    }

    /// Refuse to overwrite a file that was edited elsewhere since it was opened.
    fn ensure_unchanged_on_disk(&self) -> Result<(), String> {
        let on_disk = fs::read_to_string(&self.path)
            .map_err(|e| format!("Could not read {}: {e}", self.rel))?;
        if on_disk != self.original_text {
            return Err(format!(
                "{} changed on disk since it was opened. Reload it and try again.",
                self.rel
            ));
        }
        Ok(())
    }
}

fn display_len(line: &str) -> usize {
    line.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

/// Text as shown in the code view: tabs become spaces and very long lines are cut.
pub fn display(line: &str) -> String {
    let expanded = line.replace('\t', "    ");
    if expanded.chars().count() > MAX_DISPLAY_CHARS {
        let head: String = expanded.chars().take(MAX_DISPLAY_CHARS).collect();
        format!("{head}…")
    } else {
        expanded
    }
}