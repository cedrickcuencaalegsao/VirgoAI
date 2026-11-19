//! Live review of model edits (like Cursor's inline diff).
//!
//! While the model writes an `edit` block, the hunk being typed is shown in the code
//! panel (`CodeDoc::set_live`). A finished hunk becomes a pending hunk
//! (`CodeDoc::add_pending`) that the user accepts or rejects. Nothing is written to
//! disk until the user accepts.
//!
//! A file that still has pending changes when another one takes over is "parked"
//! and can be reopened from the review bar.

use std::collections::HashSet;

use iced::Task;
use iced::widget::scrollable;

use code::CodeDoc;

use super::{ActionStatus, App, Message};

/// Height of one code row in the code panel (must match the GUI).
const ROW_H: f32 = 24.0;

impl App {
    /// Reviewing is on for this reply: live edits are enabled, a folder is open and
    /// the reply is not a step of an already approved plan.
    pub(crate) fn live_wanted(&self, msg_idx: usize) -> bool {
        self.cfg.live_edit && self.project.is_some() && self.live_msg == Some(msg_idx)
    }

    /// A new reply starts. Earlier pending hunks stay reviewable.
    pub(crate) fn begin_live(&mut self, msg_idx: usize) {
        self.live_msg = Some(msg_idx);
        self.live_done.clear();
        self.live_accepted.clear();
        self.live_rejected.clear();
        self.live_new_files.clear();
        self.live_map.clear();
    }

    /// Forget everything about the review (project closed or switched).
    pub(crate) fn reset_review(&mut self) {
        self.parked.clear();
        self.live_msg = None;
        self.live_done.clear();
        self.live_accepted.clear();
        self.live_rejected.clear();
        self.live_new_files.clear();
        self.live_map.clear();
    }

