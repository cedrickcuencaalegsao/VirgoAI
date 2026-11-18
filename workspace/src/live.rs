//! Read a model reply that may still be streaming, hunk by hunk.
//!
//! `parse_actions` only returns blocks that are finished. For live editing the UI also
//! needs the hunk being typed right now: its SEARCH text (still being located), or its
//! REPLACE text so far.

use super::{is_close, open_fence, parse_hunks};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkState {
    /// The SEARCH part is still being written.
    Locating,
    /// SEARCH is complete, REPLACE is being written.
    Writing,
    /// `>>>>>>> REPLACE` has arrived.
    Done,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveHunk {
    pub search: String,
    pub replace: String,
    pub state: HunkState,
    /// `state == HunkState::Done`, for callers that only need a flag.
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiveEdit {
    /// Index of this block among the reply's actions, the same numbering as
    /// `parse_actions(reply)`. Stable while the reply streams.
    pub action: usize,
    pub path: String,
    /// In reply order. Only the last one can be unfinished.
    pub hunks: Vec<LiveHunk>,
}

/// Every `edit` block of the reply, including an unclosed last one.
pub fn live_edits(reply: &str) -> Vec<LiveEdit> {
    let mut out = Vec::new();
    let mut lines = reply.lines();
    let ends_with_newline = reply.ends_with('\n');
    // Counts exactly the blocks `parse_actions` would return.
    let mut action = 0usize;

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

        if let Some(path) = info.strip_prefix("edit ") {
            let hunks = live_hunks(&body, closed || ends_with_newline);
            if !hunks.is_empty() {
                out.push(LiveEdit { action, path: path.trim().to_string(), hunks });
            }
            if !parse_hunks(&body).is_empty() {
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

/// Old name of `live_edits`.
pub fn parse_live(reply: &str) -> Vec<LiveEdit> {
    live_edits(reply)
}

/// "=" or ">" characters that may be the start of a marker still being typed.
fn partial_marker(line: &str, c: char) -> bool {
    !line.is_empty() && line.len() < 7 && line.chars().all(|x| x == c)
}

fn live_hunks(body: &[&str], last_line_complete: bool) -> Vec<LiveHunk> {
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
        // A half-typed marker on the last line must not flash up as code.
        let typing = i + 1 == body.len() && !last_line_complete;
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
                } else if !(typing && partial_marker(t, '=')) {
                    search.push(line);
                }
            }
            State::Replace => {
                if t.starts_with(">>>>>>>") {
                    hunks.push(LiveHunk {
                        search: search.join("\n"),
                        replace: replace.join("\n"),
                        state: HunkState::Done,
                        done: true,
                    });
                    state = State::Out;
                } else if !(typing && partial_marker(t, '>')) {
                    replace.push(line);
                }
            }
        }
    }

    match state {
        State::Search => hunks.push(LiveHunk {
            search: search.join("\n"),
            replace: String::new(),
            state: HunkState::Locating,
            done: false,
        }),
        State::Replace => hunks.push(LiveHunk {
            search: search.join("\n"),
            replace: replace.join("\n"),
            state: HunkState::Writing,
            done: false,
        }),
        State::Out => {}
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_a_hunk_from_locating_to_done() {
        let a = "Sure.\n```edit src/a.rs\n<<<<<<< SEARCH\nfn a() {";
        assert_eq!(parse_live(a)[0].hunks[0].state, HunkState::Locating);

        let b = "```edit src/a.rs\n<<<<<<< SEARCH\nfn a() {}\n=======\nfn a() { 1";
        let h = &parse_live(b)[0].hunks[0];
        assert_eq!((h.state, h.search.as_str(), h.replace.as_str()), (HunkState::Writing, "fn a() {}", "fn a() { 1"));

        let c = "```edit src/a.rs\n<<<<<<< SEARCH\nx\n=======\ny\n>>>>>>> REPLACE\n";
        assert_eq!(parse_live(c)[0].hunks[0].state, HunkState::Done);
    }

    #[test]
    fn half_typed_markers_are_not_code() {
        let t = "```edit a.rs\n<<<<<<< SEARCH\nx\n=======\ny\n>>>>";
        assert_eq!(parse_live(t)[0].hunks[0].replace, "y");
        let t = "```edit a.rs\n<<<<<<< SEARCH\nx\n===";
        assert_eq!(parse_live(t)[0].hunks[0].search, "x");
    }

    #[test]
    fn several_hunks_and_blocks() {
        let t = "```edit a.rs\n<<<<<<< SEARCH\n1\n=======\n2\n>>>>>>> REPLACE\n<<<<<<< SEARCH\n3\n=======\n4\n>>>>>>> REPLACE\n```\n```edit b.rs\n<<<<<<< SEARCH\n5";
        let e = parse_live(t);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].hunks.len(), 2);
        assert_eq!(e[1].path, "b.rs");
        assert_eq!(e[1].hunks[0].state, HunkState::Locating);
    }

    #[test]
    fn action_index_matches_parse_actions() {
        let t = "```run\nls\n```\n```edit a.rs\n<<<<<<< SEARCH\n1\n=======\n2\n>>>>>>> REPLACE\n```\n```delete x\n```\n```edit b.rs\n<<<<<<< SEARCH\n5";
        let e = live_edits(t);
        assert_eq!((e[0].action, e[1].action), (1, 3));
        assert!(e[0].hunks[0].done && !e[1].hunks[0].done);
        let full = format!("{t}\n=======\nsix\n>>>>>>> REPLACE\n```\n");
        let actions = crate::parse_actions(&full);
        assert!(matches!(&actions[3], crate::Action::Edit { path, .. } if path == "b.rs"));
        assert_eq!(live_edits(&full)[1].action, 3);
    }
}