use super::store::Note;

/// Notes that are always sent in full when they exist.
const ALWAYS_INCLUDE: &[&str] = &["profile.md", "project/project.md"];
/// Longest piece of one section that is sent.
const MAX_SECTION_CHARS: usize = 1500;

const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "have", "what", "how", "can", "you",
    "your", "are", "was", "not", "but", "about", "into", "some", "please", "make", "need", "want",
    "use", "using", "would", "should", "could", "remember", "recall", "feature", "did", "does",
    "before", "earlier", "last", "discussed", "told", "tell", "when", "which", "there", "then",
];

/// Pick the most relevant notes and sections for `query`, joined into one block
/// of text under `budget_chars`. Keyword scoring; swap in embeddings later
/// without touching the callers.
pub fn build_context(notes: &[Note], query: &str, budget_chars: usize) -> String {
    let terms = tokenize(query);
    let mut out = String::new();

    for note in notes.iter().filter(|n| ALWAYS_INCLUDE.contains(&n.name.as_str())) {
        let header = format!("### {}\n", note.name);
        if !push_block(&mut out, &header, &note.content, budget_chars) {
            break;
        }
    }

    // (score, header, body) for every section that matches.
    let mut scored: Vec<(usize, String, String)> = Vec::new();
    for note in notes.iter().filter(|n| !ALWAYS_INCLUDE.contains(&n.name.as_str())) {
        let name = note.name.to_lowercase();
        let name_bonus = terms.iter().filter(|t| name.contains(t.as_str())).count() * 5;
        let title = note_title(&note.content);

        for (heading, body) in split_sections(&note.content) {
            let lower = format!("{heading}\n{body}").to_lowercase();
            let hits: usize = terms.iter().map(|t| lower.matches(t.as_str()).count()).sum();
            let score = hits + name_bonus;
            if score == 0 {
                continue;
            }
            let mut header = format!("### {}", note.name);
            if !title.is_empty() {
                header.push_str(&format!(" \u{2014} {title}"));
            }
            if !heading.is_empty() {
                header.push_str(&format!(" \u{203A} {heading}"));
            }
            header.push('\n');
            scored.push((score, header, excerpt(&body, &terms, MAX_SECTION_CHARS)));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0)); // stable: ties keep note and section order

    for (_, header, body) in scored {
        if !push_block(&mut out, &header, &body, budget_chars) {
            break;
        }
    }
    out.trim().to_string()
}

/// Append one block. Returns false when there is no room left.
fn push_block(out: &mut String, header: &str, body: &str, budget: usize) -> bool {
    let used = out.chars().count() + header.chars().count();
    let remaining = budget.saturating_sub(used);
    if remaining < 80 {
        return false;
    }
    let body: String = body.trim().chars().take(remaining).collect();
    out.push_str(header);
    out.push_str(&body);
    out.push_str("\n\n");
    true
}

fn note_title(content: &str) -> String {
    content
        .lines()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or("")
        .trim()
        .to_string()
}

/// Split a note at `## ` headings. Text before the first one has an empty heading.
fn split_sections(content: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut heading = String::new();
    let mut body = String::new();

    for line in content.lines() {
        if line.starts_with("## ") {
            if !(heading.is_empty() && body.trim().is_empty()) {
                out.push((heading.clone(), body.clone()));
            }
            heading = line.trim_start_matches('#').trim().to_string();
            body.clear();
        } else {
            body.push_str(line);
            body.push('\n');
        }
    }
    if !(heading.is_empty() && body.trim().is_empty()) {
        out.push((heading, body));
    }
    out
}

/// The whole text if it fits, otherwise a window around the first matching term.
fn excerpt(body: &str, terms: &[String], cap: usize) -> String {
    let chars: Vec<char> = body.chars().collect();
    if chars.len() <= cap {
        return body.to_string();
    }
    // Lowercase char by char so indexes line up with `chars`.
    let lower: String = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let at = terms
        .iter()
        .filter_map(|t| lower.find(t.as_str()))
        .min()
        .map(|byte| lower[..byte].chars().count())
        .unwrap_or(0);
    let start = at.saturating_sub(cap / 4);
    chars[start..].iter().take(cap).collect()
}

fn tokenize(text: &str) -> Vec<String> {
    let lowered = text.to_lowercase();
    let mut terms: Vec<String> = lowered
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3 && !STOP_WORDS.contains(w))
        .map(String::from)
        .collect();
    terms.sort();
    terms.dedup();
    terms
}