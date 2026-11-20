//! The file open in the code panel, plus the AI edits waiting to be accepted or rejected.
//!
//! Two kinds of pending edit exist:
//! * line edits (`edits`): one original line -> zero or more new lines (the bulb suggestions);
//! * hunks (`pending`): SEARCH/REPLACE blocks from the model. A hunk is stored as TEXT and is
//!   re-located in the file whenever the file changes (`restage`), so accepting one hunk never
//!   leaves the others pointing at stale line numbers.
//!
//! One extra "live" hunk (`LIVE_ID`) shows what the model is typing right now.
//! Nothing touches the disk until `accept`, `apply` or `undo_apply`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use project::lang_for;

/// Files bigger than this are not opened in the code panel.
const MAX_BYTES: u64 = 1_000_000;
/// Long lines are cut off in the view (the file itself is never changed by this).
const MAX_DISPLAY_CHARS: usize = 300;
/// Approximate width of one monospace character at the code font size.
const CHAR_W: f32 = 8.0;

/// Id of the hunk the model is still typing. Real hunks start at 1.
pub const LIVE_ID: u32 = 0;

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
    /// Whether to show buttons on this row.
    pub show_actions: bool,
    /// Set on the FIRST red row of a hunk: the id for Accept / Reject.
    /// `Some(LIVE_ID)` means the model is still typing it.
    pub hunk: Option<u32>,
    pub src: &'a str,
}

/// A SEARCH/REPLACE block waiting for the user.
#[derive(Debug, Clone)]
pub struct PendingHunk {
    pub id: u32,
    pub search: String,
    pub replace: String,
    /// Why it could not be located in the file right now.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct Group {
    id: u32,
    start: usize,
    len: usize,
}

pub struct CodeDoc {
    pub rel: String,
    pub path: PathBuf,
    pub lines: Vec<String>,
    /// Pending edits by original line index -> replacement lines (empty = delete).
    /// Includes the lines owned by hunk groups.
    pub edits: BTreeMap<usize, Vec<String>>,
    /// File content before the last accept/apply, so it can be undone once.
    pub previous: Option<String>,
    /// Hunks that the last accept/apply consumed (they come back on undo).
    previous_hunks: Vec<PendingHunk>,
    /// Model hunks waiting for Accept / Reject.
    pub pending: Vec<PendingHunk>,
    groups: Vec<Group>,
    live: bool,
    next_id: u32,
    original_text: String,
    crlf: bool,
    trailing_newline: bool,
    max_chars: usize,
    /// Lines (0-based) that the last applied change added or rewrote. Shown green.
    flash: BTreeSet<usize>,
}

impl CodeDoc {
    pub fn open(root: &Path, rel: &str) -> Result<Self, String> {
        if rel.split('/').any(|part| part == "..") {
            return Err("invalid path".into());
        }
        let path = root.join(rel);
        let meta = fs::metadata(&path).map_err(|e| format!("{rel}: {e}"))?;
        if meta.len() > MAX_BYTES {
            return Err(format!(
                "{rel} is larger than 1 MB, so it is not opened here"
            ));
        }
        let text =
            fs::read_to_string(&path).map_err(|_| format!("{rel} is not a UTF-8 text file"))?;
        Ok(Self::from_text(rel.to_string(), path, text))
    }

    fn from_text(rel: String, path: PathBuf, text: String) -> Self {
        let mut doc = Self {
            rel,
            path,
            lines: text.lines().map(String::from).collect(),
            edits: BTreeMap::new(),
            previous: None,
            previous_hunks: Vec::new(),
            pending: Vec::new(),
            groups: Vec::new(),
            live: false,
            next_id: 1,
            crlf: text.contains("\r\n"),
            trailing_newline: text.ends_with('\n'),
            original_text: text,
            max_chars: 0,
            flash: BTreeSet::new(),
        };
        doc.recompute_width();
        doc
    }

