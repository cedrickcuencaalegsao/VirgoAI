//! Live parsing of edit blocks while the model is still writing them.
//!
//! `parse_actions` / `completed_actions` only return finished blocks. This module
//! also reads the block that is being written right now, including a REPLACE
//! section that is only half there, so the editor can show the model typing.

/// One SEARCH/REPLACE pair of an edit block.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveHunk {
    pub search: String,
    pub replace: String,
    /// `false` while the REPLACE text is still being written.
    pub done: bool,
}

/// One ```` ```edit path ```` block, finished or not.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveEdit {
    /// Index of the block among all actions of the reply. It is the index
    /// `parse_actions` gives the same block, so it can key per-action state.
    pub action: usize,
    pub path: String,
    /// Finished hunks first; at most the last one is still `done == false`.
    pub hunks: Vec<LiveHunk>,
}

use crate::{is_close, open_fence};

/// A half-typed `>>>>>>> REPLACE` line must not show up as code.
fn is_marker_prefix(t: &str) -> bool {
    !t.is_empty() && ">>>>>>> REPLACE".starts_with(t)
}

/// Every `edit` block of a (possibly unfinished) reply.
///
/// A block shows up as soon as one hunk has its SEARCH text complete. Blocks with
/// no usable hunk yet are left out.
pub fn live_edits(partial: &str) -> Vec<LiveEdit> {
    let ends_with_newline = partial.ends_with('\n');
    let mut out = Vec::new();
    let mut action = 0usize;
    let mut lines = partial.lines();

    while let Some(line) = lines.next() {
        let Some((n, info)) = open_fence(line) else {
            continue;
        };

        let mut body: Vec<&str> = Vec::new();
        let mut closed = false;
        for l in lines.by_ref() {
            if is_close(l, n) {
                closed = true;
                break;
            }
            body.push(l);
        }
        // The last line of an unfinished reply may be cut in the middle of a word.
        let cut = !closed && !ends_with_newline;

        if let Some(path) = info.strip_prefix("edit ") {
            let hunks = hunks_of(&body, cut);
            if !hunks.is_empty() {
                out.push(LiveEdit {
                    action,
                    path: path.trim().to_string(),
                    hunks,
                });
                action += 1;
            }
        } else if let Some(path) = info.strip_prefix("delete ") {
            if !path.trim().is_empty() {
                action += 1;
            }
        } else if info == "run" && !body.join("\n").trim().is_empty() {
            action += 1;
        }
    }
    out
}

fn hunks_of(body: &[&str], cut: bool) -> Vec<LiveHunk> {
    #[derive(PartialEq)]
    enum State {
        Out,
        Search,
        Replace,
    }

    let mut state = State::Out;
    let (mut search, mut replace): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let mut hunks = Vec::new();

    for (i, line) in body.iter().enumerate() {
        let t = line.trim_end();
        let last_cut = cut && i + 1 == body.len();
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
                    hunks.push(LiveHunk {
                        search: search.join("\n"),
                        replace: replace.join("\n"),
                        done: true,
                    });
                    state = State::Out;
                } else if last_cut && is_marker_prefix(t) {
                    // still typing the end marker: not code
                } else {
                    replace.push(line);
                }
            }
        }
    }

    // SEARCH is complete but REPLACE is still being written.
    if state == State::Replace {
        hunks.push(LiveHunk {
            search: search.join("\n"),
            replace: replace.join("\n"),
            done: false,
        });
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_written_replace_is_a_live_hunk() {
        let text = "Sure.\n```edit a.rs\n<<<<<<< SEARCH\nfn a() {}\n=======\nfn a() {\n    1";
        let edits = live_edits(text);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].path, "a.rs");
        assert_eq!(
            edits[0].hunks,
            vec![LiveHunk {
                search: "fn a() {}".into(),
                replace: "fn a() {\n    1".into(),
                done: false
            }]
        );
    }

    #[test]
    fn half_typed_end_marker_is_not_code() {
        let text = "```edit a.rs\n<<<<<<< SEARCH\nx\n=======\ny\n>>>>";
        let edits = live_edits(text);
        assert_eq!(edits[0].hunks[0].replace, "y");
        assert!(!edits[0].hunks[0].done);
    }

    #[test]
    fn finished_hunk_is_done_and_the_next_one_is_live() {
        let text = "```edit a.rs\n<<<<<<< SEARCH\nx\n=======\ny\n>>>>>>> REPLACE\n<<<<<<< SEARCH\np\n=======\nq";
        let hunks = &live_edits(text)[0].hunks;
        assert_eq!(hunks.len(), 2);
        assert!(hunks[0].done);
        assert_eq!(hunks[0].replace, "y");
        assert!(!hunks[1].done);
        assert_eq!(hunks[1].replace, "q");
    }

    #[test]
    fn search_still_being_written_is_not_returned() {
        let text = "```edit a.rs\n<<<<<<< SEARCH\nfn a(";
        assert!(live_edits(text).is_empty());
        let text = "```edit a.rs\n<<<<<<< SEARCH\nfn a() {}\n=====";
        assert!(live_edits(text).is_empty());
    }

    #[test]
    fn closed_block_has_only_done_hunks() {
        let text = "```edit a.rs\n<<<<<<< SEARCH\nx\n=======\ny\n>>>>>>> REPLACE\n```\n";
        let edits = live_edits(text);
        assert_eq!(edits.len(), 1);
        assert!(edits[0].hunks.iter().all(|h| h.done));
    }

    #[test]
    fn action_index_counts_delete_and_run_blocks_too() {
        let text = "```delete old.rs\n```\n```run\ncargo check\n```\n\
                    ```edit a.rs\n<<<<<<< SEARCH\nx\n=======\ny";
        let edits = live_edits(text);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].action, 2);
    }

    #[test]
    fn four_backtick_fences_work() {
        let text = "````edit a.rs\n<<<<<<< SEARCH\nx\n=======\ny\n>>>>>>> REPLACE\n````\n";
        assert_eq!(live_edits(text)[0].hunks.len(), 1);
    }

    #[test]
    fn paths_are_cleaned_like_apply_does() {
        let edits = live_edits("```edit  ./src/a.rs \n<<<<<<< SEARCH\nx\n=======\ny");
        assert_eq!(crate::clean(&edits[0].path), "src/a.rs");
    }
}