    /// Files with changes waiting for a decision, with the number of changes.
    pub fn review_files(&self) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = self
            .code
            .iter()
            .chain(self.parked.values())
            .filter(|d| d.has_pending() && d.change_count() > 0)
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
            .any(|d| d.is_live())
    }

    /// (accepted, rejected, still open) hunks of one edit block of the live reply.
    pub fn live_card(
        &self,
        msg_idx: usize,
        action: usize,
        hunks: usize,
    ) -> Option<(usize, usize, usize)> {
        if self.live_msg != Some(msg_idx) || !self.cfg.live_edit {
            return None;
        }
        let count = |set: &HashSet<(usize, usize)>| {
            (0..hunks).filter(|h| set.contains(&(action, *h))).count()
        };
        let (a, r) = (count(&self.live_accepted), count(&self.live_rejected));
        Some((a, r, hunks.saturating_sub(a + r)))
    }

    fn mark(&mut self, rel: &str, id: u32, accepted: bool) {
        if let Some(key) = self.live_map.get(&(rel.to_string(), id)).copied() {
            self.live_done.insert(key); // never stage it again
            if accepted {
                self.live_accepted.insert(key);
            } else {
                self.live_rejected.insert(key);
            }
        }
    }

    /// Finished hunks become pending hunks (once). The hunk being typed is shown
    /// as live text and redrawn on every call.
    pub(crate) fn restage(&mut self, msg_idx: usize, streaming: bool) -> Task<Message> {
        let Some(content) = self.messages.get(msg_idx).map(|m| m.content.clone()) else {
            return Task::none();
        };
        let Ok(root) = self.edit_root() else {
            return Task::none();
        };

        let mut followed = false;
        let mut typing_in: Option<String> = None;

        for edit in workspace::live_edits(&content) {
            let rel = workspace::clean(&edit.path).to_string();
            // New files are written when their block is complete (see apply_streamed).
            if self.live_new_files.contains(&edit.action) || !root.join(&rel).is_file() {
                continue;
            }
            for (h, hunk) in edit.hunks.iter().enumerate() {
                let key = (edit.action, h);
                if self.live_done.contains(&key) || (!hunk.done && !streaming) {
                    continue;
                }
                if !self.show_file(&rel) {
                    break;
                }
                let Some(doc) = self.code.as_mut() else {
                    break;
                };
                if hunk.done {
                    doc.clear_live();
                    let id = doc.add_pending(hunk.search.clone(), hunk.replace.clone());
                    self.live_done.insert(key);
                    let error = doc
                        .pending
                        .iter()
                        .find(|p| p.id == id)
                        .and_then(|p| p.error.clone());
                    match error {
                        Some(e) => {
                            doc.reject(id);
                            self.action_status.insert(
                                (msg_idx, edit.action),
                                ActionStatus::Failed(format!("{rel}: {e}")),
                            );
                        }
                        None => {
                            self.live_map.insert((rel.clone(), id), key);
                            followed = true;
                        }
                    }
                } else {
                    doc.set_live(&hunk.search, &hunk.replace, true);
                    typing_in = Some(rel.clone());
                    followed = true;
                }
            }
        }

        // Only the file that is being typed into shows live text.
        for doc in self.code.iter_mut().chain(self.parked.values_mut()) {
            if doc.is_live() && typing_in.as_deref() != Some(doc.rel.as_str()) {
                doc.clear_live();
            }
        }

        if followed {
            self.follow_typing()
        } else {
            Task::none()
        }
    }

    /// Keep the row being typed in view.
    fn follow_typing(&mut self) -> Task<Message> {
        let Some((_, last)) = self.code.as_ref().and_then(CodeDoc::live_span) else {
            return Task::none();
        };
        let y = last as f32 * ROW_H;
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
        if failed > 0 {
            self.retry_failed(msg_idx)
        } else {
            Task::none()
        }
    }

    /// Stop or error while typing: the half-written hunk disappears.
    pub(crate) fn drop_live_typing(&mut self) {
        for doc in self.code.iter_mut().chain(self.parked.values_mut()) {
            doc.clear_live();
        }
    }

    // ---- accept / reject / undo ------------------------------------------

    pub(crate) fn accept_hunk(&mut self, id: u32) {
        let Some(doc) = self.code.as_mut() else {
            return;
        };
        let rel = doc.rel.clone();
        match doc.accept(id) {
            Ok(_) => {
                let left = doc.change_count();
                self.mark(&rel, id, true);
                self.status = format!("Accepted a change in {rel} ({left} left in this file)");
                self.tidy_after_decision();
            }
            Err(e) => self.status = e,
        }
    }

    pub(crate) fn reject_hunk(&mut self, id: u32) {
        let Some(doc) = self.code.as_mut() else {
            return;
        };
        let rel = doc.rel.clone();
        doc.reject(id);
        self.mark(&rel, id, false);
        self.status = "Rejected a change".into();
        self.tidy_after_decision();
    }

    /// Accept every finished change of the open file.
    pub(crate) fn accept_file(&mut self) {
        let Some(doc) = self.code.as_mut() else {
            return;
        };
        let rel = doc.rel.clone();
        let before: Vec<u32> = doc.pending.iter().map(|p| p.id).collect();
        self.status = match doc.apply() {
            Ok(0) => "Nothing to accept".into(),
            Ok(n) => format!("Accepted {n} change(s) in {rel}"),
            Err(e) => e,
        };
        let gone: Vec<u32> = before
            .into_iter()
            .filter(|id| !doc.pending.iter().any(|p| p.id == *id))
            .collect();
        for id in gone {
            self.mark(&rel, id, true);
        }
        self.tidy_after_decision();
    }

    /// Reject every pending change of the open file.
    pub(crate) fn reject_file(&mut self) {
        let Some(doc) = self.code.as_mut() else {
            return;
        };
        let rel = doc.rel.clone();
        let ids: Vec<u32> = doc.pending.iter().map(|p| p.id).collect();
        doc.clear_all();
        for id in ids {
            self.mark(&rel, id, false);
        }
        self.status = "Rejected all pending changes in this file".into();
        self.tidy_after_decision();
    }

    /// Accept every finished change in every file.
    pub(crate) fn accept_everything(&mut self) {
        let (mut total, mut errors) = (0, Vec::new());
        let mut accepted: Vec<(String, u32)> = Vec::new();
        for doc in self.code.iter_mut().chain(self.parked.values_mut()) {
            let before: Vec<u32> = doc.pending.iter().map(|p| p.id).collect();
            match doc.apply() {
                Ok(n) => {
                    total += n;
                    for id in before {
                        if !doc.pending.iter().any(|p| p.id == id) {
                            accepted.push((doc.rel.clone(), id));
                        }
                    }
                }
                Err(e) => errors.push(e),
            }
        }
        for (rel, id) in accepted {
            self.mark(&rel, id, true);
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
        let mut rejected: Vec<(String, u32)> = Vec::new();
        for doc in self.code.iter_mut().chain(self.parked.values_mut()) {
            rejected.extend(doc.pending.iter().map(|p| (doc.rel.clone(), p.id)));
            doc.clear_all();
        }
        for (rel, id) in rejected {
            self.mark(&rel, id, false);
        }
        self.status = "Rejected all pending changes".into();
        self.tidy_after_decision();
    }

    /// Undo the last accept in the open file. The hunk it wrote comes back as pending.
    pub(crate) fn undo_accept(&mut self) -> Task<Message> {
        let Some(doc) = self.code.as_mut() else {
            return Task::none();
        };
        let rel = doc.rel.clone();
        match doc.undo_apply() {
            Ok(()) => {
                let ids: Vec<u32> = doc.pending.iter().map(|p| p.id).collect();
                for id in ids {
                    if let Some(key) = self.live_map.get(&(rel.clone(), id)).copied() {
                        self.live_done.insert(key); // it is pending again: do not stage it twice
                        self.live_accepted.remove(&key);
                        self.live_rejected.remove(&key);
                    }
                }
                self.status = format!("Undid the last accept in {rel}");
            }
            Err(e) => self.status = e,
        }
        Task::none()
    }

    /// Mutable access to the open or parked document of `rel`.
    fn doc_mut(&mut self, rel: &str) -> Option<&mut CodeDoc> {
        if self.code.as_ref().is_some_and(|d| d.rel == rel) {
            self.code.as_mut()
        } else {
            self.parked.get_mut(rel)
        }
    }

    /// True when an accepted change of this file can still be undone.
    pub fn can_revert_file(&self, rel: &str) -> bool {
        let rel = workspace::clean(rel);
        self.code
            .iter()
            .chain(self.parked.values())
            .any(|d| d.rel == rel && d.can_undo())
    }

    /// Card button "Apply": accept every finished change of that file.
    pub(crate) fn apply_file(&mut self, rel: String) {
        let rel = workspace::clean(&rel).to_string();
        if self.show_file(&rel) {
            self.accept_file();
        }
    }

    /// Card button "Revert" on a file with pending changes: reject them.
    pub(crate) fn reject_file_named(&mut self, rel: String) {
        let rel = workspace::clean(&rel).to_string();
        if self.show_file(&rel) {
            self.reject_file();
        }
    }

    /// Card button "Revert" on an applied change: undo the last accept in that file.
    pub(crate) fn revert_file(&mut self, rel: String) {
        let rel = workspace::clean(&rel).to_string();
        let result: Result<(), String> = match self.doc_mut(&rel) {
            Some(doc) => doc.undo_apply().map(|_| ()),
            None => Err(format!("{rel} is not open, nothing to revert")),
        };
        match result {
            Ok(()) => {
                // Treat what was applied as reverted so it is never staged again.
                let applied: Vec<(usize, usize)> = self.live_accepted.drain().collect();
                for key in applied {
                    self.live_done.insert(key);
                    self.live_rejected.insert(key);
                }
                self.status = format!("Reverted the last accepted change in {rel}");
                self.refresh_project();
            }
            Err(e) => self.status = e,
        }
        self.tidy_after_decision();
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
        let mut seen_live = false;
        for end in (1..=chars.len()).step_by(3).chain([chars.len()]) {
            let partial: String = chars[..end].iter().collect();
            for edit in workspace::live_edits(&partial) {
                for (h, hunk) in edit.hunks.iter().enumerate() {
                    let key = (edit.action, h);
                    if done.contains(&key) {
                        continue;
                    }
                    if hunk.done {
                        doc.clear_live();
                        doc.add_pending(hunk.search.clone(), hunk.replace.clone());
                        done.insert(key);
                    } else {
                        doc.set_live(&hunk.search, &hunk.replace, true);
                        seen_live |= doc.is_live();
                    }
                }
            }
            // the file is never touched while streaming
            assert_eq!(fs::read_to_string(dir.join("m.rs")).unwrap(), original);
        }
        doc.clear_live();
        assert!(seen_live, "the REPLACE text was visible while it was typed");
        assert_eq!(doc.pending.len(), 2);

        let hunks: Vec<workspace::Hunk> = workspace::parse_actions(reply)
            .into_iter()
            .flat_map(|a| match a {
                workspace::Action::Edit { hunks, .. } => hunks,
                _ => vec![],
            })
            .collect();
        let expected = workspace::apply_hunks(original, &hunks).unwrap();

        let ids: Vec<u32> = doc.pending.iter().map(|p| p.id).collect();
        for id in ids {
            doc.accept(id).unwrap();
        }
        assert_eq!(fs::read_to_string(dir.join("m.rs")).unwrap(), expected);
    }
}
