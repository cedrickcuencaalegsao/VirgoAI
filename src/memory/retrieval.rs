use super::store::Note;

/// This note is always included when it exists.
const ALWAYS_INCLUDE: &str = "profile.md";

const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "have", "what", "how", "can", "you",
    "your", "are", "was", "not", "but", "about", "into", "some", "please", "make", "need", "want",
    "use", "using", "would", "should", "could",
];

/// Pick the notes most relevant to `query` and join them into one block of text,
/// staying under `budget_chars`. Simple keyword scoring for now; swap in
/// embeddings later without touching the callers.
pub fn build_context(notes: &[Note], query: &str, budget_chars: usize) -> String {
    let terms = tokenize(query);

    let mut chosen: Vec<&Note> = notes.iter().filter(|n| n.name == ALWAYS_INCLUDE).collect();

    let mut scored: Vec<(usize, &Note)> = notes
        .iter()
        .filter(|n| n.name != ALWAYS_INCLUDE)
        .map(|n| (score(n, &terms), n))
        .filter(|(s, _)| *s > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    chosen.extend(scored.into_iter().map(|(_, n)| n));

    let mut out = String::new();
    for note in chosen {
        let header = format!("### {}\n", note.name);
        let used = out.chars().count() + header.chars().count();
        let remaining = budget_chars.saturating_sub(used);
        if remaining < 80 {
            break;
        }
        let body: String = note.content.trim().chars().take(remaining).collect();
        out.push_str(&header);
        out.push_str(&body);
        out.push_str("\n\n");
    }
    out.trim().to_string()
}

fn score(note: &Note, terms: &[String]) -> usize {
    let content = note.content.to_lowercase();
    let name = note.name.to_lowercase();
    terms
        .iter()
        .map(|t| {
            content.matches(t.as_str()).count() + if name.contains(t.as_str()) { 5 } else { 0 }
        })
        .sum()
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
