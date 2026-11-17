//! Live review of model edits (like Cursor's inline diff).
//!
//! While the model writes an `edit` block, every flush stages its SEARCH/REPLACE
//! hunks in the code panel as red/green rows, so the user watches the change being
//! typed. Nothing is written to disk until the user accepts a change. Accepted
//! changes go on a multi-level undo stack inside `CodeDoc`.
//!
//! Files the model touches are opened automatically. A file that still has pending
//! changes when another one takes over is "parked" and can be reopened from the
//! review bar.

use std::collections::HashSet;

use iced::Task;
use iced::widget::scrollable;

use code::CodeDoc;

use super::{ActionStatus, App, Message, Phase, Role};

/// Height of one code row in the code panel (must match the GUI).
const ROW_H: f32 = 24.0;

impl App {
    /// Reviewing is on for this reply: live edits are enabled, a folder is open and
    /// the reply is not a step of an already approved plan.
    pub(crate) fn live_wanted(&self, msg_idx: usize) -> bool {
        self.cfg.live_edit
            && self.project.is_some()
            && self.live_msg == Some(msg_idx)
            && self.plan_run.as_ref().is_none_or(|r| r.cur.is_none())
    }

    fn live_streaming(&self) -> bool {
        let last = self.messages.len().saturating_sub(1);
        self.phase == Phase::Executing
            && self.messages.get(last).is_some_and(|m| m.role == Role::Assistant)
            && self.live_wanted(last)
    }

    /// A new reply starts: earlier pending changes stay reviewable but can no
    /// longer be replaced by the new stream.
    pub(crate) fn begin_live(&mut self, msg_idx: usize) {
        for doc in self.code.iter_mut().chain(self.parked.values_mut()) {
            doc.detach_keys();
        }
        self.live_msg = Some(msg_idx);
        self.live_done.clear();
        self.live_accepted.clear();
        self.live_rejected.clear();
        self.live_new_files.clear();
    }

    /// Forget everything about the review (project closed or switched).
    pub(crate) fn reset_review(&mut self) {
        self.parked.clear();
        self.live_msg = None;
        self.live_done.clear();
        self.live_accepted.clear();
        self.live_rejected.clear();
        self.live_new_files.clear();
    }

    /// Files with changes waiting for a decision, with the number of changes.
    pub fn review_files(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = self
            .code
            .iter()
            .chain(self.parked.values())
            .filter(|d| d.has_pending())
            .map(|d| (d.rel.clone(), d.change_count()))
            .collect();
        out.sort();
        out
    }

    /// The model is writing a change right now.
    pub fn is_typing_edit(&self) -> bool {
        self.code
            .iter()
            .chain(self.parked.values())
            .any(|d| d.groups.iter().any(|g| g.typing))
    }

    /// (accepted, rejected, still open) hunks of one edit block of the live reply.
    pub fn live_card(&self, msg_idx: usize, action: usize, hunks: usize) -> Option<(usize, usize, usize)> {
        if self.live_msg != Some(msg_idx) || !self.cfg.live_edit {
            return None;
        }
        let count = |set: &HashSet<(usize, usize)>| (0..hunks).filter(|h| set.contains(&(action, *h))).count();
        let (a, r) = (count(&self.live_accepted), count(&self.live_rejected));
        Some((a, r, hunks.saturating_sub(a + r)))
    }

    // ---- streaming -------------------------------------------------------

    /// Stage the hunks of the reply written so far. Called on every flush.
    pub(crate) fn sync_live(&mut self) -> Task<Message> {
        if !self.live_streaming() {
            return Task::none();
        }
        let idx = self.messages.len() - 1;
        self.restage(idx, true)
    }