    /// Re-read the file from disk. Line edits are dropped; model hunks are kept and
    /// re-located in the new text.
    pub fn reload(&mut self) -> Result<(), String> {
        let text = fs::read_to_string(&self.path)
            .map_err(|e| format!("Could not reload {}: {e}", self.rel))?;
        let pending = std::mem::take(&mut self.pending);
        let next_id = self.next_id;
        *self = Self::from_text(self.rel.clone(), self.path.clone(), text);
        self.pending = pending;
        self.next_id = next_id;
        self.restage();
        Ok(())
    }

    /// The text of the file as last read from or written to disk.
    pub fn text(&self) -> &str {
        &self.original_text
    }

    // ---- hunks (live editing) ---------------------------------------------

    /// Queue a finished SEARCH/REPLACE block. Returns its id.
    pub fn add_pending(&mut self, search: String, replace: String) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.pending.push(PendingHunk {
            id,
            search,
            replace,
            error: None,
        });
        self.restage();
        id
    }

    /// Show the hunk the model is typing. Before its SEARCH is complete nothing is drawn
    /// (`writing == false`); afterwards the matched lines turn red and the text typed so far
    /// shows as green rows followed by a cursor.
    pub fn set_live(&mut self, search: &str, replace: &str, writing: bool) {
        self.clear_live();
        self.live = true;
        if !writing {
            return;
        }
        let Ok((start, n)) = locate(&self.lines, search) else {
            return;
        };
        if self
            .groups
            .iter()
            .any(|g| start < g.start + g.len && g.start < start + n)
        {
            return;
        }
        let mut shown: Vec<String> = replace.lines().map(String::from).collect();
        match shown.last_mut() {
            Some(last) => last.push('\u{258D}'),
            None => shown.push("\u{258D}".to_string()),
        }
        self.edits.insert(start, shown);
        for i in start + 1..start + n {
            self.edits.insert(i, Vec::new());
        }
        self.groups.push(Group {
            id: LIVE_ID,
            start,
            len: n,
        });
        self.recompute_width();
    }

    pub fn clear_live(&mut self) {
        self.live = false;
        if let Some(pos) = self.groups.iter().position(|g| g.id == LIVE_ID) {
            let g = self.groups.remove(pos);
            for i in g.start..g.start + g.len {
                self.edits.remove(&i);
            }
            self.recompute_width();
        }
    }

    /// The model is typing into this file right now.
    pub fn is_live(&self) -> bool {
        self.live
    }

    /// Anything on screen that the user still has to decide on.
    pub fn has_edits(&self) -> bool {
        !self.edits.is_empty() || self.live
    }

    /// Hunks that could not be found in the file.
    pub fn failed_hunks(&self) -> usize {
        self.pending.iter().filter(|p| p.error.is_some()).count()
    }

    /// Number of things to accept: located hunks plus single-line suggestions.
    pub fn change_count(&self) -> usize {
        let hunks = self.groups.iter().filter(|g| g.id != LIVE_ID).count();
        let owned: usize = self.groups.iter().map(|g| g.len).sum();
        hunks + self.edits.len().saturating_sub(owned)
    }

    /// First and last display row of the live hunk (for auto-scroll).
    pub fn live_span(&self) -> Option<(usize, usize)> {
        let g = self.groups.iter().find(|g| g.id == LIVE_ID)?;
        let first = self.row_of(g.start);
        let added = self.edits.get(&g.start).map_or(0, |r| r.len());
        Some((first, first + g.len + added))
    }

    /// Display row index at which original line `line` starts.
    pub fn row_of(&self, line: usize) -> usize {
        (0..line.min(self.lines.len()))
            .map(|i| 1 + self.edits.get(&i).map_or(0, |r| r.len()))
            .sum()
    }

    /// Write ONE hunk to disk. Returns (content before, content after) so the caller can
    /// record it for "Revert". The other hunks stay pending and are re-located.
    pub fn accept(&mut self, id: u32) -> Result<(String, String), String> {
        let idx = self
            .pending
            .iter()
            .position(|p| p.id == id)
            .ok_or("that change no longer exists")?;
        let Some(g) = self.groups.iter().find(|g| g.id == id).copied() else {
            return Err(self.pending[idx]
                .error
                .clone()
                .unwrap_or_else(|| "that change could not be located".into()));
        };
        self.ensure_unchanged_on_disk()?;

        let replace: Vec<String> = self.pending[idx]
            .replace
            .lines()
            .map(String::from)
            .collect();
        let mut out: Vec<String> = Vec::with_capacity(self.lines.len());
        for (i, line) in self.lines.iter().enumerate() {
            if i == g.start {
                out.extend(replace.iter().cloned());
            }
            if i < g.start || i >= g.start + g.len {
                out.push(line.clone());
            }
        }

        let new_text = self.join(&out);
        fs::write(&self.path, &new_text)
            .map_err(|e| format!("Could not write {}: {e}", self.rel))?;

        // Keep single-line suggestions in place: shift the ones below the hunk.
        let delta = replace.len() as isize - g.len as isize;
        self.drop_group_edits();
        let old = std::mem::take(&mut self.edits);
        for (k, v) in old {
            if k < g.start {
                self.edits.insert(k, v);
            } else if k >= g.start + g.len {
                self.edits.insert((k as isize + delta) as usize, v);
            }
        }

        let before = std::mem::replace(&mut self.original_text, new_text.clone());
        self.previous = Some(before.clone());
        self.previous_hunks = vec![self.pending.remove(idx)];
        self.lines = out;
        self.mark_changed(&before); // highlight what was just written
        self.restage();
        Ok((before, new_text))
    }

    /// Drop one hunk without touching the file.
    pub fn reject(&mut self, id: u32) {
        self.pending.retain(|p| p.id != id);
        self.restage();
    }

    /// Remove the edit rows owned by hunk groups (the live one included).
    fn drop_group_edits(&mut self) {
        for g in std::mem::take(&mut self.groups) {
            for i in g.start..g.start + g.len {
                self.edits.remove(&i);
            }
        }
    }

    /// Find every pending hunk in the current text and rebuild the rows they own.
    fn restage(&mut self) {
        self.drop_group_edits();
        let mut used: Vec<(usize, usize)> = Vec::new();
        let mut pending = std::mem::take(&mut self.pending);
        for p in pending.iter_mut() {
            match locate(&self.lines, &p.search) {
                Ok((start, n)) => {
                    if used.iter().any(|(s, l)| start < s + l && *s < start + n) {
                        p.error = Some("overlaps another change".into());
                        continue;
                    }
                    used.push((start, n));
                    p.error = None;
                    self.edits
                        .insert(start, p.replace.lines().map(String::from).collect());
                    for i in start + 1..start + n {
                        self.edits.insert(i, Vec::new());
                    }
                    self.groups.push(Group {
                        id: p.id,
                        start,
                        len: n,
                    });
                }
                Err(e) => p.error = Some(e),
            }
        }
        self.pending = pending;
        self.recompute_width();
    }

    // ---- line edits --------------------------------------------------------

    /// Turn a SEARCH/REPLACE block into pending line edits. The first matched
    /// line gets the replacement, the other matched lines are deleted.
    pub fn stage(&mut self, search: &str, replace: &str) -> Result<(), String> {
        let (start, n) = locate(&self.lines, search)?;
        let replacement: Vec<String> = replace.lines().map(String::from).collect();
        self.set_edit(start, replacement);
        for i in start + 1..start + n {
            self.set_edit(i, Vec::new());
        }
        Ok(())
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

    /// Reject everything: hunks, live text and line edits.
    pub fn clear_all(&mut self) {
        self.edits.clear();
        self.pending.clear();
        self.groups.clear();
        self.live = false;
        self.recompute_width();
    }

    /// Accept everything: write all pending edits to the file. Returns how many were applied.
    pub fn apply(&mut self) -> Result<usize, String> {
        self.clear_live(); // never write the half-typed hunk
        if self.edits.is_empty() {
            return Ok(0);
        }
        self.ensure_unchanged_on_disk()?;

        let count = self.change_count();
        let applied: Vec<u32> = self.groups.iter().map(|g| g.id).collect();

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

        let before = std::mem::replace(&mut self.original_text, new_text);
        self.previous = Some(before.clone());
        self.previous_hunks = self
            .pending
            .iter()
            .filter(|p| applied.contains(&p.id))
            .cloned()
            .collect();
        self.pending.retain(|p| !applied.contains(&p.id));
        self.lines = out;
        self.edits.clear();
        self.groups.clear();
        self.mark_changed(&before); // highlight what was just written
        self.restage();
        Ok(count)
    }

    /// Put the file back to how it was before the last accept. The hunks that were
    /// accepted come back as pending, so you can decide again.
    pub fn undo_apply(&mut self) -> Result<(), String> {
        let Some(previous) = self.previous.clone() else {
            return Err("Nothing to undo".into());
        };
        self.ensure_unchanged_on_disk()?;
        fs::write(&self.path, &previous)
            .map_err(|e| format!("Could not write {}: {e}", self.rel))?;

        let mut pending = std::mem::take(&mut self.pending);
        pending.extend(std::mem::take(&mut self.previous_hunks));
        let next_id = self.next_id;
        *self = Self::from_text(self.rel.clone(), self.path.clone(), previous);
        self.pending = pending;
        self.next_id = next_id;
        self.restage();
        Ok(())
    }

    // ---- display ---------------------------------------------------------
    pub fn rows(&self) -> Vec<RowRef<'_>> {
        // line -> (group id, is the first line of the group)
        let mut owner: BTreeMap<usize, (u32, bool)> = BTreeMap::new();
        for g in &self.groups {
            for i in g.start..g.start + g.len {
                owner.insert(i, (g.id, i == g.start));
            }
        }

        let mut out = Vec::with_capacity(self.lines.len());
        for (index, line) in self.lines.iter().enumerate() {
            match self.edits.get(&index) {
                None => out.push(RowRef {
                    // Lines the last applied change wrote are shown green.
                    kind: if self.flash.contains(&index) {
                        RowKind::Added
                    } else {
                        RowKind::Normal
                    },
                    line: index,
                    number: Some(index + 1),
                    show_actions: true,
                    hunk: None,
                    src: line.as_str(),
                }),
                Some(replacement) => {
                    let info = owner.get(&index).copied();
                    out.push(RowRef {
                        kind: RowKind::Removed,
                        line: index,
                        number: Some(index + 1),
                        show_actions: info.map_or(true, |(_, first)| first),
                        hunk: info.and_then(|(id, first)| first.then_some(id)),
                        src: line.as_str(),
                    });
                    for new_line in replacement {
                        out.push(RowRef {
                            kind: RowKind::Added,
                            line: index,
                            number: None,
                            show_actions: false,
                            hunk: None,
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
            out.push_str(&format!(
                "{marker} {:>4} | {}\n",
                index + 1,
                self.lines[index]
            ));
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

    /// Anything waiting for a decision (located hunks, failed hunks or line edits).
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty() || !self.edits.is_empty()
    }

    pub fn can_undo(&self) -> bool {
        self.previous.is_some()
    }

    pub fn undo_depth(&self) -> usize {
        usize::from(self.previous.is_some())
    }

    /// Highlight the lines of the current text that differ from `before`.
    pub fn mark_changed(&mut self, before: &str) {
        self.flash = changed_lines(before, &self.lines);
    }

    pub fn clear_flash(&mut self) {
        self.flash.clear();
    }
}

/// Find `search` in `lines`: exact (ignoring trailing spaces) first, then ignoring
/// indentation. Returns (first line, number of lines). Errors when absent or ambiguous.
fn locate(lines: &[String], search: &str) -> Result<(usize, usize), String> {
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
    if needle.len() > lines.len() {
        return Err("search block is longer than the file".into());
    }

    let find = |norm: fn(&str) -> &str| -> Vec<usize> {
        (0..=lines.len() - needle.len())
            .filter(|&i| {
                needle
                    .iter()
                    .enumerate()
                    .all(|(k, n)| norm(&lines[i + k]) == norm(n))
            })
            .collect()
    };

    let mut hits = find(end);
    if hits.is_empty() {
        hits = find(both);
    }
    match hits.as_slice() {
        [] => Err("search block not found".into()),
        [start] => Ok((*start, needle.len())),
        _ => Err("search block matches more than once".into()),
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

/// Indexes of lines in `after` that are new or rewritten compared with `before`.
/// Trims the common start and end, then runs an LCS on the middle part.
fn changed_lines(before: &str, after: &[String]) -> BTreeSet<usize> {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.iter().map(String::as_str).collect();

    let mut pre = 0;
    while pre < old.len() && pre < new.len() && old[pre] == new[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < old.len() - pre
        && suf < new.len() - pre
        && old[old.len() - 1 - suf] == new[new.len() - 1 - suf]
    {
        suf += 1;
    }

    let o = &old[pre..old.len() - suf];
    let n = &new[pre..new.len() - suf];
    let mut out = BTreeSet::new();

    // Pure insertion, or too big for the table: mark the whole changed span.
    if o.is_empty() || o.len() * n.len() > 4_000_000 {
        out.extend(pre..new.len() - suf);
        return out;
    }

    let w = n.len() + 1;
    let mut t = vec![0u32; (o.len() + 1) * w];
    for i in (0..o.len()).rev() {
        for j in (0..n.len()).rev() {
            t[i * w + j] = if o[i] == n[j] {
                t[(i + 1) * w + j + 1] + 1
            } else {
                t[(i + 1) * w + j].max(t[i * w + j + 1])
            };
        }
    }

    let (mut i, mut j) = (0, 0);
    while j < n.len() {
        if i < o.len() && o[i] == n[j] {
            i += 1;
            j += 1;
        } else if i < o.len() && t[(i + 1) * w + j] >= t[i * w + j + 1] {
            i += 1; // an old line that was removed
        } else {
            out.insert(pre + j); // a new line
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(name: &str, text: &str) -> (PathBuf, CodeDoc) {
        let root = std::env::temp_dir().join(format!("virgo-code-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("f.txt"), text).unwrap();
        let d = CodeDoc::open(&root, "f.txt").unwrap();
        (root, d)
    }

    #[test]
    fn accepting_one_hunk_relocates_the_others() {
        let (root, mut d) = doc("accept", "a\nb\nc\nd\n");
        let h1 = d.add_pending("b".into(), "B1\nB2".into());
        let h2 = d.add_pending("d".into(), "D".into());
        assert_eq!(d.change_count(), 2);

        d.accept(h1).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("f.txt")).unwrap(),
            "a\nB1\nB2\nc\nd\n"
        );
        assert_eq!(d.change_count(), 1); // "d" is now on line 5 (index 4)
        assert!(d.edits.contains_key(&4));

        d.accept(h2).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("f.txt")).unwrap(),
            "a\nB1\nB2\nc\nD\n"
        );
        assert!(!d.has_edits());
    }

    #[test]
    fn undo_brings_the_hunk_back_as_pending() {
        let (root, mut d) = doc("undo", "a\nb\nc\n");
        let h = d.add_pending("b".into(), "B".into());
        d.accept(h).unwrap();
        d.undo_apply().unwrap();
        assert_eq!(fs::read_to_string(root.join("f.txt")).unwrap(), "a\nb\nc\n");
        assert_eq!(d.change_count(), 1);
    }

    #[test]
    fn reject_leaves_the_file_alone_and_unlocatable_hunks_report_why() {
        let (root, mut d) = doc("reject", "a\nb\n");
        let h = d.add_pending("b".into(), "B".into());
        d.add_pending("zzz".into(), "x".into());
        assert_eq!(d.failed_hunks(), 1);
        d.reject(h);
        assert_eq!(fs::read_to_string(root.join("f.txt")).unwrap(), "a\nb\n");
        assert_eq!(d.change_count(), 0);
    }

    #[test]
    fn live_hunk_is_shown_but_never_written() {
        let (root, mut d) = doc("live", "a\nb\nc\n");
        d.set_live("b", "B and more", true);
        assert!(d.is_live() && d.live_span().is_some());
        assert_eq!(d.apply().unwrap(), 0);
        assert_eq!(fs::read_to_string(root.join("f.txt")).unwrap(), "a\nb\nc\n");
    }
}
