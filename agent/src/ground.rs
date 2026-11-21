//! Grounding check: compare a model's answer with the evidence it was given.
//!
//! The model is shown a project tree, some file contents and memory notes.
//! Anything in its reply written as `inline code` that names a file or an
//! identifier must appear in that evidence. Code inside fenced blocks is
//! skipped, because new code legitimately introduces new names.
//! Std only, so it can live in the `agent` crate without new dependencies.

#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// Paths that are not in the project.
    pub unknown_paths: Vec<String>,
    /// Identifiers that appear nowhere in the evidence.
    pub unknown_symbols: Vec<String>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.unknown_paths.is_empty() && self.unknown_symbols.is_empty()
    }

    /// Markdown note appended under the reply, empty when everything checks out.
    pub fn warning(&self) -> String {
        if self.is_clean() {
            return String::new();
        }
        let mut out = String::from(
            "\n\n> **Unverified references.** These do not appear in the files, notes or request I was given, so treat them as possibly invented:",
        );
        if !self.unknown_paths.is_empty() {
            out.push_str(&format!("\n> - files: {}", join(&self.unknown_paths)));
        }
        if !self.unknown_symbols.is_empty() {
            out.push_str(&format!("\n> - names: {}", join(&self.unknown_symbols)));
        }
        out
    }
}

fn join(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("`{s}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `known_paths`: every file in the project. `evidence`: all text the model saw
/// (tree, file contents, notes, the user's request).
pub fn check(reply: &str, known_paths: &[String], evidence: &str) -> Report {
    let mut report = Report::default();
    for span in inline_spans(reply) {
        let span = span.trim();
        if span.is_empty() || span.contains(char::is_whitespace) && !span.ends_with("()") {
            continue;
        }
        if looks_like_path(span) {
            let p = span.trim_start_matches("./");
            let found = known_paths
                .iter()
                .any(|k| k == p || k.ends_with(&format!("/{p}")))
                || evidence.contains(p);
            if !found && !report.unknown_paths.contains(&p.to_string()) {
                report.unknown_paths.push(p.to_string());
            }
        } else if let Some(name) = symbol(span) {
            if !evidence.contains(&name) && !report.unknown_symbols.contains(&name) {
                report.unknown_symbols.push(name);
            }
        }
    }
    report
}

/// Contents of single-backtick spans outside fenced code blocks.
fn inline_spans(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut parts = line.split('`');
        parts.next(); // text before the first backtick
        while let (Some(code), _) = (parts.next(), parts.next()) {
            out.push(code.to_string());
        }
    }
    out
}

fn looks_like_path(s: &str) -> bool {
    const EXT: &[&str] = &[
        "rs", "toml", "md", "py", "js", "ts", "tsx", "json", "yml", "yaml", "go", "java", "c", "h",
        "cpp", "swift", "rb", "php", "sh", "css", "html", "svg", "lock",
    ];
    if s.contains("://") || s.contains(' ') || s.starts_with('/') || s.starts_with('-') {
        return false;
    }
    if s.contains('/') {
        return s.chars().all(|c| c.is_alphanumeric() || "/._-".contains(c));
    }
    match s.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && EXT.contains(&ext) && !stem.contains("::"),
        None => false,
    }
}

/// An identifier worth checking: snake_case, CamelCase, `a::b` or `f()`.
/// Plain lowercase words and short names are ignored to avoid false alarms.
fn symbol(s: &str) -> Option<String> {
    let s = s.strip_suffix("()").unwrap_or(s);
    let last = s.rsplit("::").next()?;
    if last.len() < 4
        || !s
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
        || last.chars().next()?.is_ascii_digit()
    {
        return None;
    }
    let camel = last.chars().next()?.is_uppercase() && last.chars().any(|c| c.is_lowercase())
        || last.chars().skip(1).any(|c| c.is_uppercase()) && last.chars().any(|c| c.is_lowercase());
    (last.contains('_') || camel || s.contains("::")).then(|| last.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths() -> Vec<String> {
        vec!["src/app.rs".into(), "agent/src/plan.rs".into()]
    }
    const EVIDENCE: &str = "pub fn start_scout() {}\nstruct PlanRun;\nfn finish_planning() {}";

    #[test]
    fn real_references_pass() {
        let r = check(
            "See `plan.rs` and `src/app.rs`; `PlanRun` calls `finish_planning()`.",
            &paths(),
            EVIDENCE,
        );
        assert!(r.is_clean(), "{r:?}");
    }

    #[test]
    fn invented_references_are_flagged() {
        let r = check(
            "Edit `src/magic.rs`; it uses `auto_fix_everything()` and `GhostState`.",
            &paths(),
            EVIDENCE,
        );
        assert_eq!(r.unknown_paths, vec!["src/magic.rs"]);
        assert_eq!(r.unknown_symbols, vec!["auto_fix_everything", "GhostState"]);
        assert!(r.warning().contains("Unverified"));
    }

    #[test]
    fn fenced_code_and_plain_words_are_ignored() {
        let reply =
            "Use `cargo` then:\n```rust\nfn brand_new_name() {}\n```\nUse `Vec` and `true`.";
        assert!(check(reply, &paths(), EVIDENCE).is_clean());
    }

    #[test]
    fn urls_and_flags_are_not_paths() {
        assert!(
            check(
                "See `https://x.dev/a.rs` and `--release`.",
                &paths(),
                EVIDENCE
            )
            .is_clean()
        );
    }
}