    /// Stage the hunks of message `msg_idx`. Finished hunks are staged once;
    /// the hunk being typed is staged again on every call.
    fn restage(&mut self, msg_idx: usize, streaming: bool) -> Task<Message> {
        let Some(content) = self.messages.get(msg_idx).map(|m| m.content.clone()) else {
            return Task::none();
        };
        let Ok(root) = self.edit_root() else {
            return Task::none();
        };

        let mut followed = false;
        for edit in workspace::live_edits(&content) {
            let rel = workspace::clean(&edit.path).to_string();
            // New files are written when their block is complete (see apply_streamed).
            if self.live_new_files.contains(&edit.action) || !root.join(&rel).is_file() {
                continue;
            }
            let todo: Vec<usize> = (0..edit.hunks.len())
                .filter(|h| !self.live_done.contains(&(edit.action, *h)))
                .filter(|h| edit.hunks[*h].done || streaming)
                .collect();
            if todo.is_empty() || !self.show_file(&rel) {
                continue;
            }
            let Some(doc) = self.code.as_mut() else {
                continue;
            };
            for h in todo {
                let hunk = &edit.hunks[h];
                let key = (edit.action, h);
                match doc.stage_keyed(Some(key), &hunk.search, &hunk.replace, !hunk.done) {
                    Ok(()) => {
                        followed = true;
                        if hunk.done {
                            self.live_done.insert(key);
                        }
                    }
                    // A hunk that is still being typed may simply not match yet.
                    Err(_) if !hunk.done => {}
                    Err(e) => {
                        self.live_done.insert(key);
                        self.action_status.insert(
                            (msg_idx, edit.action),
                            ActionStatus::Failed(format!("{rel}: {e}")),
                        );
                    }
                }
            }
        }

        if followed { self.follow_typing() } else { Task::none() }
    }

    /// Keep the row being typed in view.
    fn follow_typing(&mut self) -> Task<Message> {
        let Some(row) = self.code.as_ref().and_then(CodeDoc::typing_row) else {
            return Task::none();
        };
        let y = row as f32 * ROW_H;
        let (top, bottom) = (self.code_scroll_y, self.code_scroll_y + self.code_view_h);
        if y >= top && y + ROW_H * 2.0 <= bottom {
            return Task::none();
        }
        let target = (y - self.code_view_h * 0.6).max(0.0);
        self.code_scroll_y = target;
        scrollable::scroll_to(
            shared::code_scroll_id(),
            scrollable::AbsoluteOffset { x: 0.0, y: target },
        )
    }

    /// Show `rel` in the code panel. The file that was open keeps its pending
    /// changes by being parked.
    pub(crate) fn show_file(&mut self, rel: &str) -> bool {
        if self.code.as_ref().is_some_and(|d| d.rel == rel) {
            return true;
        }
        let doc = match self.take_doc(rel) {
            Ok(doc) => doc,
            Err(e) => {
                self.status = e;
                return false;
            }
        };
        self.park_open_doc();
        self.code = Some(doc);
        self.code_scroll_y = 0.0;
        self.suggest_line = None;
        self.suggest_input.clear();
        self.suggesting = None;
        true
    }

    /// The parked document of `rel`, or a fresh one read from disk.
    pub(crate) fn take_doc(&mut self, rel: &str) -> Result<CodeDoc, String> {
        if let Some(doc) = self.parked.remove(rel) {
            return Ok(doc);
        }
        let root = self
            .project
            .as_ref()
            .map(|p| p.root.clone())
            .ok_or("Open a project folder first")?;
        CodeDoc::open(&root, rel)
    }

    /// Move the open document out of the way if it has something worth keeping.
    pub(crate) fn park_open_doc(&mut self) {
        if let Some(old) = self.code.take() {
            if old.has_pending() || old.can_undo() {
                self.parked.insert(old.rel.clone(), old);
            }
        }
    }

    /// The reply ended: no more typing. Drop what the model did not finish and
    /// tell the user what is waiting for review.
    pub(crate) fn finish_live(&mut self, msg_idx: usize) -> Task<Message> {
        let _ = self.restage(msg_idx, false);
        self.drop_live_typing();

        let files = self.review_files();
        let failed = self
            .action_status
            .iter()
            .filter(|((m, _), s)| *m == msg_idx && matches!(s, ActionStatus::Failed(_)))
            .count();
        let total: usize = files.iter().map(|(_, n)| n).sum();

        self.status = match (total, failed) {
            (0, 0) => "Done".into(),
            (0, f) => format!("{f} change(s) could not be placed in the file (see the cards)."),
            (n, 0) => format!(
                "{n} change(s) in {} file(s) ready to review. Accept or reject each one in the code panel.",
                files.len()
            ),
            (n, f) => format!("{n} change(s) ready to review; {f} could not be placed."),
        };

        // Show the first file that needs a decision if nothing is open.
        if self.code.is_none() {
            if let Some((rel, _)) = files.first().cloned() {
                self.show_file(&rel);
            }
        }
        if failed > 0 { self.retry_failed(msg_idx) } else { Task::none() }
    }

