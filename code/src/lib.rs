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
    /// Set on the first row of a change group: the group the accept / reject
    /// buttons act on.
    pub hunk: Option<usize>,
    /// The group is still being written by the model (no accept button yet).
    pub typing: bool,
    pub src: &'a str,
}

/// One AI change: the original lines `first..=last` and their replacement.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// (reply action, hunk number) when the change comes from a streamed reply.
    pub key: Option<(usize, usize)>,
    pub first: usize,
    pub last: usize,
    /// The model is still writing the replacement text.
    pub typing: bool,
}

/// One step of the undo history: the file as it was before an accept.
#[derive(Debug, Clone)]
struct Snapshot {
    text: String,
    /// Streamed changes that this accept wrote, so they can be shown again.
    keys: Vec<(usize, usize)>,
}

pub struct CodeDoc {
    pub rel: String,
    pub path: PathBuf,
    pub lines: Vec<String>,
    /// Pending AI edits: original line index -> replacement lines (empty = delete).
    pub edits: BTreeMap<usize, Vec<String>>,
    /// The pending edits grouped per change (one SEARCH/REPLACE block each).
    pub groups: Vec<Group>,
    /// File contents before each accept, newest last (multi-level undo).
    undo: Vec<Snapshot>,
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
            groups: Vec::new(),
            undo: Vec::new(),
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
        self.stage_keyed(None, search, replace, false)
    }

    /// Like `stage`, for a block of a streamed reply. Staging the same `key` again
    /// replaces the earlier version, so it can be called on every flush while the
    /// model is still typing (`typing` = the REPLACE text is not finished).
    pub fn stage_keyed(
        &mut self,
        key: Option<(usize, usize)>,
        search: &str,
        replace: &str,
        typing: bool,
    ) -> Result<(), String> {
        fn end(s: &str) -> &str {
            s.trim_end()
        }
        fn both(s: &str) -> &str {
            s.trim()
        }

        // Drop the previous version of this block first.
        if let Some(key) = key {
            if let Some(pos) = self.groups.iter().position(|g| g.key == Some(key)) {
                self.drop_group(pos);
            }
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

        let start = match hits.as_slice() {
            [] => return Err("search block not found".into()),
            [start] => *start,
            _ => return Err("search block matches more than once".into()),
        };
        let last = start + needle.len() - 1;
        if (start..=last).any(|i| self.edits.contains_key(&i)) {
            return Err("overlaps another pending change".into());
        }

        let replacement: Vec<String> = replace.lines().map(String::from).collect();
        self.edits.insert(start, replacement);
        for i in start + 1..=last {
            self.edits.insert(i, Vec::new());
        }
        let pos = self.groups.partition_point(|g| g.first < start);
        self.groups.insert(pos, Group { key, first: start, last, typing });
        self.recompute_width();
        Ok(())
    }

    pub fn set_edit(&mut self, line: usize, replacement: Vec<String>) {
        if line < self.lines.len() {
            self.edits.insert(line, replacement);
            self.recompute_width();
        }
    }

    /// Revert one line. A line that belongs to a change group rejects the whole
    /// group, since half a change would not make sense.
    pub fn clear_edit(&mut self, line: usize) {
        match self.groups.iter().position(|g| (g.first..=g.last).contains(&line)) {
            Some(pos) => self.drop_group(pos),
            None => {
                self.edits.remove(&line);
            }
        }
        self.recompute_width();
    }

    pub fn clear_all(&mut self) {
        self.edits.clear();
        self.groups.clear();
        self.recompute_width();
    }

    /// Reject one change: the original lines stay as they are.
    pub fn reject_group(&mut self, index: usize) {
        if index < self.groups.len() {
            self.drop_group(index);
            self.recompute_width();
        }
    }

    fn drop_group(&mut self, index: usize) {
        let group = self.groups.remove(index);
        for line in group.first..=group.last {
            self.edits.remove(&line);
        }
    }

    /// Number of changes pending: change groups plus loose single-line edits.
    pub fn change_count(&self) -> usize {
        let grouped: usize = self.groups.iter().map(|g| g.last - g.first + 1).sum();
        let loose = self.edits.len().saturating_sub(grouped);
        self.groups.len() + loose
    }

    /// Forget which streamed reply the pending changes came from, so a new reply
    /// cannot replace them. They stay reviewable.
    pub fn detach_keys(&mut self) {
        for g in &mut self.groups {
            g.key = None;
        }
    }

    /// Remove changes the model never finished writing (stopped or cut off).
    pub fn drop_typing(&mut self) {
        while let Some(pos) = self.groups.iter().position(|g| g.typing) {
            self.drop_group(pos);
        }
        self.recompute_width();
    }

    pub fn has_pending(&self) -> bool {
        !self.edits.is_empty()
    }

    /// Changes whose text is complete (not still being typed).
    pub fn ready_groups(&self) -> usize {
        self.groups.iter().filter(|g| !g.typing).count()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Accept one change: write it to the file and move the other pending changes
    /// to their new line numbers. Returns the streamed key of the change.
    pub fn accept_group(&mut self, index: usize) -> Result<Option<(usize, usize)>, String> {
        let Some(group) = self.groups.get(index).cloned() else {
            return Err("That change is no longer pending".into());
        };
        if group.typing {
            return Err("The model is still writing this change".into());
        }
        self.ensure_unchanged_on_disk()?;

        let mut out: Vec<String> = Vec::with_capacity(self.lines.len());
        let mut inserted = 0usize;
        for (i, line) in self.lines.iter().enumerate() {
            if (group.first..=group.last).contains(&i) {
                if let Some(rep) = self.edits.get(&i) {
                    inserted += rep.len();
                    out.extend(rep.iter().cloned());
                }
            } else {
                out.push(line.clone());
            }
        }
        let removed = group.last - group.first + 1;
        let new_text = self.join(&out);
        fs::write(&self.path, &new_text)
            .map_err(|e| format!("Could not write {}: {e}", self.rel))?;

        // Move everything after the change by the number of lines it added.
        let shift = |i: usize| -> usize {
            if i > group.last {
                i + inserted - removed
            } else {
                i
            }
        };
        let mut edits = BTreeMap::new();
        for (line, rep) in std::mem::take(&mut self.edits) {
            if !(group.first..=group.last).contains(&line) {
                edits.insert(shift(line), rep);
            }
        }
        self.edits = edits;
        self.groups.remove(index);
        for g in &mut self.groups {
            g.first = shift(g.first);
            g.last = shift(g.last);
        }

        let before = std::mem::replace(&mut self.original_text, new_text);
        self.undo.push(Snapshot { text: before, keys: group.key.into_iter().collect() });
        self.lines = out;
        self.recompute_width();
        Ok(group.key)
    }

    /// Write all pending changes to the file. Returns how many were applied.
    /// Changes that are still being typed stay pending.
    pub fn apply(&mut self) -> Result<usize, String> {
        if self.edits.is_empty() {
            return Ok(0);
        }
        self.ensure_unchanged_on_disk()?;

        // Only finished changes are written. Lines of a typing group are kept.
        let typing: Vec<(usize, usize)> = self
            .groups
            .iter()
            .filter(|g| g.typing)
            .map(|g| (g.first, g.last))
            .collect();
        let in_typing = |i: usize| typing.iter().any(|(a, b)| (*a..=*b).contains(&i));

        let mut out: Vec<String> = Vec::with_capacity(self.lines.len());
        let mut remap: Vec<usize> = Vec::with_capacity(self.lines.len());
        for (index, line) in self.lines.iter().enumerate() {
            remap.push(out.len());
            match self.edits.get(&index) {
                Some(replacement) if !in_typing(index) => out.extend(replacement.iter().cloned()),
                _ => out.push(line.clone()),
            }
        }

        let new_text = self.join(&out);
        let count = self.edits.keys().filter(|i| !in_typing(**i)).count();
        if count == 0 {
            return Ok(0);
        }
        fs::write(&self.path, &new_text)
            .map_err(|e| format!("Could not write {}: {e}", self.rel))?;

        let keys: Vec<(usize, usize)> = self
            .groups
            .iter()
            .filter(|g| !g.typing)
            .filter_map(|g| g.key)
            .collect();
        let before = std::mem::replace(&mut self.original_text, new_text);
        self.undo.push(Snapshot { text: before, keys });

        // Keep the groups that are still being typed, at their new line numbers.
        let kept: Vec<Group> = self
            .groups
            .iter()
            .filter(|g| g.typing)
            .map(|g| Group { first: remap[g.first], last: remap[g.last], ..g.clone() })
            .collect();
        let mut kept_edits = BTreeMap::new();
        for g in &self.groups {
            if g.typing {
                for i in g.first..=g.last {
                    if let Some(rep) = self.edits.get(&i) {
                        kept_edits.insert(remap[i], rep.clone());
                    }
                }
            }
        }
        self.lines = out;
        self.edits = kept_edits;
        self.groups = kept;
        self.recompute_width();
        Ok(count)
    }

    /// Put the file back to how it was before the last accept. Pending changes are
    /// dropped (their line numbers no longer fit). Returns the streamed changes
    /// that the undone accept had written, so they can be shown again.
    pub fn undo_apply(&mut self) -> Result<Vec<(usize, usize)>, String> {
        let Some(snapshot) = self.undo.last().cloned() else {
            return Err("Nothing to undo".into());
        };
        self.ensure_unchanged_on_disk()?;
        fs::write(&self.path, &snapshot.text)
            .map_err(|e| format!("Could not write {}: {e}", self.rel))?;

        let mut history = std::mem::take(&mut self.undo);
        history.pop();
        *self = Self::from_text(self.rel.clone(), self.path.clone(), snapshot.text);
        self.undo = history;
        Ok(snapshot.keys)
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
                    hunk: None,
                    typing: false,
                    src: line.as_str(),
                }),
                Some(replacement) => {
                    let group = self.groups.iter().position(|g| g.first == index);
                    let typing = self
                        .groups
                        .iter()
                        .find(|g| (g.first..=g.last).contains(&index))
                        .is_some_and(|g| g.typing);
                    out.push(RowRef {
                        kind: RowKind::Removed,
                        line: index,
                        number: Some(index + 1),
                        show_actions: true,
                        hunk: group,
                        typing,
                        src: line.as_str(),
                    });
                    for new_line in replacement {
                        out.push(RowRef {
                            kind: RowKind::Added,
                            line: index,
                            number: None,
                            show_actions: false,
                            hunk: None,
                            typing,
                            src: new_line.as_str(),
                        });
                    }
                }
            }
        }
        out
    }

    /// Index of the first displayed row of the change that is being typed, so the
    /// view can follow the model. `None` when nothing is being typed.
    pub fn typing_row(&self) -> Option<usize> {
        let group = self.groups.iter().rev().find(|g| g.typing)?;
        let mut row = 0;
        for (index, _) in self.lines.iter().enumerate() {
            if index == group.first {
                // the last added row of the group
                let added = self.edits.get(&index).map_or(0, Vec::len);
                return Some(row + added);
            }
            row += 1 + self.edits.get(&index).map_or(0, Vec::len);
        }
        None
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
#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> (CodeDoc, PathBuf) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "virgo-code-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("f.txt"), text).unwrap();
        (CodeDoc::open(&dir, "f.txt").unwrap(), dir)
    }

    fn disk(dir: &Path) -> String {
        fs::read_to_string(dir.join("f.txt")).unwrap()
    }

    #[test]
    fn restaging_a_key_replaces_the_previous_version() {
        let (mut d, _) = doc("a\nb\nc\n");
        d.stage_keyed(Some((0, 0)), "b", "B", true).unwrap();
        d.stage_keyed(Some((0, 0)), "b", "B\nB2", true).unwrap();
        assert_eq!(d.groups.len(), 1);
        assert_eq!(d.edits[&1], vec!["B".to_string(), "B2".to_string()]);
        assert!(d.groups[0].typing);
    }

    #[test]
    fn accept_one_group_shifts_the_other() {
        let (mut d, dir) = doc("a\nb\nc\nd\n");
        d.stage_keyed(Some((0, 0)), "a", "a1\na2\na3", false).unwrap();
        d.stage_keyed(Some((0, 1)), "c", "C", false).unwrap();
        assert_eq!(d.accept_group(0).unwrap(), Some((0, 0)));
        assert_eq!(disk(&dir), "a1\na2\na3\nb\nc\nd\n");
        // the second change moved down by two lines
        assert_eq!(d.groups[0].first, 4);
        assert_eq!(d.edits[&4], vec!["C".to_string()]);
        assert_eq!(d.accept_group(0).unwrap(), Some((0, 1)));
        assert_eq!(disk(&dir), "a1\na2\na3\nb\nC\nd\n");
    }

    #[test]
    fn reject_leaves_the_file_alone() {
        let (mut d, dir) = doc("a\nb\n");
        d.stage_keyed(None, "a", "x", false).unwrap();
        d.reject_group(0);
        assert!(!d.has_pending());
        assert_eq!(disk(&dir), "a\nb\n");
    }

    #[test]
    fn undo_goes_back_several_levels() {
        let (mut d, dir) = doc("a\nb\nc\n");
        d.stage_keyed(Some((0, 0)), "a", "A", false).unwrap();
        d.accept_group(0).unwrap();
        d.stage_keyed(Some((0, 1)), "c", "C", false).unwrap();
        d.accept_group(0).unwrap();
        assert_eq!(disk(&dir), "A\nb\nC\n");
        assert_eq!(d.undo_depth(), 2);
        assert_eq!(d.undo_apply().unwrap(), vec![(0, 1)]);
        assert_eq!(disk(&dir), "A\nb\nc\n");
        assert_eq!(d.undo_apply().unwrap(), vec![(0, 0)]);
        assert_eq!(disk(&dir), "a\nb\nc\n");
        assert!(d.undo_apply().is_err());
    }

    #[test]
    fn typing_group_cannot_be_accepted_and_survives_apply_all() {
        let (mut d, dir) = doc("a\nb\nc\n");
        d.stage_keyed(Some((0, 0)), "a", "A", false).unwrap();
        d.stage_keyed(Some((0, 1)), "c", "C\nmore", true).unwrap();
        assert!(d.accept_group(1).is_err());
        assert_eq!(d.apply().unwrap(), 1);
        assert_eq!(disk(&dir), "A\nb\nc\n");
        assert_eq!(d.groups.len(), 1);
        assert!(d.groups[0].typing);
        assert_eq!(d.groups[0].first, 2);
    }

    #[test]
    fn overlapping_change_is_refused() {
        let (mut d, _) = doc("a\nb\nc\n");
        d.stage_keyed(Some((0, 0)), "a\nb", "x", false).unwrap();
        assert!(d.stage_keyed(Some((0, 1)), "b\nc", "y", false).is_err());
    }

    #[test]
    fn accept_refuses_when_the_file_changed_on_disk() {
        let (mut d, dir) = doc("a\nb\n");
        d.stage_keyed(None, "a", "A", false).unwrap();
        fs::write(dir.join("f.txt"), "edited elsewhere\n").unwrap();
        assert!(d.accept_group(0).is_err());
        assert_eq!(disk(&dir), "edited elsewhere\n");
    }

    #[test]
    fn deleting_lines_and_clearing_a_line_rejects_the_group() {
        let (mut d, dir) = doc("a\nb\nc\n");
        d.stage_keyed(None, "a\nb", "", false).unwrap();
        d.clear_edit(1);
        assert!(!d.has_pending());
        d.stage_keyed(None, "a\nb", "", false).unwrap();
        d.accept_group(0).unwrap();
        assert_eq!(disk(&dir), "c\n");
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn drop_typing_and_detach_keys() {
        let dir = std::env::temp_dir().join(format!("virgo-code-more-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("f.txt"), "a\nb\nc\n").unwrap();
        let mut d = CodeDoc::open(&dir, "f.txt").unwrap();
        d.stage_keyed(Some((0, 0)), "a", "A", false).unwrap();
        d.stage_keyed(Some((0, 1)), "c", "C", true).unwrap();
        assert_eq!(d.change_count(), 2);
        d.drop_typing();
        assert_eq!(d.change_count(), 1);
        d.detach_keys();
        assert_eq!(d.groups[0].key, None);
        // a fresh stage with the old key no longer replaces the detached change
        d.stage_keyed(Some((0, 0)), "b", "B", false).unwrap();
        assert_eq!(d.groups.len(), 2);
    }
}