    /// Stop or error while typing: unfinished changes are removed.
    pub(crate) fn drop_live_typing(&mut self) {
        for doc in self.code.iter_mut().chain(self.parked.values_mut()) {
            doc.drop_typing();
        }
    }

    // ---- accept / reject / undo ------------------------------------------

    pub(crate) fn accept_hunk(&mut self, index: usize) {
        let Some(doc) = self.code.as_mut() else {
            return;
        };
        let rel = doc.rel.clone();
        match doc.accept_group(index) {
            Ok(key) => {
                if let Some(key) = key {
                    self.live_done.insert(key);
                    self.live_accepted.insert(key);
                }
                let left = doc.change_count();
                self.status = format!("Accepted a change in {rel} ({left} left in this file)");
                self.tidy_after_decision();
            }
            Err(e) => self.status = e,
        }
    }

    pub(crate) fn reject_hunk(&mut self, index: usize) {
        let Some(doc) = self.code.as_mut() else {
            return;
        };
        let key = doc.groups.get(index).and_then(|g| g.key);
        doc.reject_group(index);
        if let Some(key) = key {
            self.live_done.insert(key); // never stage it again
            self.live_rejected.insert(key);
        }
        self.status = "Rejected a change".into();
        self.tidy_after_decision();
    }

    /// Accept every finished change of the open file.
    pub(crate) fn accept_file(&mut self) {
        let Some(doc) = self.code.as_mut() else {
            return;
        };
        let keys: Vec<(usize, usize)> =
            doc.groups.iter().filter(|g| !g.typing).filter_map(|g| g.key).collect();
        self.status = match doc.apply() {
            Ok(0) => "Nothing to accept".into(),
            Ok(n) => format!("Accepted {n} change(s) in {}", doc.rel),
            Err(e) => e,
        };
        for key in keys {
            if !doc.groups.iter().any(|g| g.key == Some(key)) {
                self.live_done.insert(key);
                self.live_accepted.insert(key);
            }
        }
        self.tidy_after_decision();
    }

    /// Reject every pending change of the open file.
    pub(crate) fn reject_file(&mut self) {
        let Some(doc) = self.code.as_mut() else {
            return;
        };
        for key in doc.groups.iter().filter_map(|g| g.key) {
            self.live_done.insert(key);
            self.live_rejected.insert(key);
        }
        doc.clear_all();
        self.status = "Rejected all pending changes in this file".into();
        self.tidy_after_decision();
    }

    /// Accept every finished change in every file.
    pub(crate) fn accept_everything(&mut self) {
        let (mut total, mut errors) = (0, Vec::new());
        let mut keys = Vec::new();
        for doc in self.code.iter_mut().chain(self.parked.values_mut()) {
            let before: Vec<(usize, usize)> =
                doc.groups.iter().filter(|g| !g.typing).filter_map(|g| g.key).collect();
            match doc.apply() {
                Ok(n) => {
                    total += n;
                    keys.extend(before);
                }
                Err(e) => errors.push(e),
            }
        }
        for key in keys {
            self.live_done.insert(key);
            self.live_accepted.insert(key);
        }
        self.status = match errors.first() {
            Some(e) if total == 0 => e.clone(),
            Some(e) => format!("Accepted {total} change(s); {e}"),
            None if total == 0 => "Nothing to accept".into(),
            None => format!("Accepted {total} change(s)"),
        };
        self.tidy_after_decision();
        self.refresh_project();
    }

    /// Reject every pending change in every file.
    pub(crate) fn reject_everything(&mut self) {
        for doc in self.code.iter_mut().chain(self.parked.values_mut()) {
            for key in doc.groups.iter().filter_map(|g| g.key) {
                self.live_done.insert(key);
                self.live_rejected.insert(key);
            }
            doc.clear_all();
        }
        self.status = "Rejected all pending changes".into();
        self.tidy_after_decision();
    }

    /// Undo the last accept in the open file. The streamed changes it wrote are
    /// shown again for review.
    pub(crate) fn undo_accept(&mut self) -> Task<Message> {
        let Some(doc) = self.code.as_mut() else {
            return Task::none();
        };
        // Pending changes are dropped by the undo; they are staged again below.
        for key in doc.groups.iter().filter_map(|g| g.key) {
            self.live_done.remove(&key);
        }
        match doc.undo_apply() {
            Ok(keys) => {
                for key in &keys {
                    self.live_done.remove(key);
                    self.live_accepted.remove(key);
                }
                self.status = format!("Undid the last accept in {}", doc.rel);
                match self.live_msg {
                    Some(idx) => self.restage(idx, false),
                    None => Task::none(),
                }
            }
            Err(e) => {
                self.status = e;
                Task::none()
            }
        }
    }

    /// Parked files without anything left to decide or undo are released.
    fn tidy_after_decision(&mut self) {
        self.parked.retain(|_, d| d.has_pending() || d.can_undo());
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use code::CodeDoc;

    /// Feed a reply to the same parser + staging calls the app uses, a few
    /// characters at a time, and check the accepted result equals `apply_hunks`.
    #[test]
    fn streamed_reply_stages_live_and_accepts_to_the_same_file() {
        let dir = std::env::temp_dir().join(format!("virgo-live-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let original = "fn a() {}\nfn b() {}\nfn c() {}\n";
        fs::write(dir.join("m.rs"), original).unwrap();

        let reply = "Sure.\n```edit m.rs\n<<<<<<< SEARCH\nfn a() {}\n=======\nfn a() {\n    one();\n    two();\n}\n>>>>>>> REPLACE\n<<<<<<< SEARCH\nfn c() {}\n=======\nfn c() { 3 }\n>>>>>>> REPLACE\n```\nDone.\n";

        let mut doc = CodeDoc::open(&dir, "m.rs").unwrap();
        let mut done = std::collections::HashSet::new();
        let chars: Vec<char> = reply.chars().collect();
        let mut seen_typing = false;
        for end in (1..=chars.len()).step_by(3).chain([chars.len()]) {
            let partial: String = chars[..end].iter().collect();
            for edit in workspace::live_edits(&partial) {
                for (h, hunk) in edit.hunks.iter().enumerate() {
                    let key = (edit.action, h);
                    if done.contains(&key) {
                        continue;
                    }
                    if doc
                        .stage_keyed(Some(key), &hunk.search, &hunk.replace, !hunk.done)
                        .is_ok()
                    {
                        seen_typing |= !hunk.done;
                        if hunk.done {
                            done.insert(key);
                        }
                    }
                }
            }
            // the file is never touched while streaming
            assert_eq!(fs::read_to_string(dir.join("m.rs")).unwrap(), original);
        }
        assert!(seen_typing, "the REPLACE text was visible while it was typed");
        assert_eq!(doc.groups.len(), 2);
        assert!(doc.groups.iter().all(|g| !g.typing));

        let hunks: Vec<workspace::Hunk> = workspace::parse_actions(reply)
            .into_iter()
            .flat_map(|a| match a {
                workspace::Action::Edit { hunks, .. } => hunks,
                _ => vec![],
            })
            .collect();
        let expected = workspace::apply_hunks(original, &hunks).unwrap();

        doc.accept_group(0).unwrap();
        doc.accept_group(0).unwrap();
        assert_eq!(fs::read_to_string(dir.join("m.rs")).unwrap(), expected);

        // two undos bring the original back
        doc.undo_apply().unwrap();
        doc.undo_apply().unwrap();
        assert_eq!(fs::read_to_string(dir.join("m.rs")).unwrap(), original);
    }
}
