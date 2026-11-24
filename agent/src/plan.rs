//! Structured plans.
//!
//! The planner returns JSON that is parsed into a [`Plan`], checked against the
//! real project files by [`validate`], and then executed one [`Step`] at a time.
//! [`PlanRun`] is the small state machine that tracks which step is running, done
//! or failed, so progress lives in Rust and not in the model's head.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use crate::pipeline::strip_think_tags;

/// More steps than this and the planner is asked to merge related changes.
pub const MAX_STEPS: usize = 7;
/// Attempts per step after the first one.
pub const MAX_STEP_TRIES: u8 = 2;

/// Models sometimes send `null` for a field: treat it as the default value.
fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Plan {
    #[serde(default, deserialize_with = "null_default")]
    pub goal: String,
    #[serde(default, deserialize_with = "null_default")]
    pub steps: Vec<Step>,
    /// Files the planner wanted to see but was not given.
    #[serde(default, deserialize_with = "null_default")]
    pub need_to_check: Vec<String>,
}

/// One step = one file = one logical change.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Step {
    #[serde(default, deserialize_with = "null_default")]
    pub id: u32,
    /// Exact path relative to the project root.
    #[serde(default, deserialize_with = "null_default")]
    pub file: String,
    #[serde(default, deserialize_with = "null_default")]
    pub is_new: bool,
    /// An EXISTING function, struct or section the change touches or sits next to.
    #[serde(default, deserialize_with = "null_default")]
    pub target: String,
    /// One existing line of the file near the change, copied verbatim.
    #[serde(default, deserialize_with = "null_default")]
    pub anchor: String,
    #[serde(default, deserialize_with = "null_default")]
    pub change: String,
    #[serde(default, deserialize_with = "null_default")]
    pub depends_on: Vec<u32>,
    /// A shell command that checks the result, e.g. `cargo check`.
    #[serde(default)]
    pub verify: Option<String>,
    /// Exact names of functions, structs, fields or variants this step touches.
    #[serde(default, deserialize_with = "null_default")]
    pub symbols: Vec<String>,
    /// Ordered, concrete instructions for the executor.
    #[serde(default, deserialize_with = "null_default")]
    pub details: Vec<String>,
}

/// JSON schema for Ollama's `format` field.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "goal": { "type": "string" },
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer" },
                        "file": { "type": "string" },
                        "is_new": { "type": "boolean" },
                        "target": { "type": "string" },
                        "anchor": { "type": "string" },
                        "change": { "type": "string" },
                        "depends_on": { "type": "array", "items": { "type": "integer" } },
                        "verify": { "type": "string" },
                        "symbols": { "type": "array", "items": { "type": "string" } },
                        "details": { "type": "array", "items": { "type": "string" } },
                    },
                    "required": ["id", "file", "is_new", "target", "anchor", "change"]
                }
            },
            "need_to_check": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["goal", "steps"]
    })
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Read a plan out of a model reply. Tolerates `<think>` blocks, code fences and
/// prose around the JSON object.
pub fn parse(reply: &str) -> Result<Plan, String> {
    let text = strip_think_tags(reply);
    let json = extract_json(&text).ok_or("no JSON object found in the reply")?;
    let mut plan: Plan =
        serde_json::from_str(json).map_err(|e| format!("invalid plan JSON: {e}"))?;
    normalize(&mut plan);
    if plan.steps.is_empty() {
        return Err("the plan has no steps".into());
    }
    Ok(plan)
}

fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| &text[start..=end])
}

fn clean_path(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c| c == '`' || c == '"' || c == '\'')
        .trim_start_matches("./")
        .to_string()
}

/// Renumber the steps 1..n and keep only dependencies that point at earlier steps.
fn normalize(plan: &mut Plan) {
    plan.goal = plan.goal.trim().to_string();

    // Drop steps that repeat an earlier one (same file + same change).
    let mut seen_steps: std::collections::HashSet<(String, String)> = Default::default();
    plan.steps
        .retain(|s| seen_steps.insert((clean_path(&s.file), s.change.trim().to_lowercase())));

    let mut renumber: HashMap<u32, u32> = HashMap::new();
    for (i, s) in plan.steps.iter().enumerate() {
        renumber.entry(s.id).or_insert(i as u32 + 1);
    }

    for (i, s) in plan.steps.iter_mut().enumerate() {
        let new_id = i as u32 + 1;
        s.file = clean_path(&s.file);
        s.target = s.target.trim().to_string();
        s.anchor = s
            .anchor
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .to_string();
        s.change = s.change.trim().to_string();

        let mut seen_sym = std::collections::HashSet::new();
        s.symbols
            .retain(|x| !x.trim().is_empty() && seen_sym.insert(x.trim().to_string()));

        let mut seen_det = std::collections::HashSet::new();
        s.details = s
            .details
            .iter()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty())
            .filter(|d| seen_det.insert(d.to_lowercase()))
            .collect();

        s.verify = s
            .verify
            .take()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        let mut deps: Vec<u32> = s
            .depends_on
            .iter()
            .filter_map(|d| renumber.get(d).copied())
            .filter(|d| *d < new_id)
            .collect();
        deps.sort_unstable();
        deps.dedup();
        s.depends_on = deps;
        s.id = new_id;
    }

    let mut seen_need = std::collections::HashSet::new();
    plan.need_to_check = plan
        .need_to_check
        .iter()
        .map(|f| clean_path(f))
        .filter(|f| !f.is_empty())
        .filter(|f| seen_need.insert(f.clone()))
        .collect();
}

// ---------------------------------------------------------------------------
// Validation against the real project
// ---------------------------------------------------------------------------

/// Check a plan against the project. An empty list means the plan is grounded:
/// every path exists (or is marked new) and every anchor is a real line.
///
/// `read` returns a file's text, `exists` says whether a project file exists.
pub fn validate(
    plan: &Plan,
    read: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    let mut errors = Vec::new();

    if plan.steps.is_empty() {
        errors.push("The plan has no steps.".to_string());
    }
    if plan.steps.len() > MAX_STEPS {
        errors.push(format!(
            "The plan has {} steps. Use at most {MAX_STEPS} by merging related changes.",
            plan.steps.len()
        ));
    }

    for s in &plan.steps {
        let n = s.id;
        if s.file.is_empty() || s.file.starts_with('/') || s.file.split('/').any(|p| p == "..") {
            errors.push(format!(
                "Step {n}: `{}` is not a safe relative path.",
                s.file
            ));
            continue;
        }
        if s.change.is_empty() {
            errors.push(format!("Step {n}: `change` is empty."));
        }

        if s.details.len() < 2 || s.details.iter().all(|d| d.split_whitespace().count() < 6) {
            errors.push(format!(
                "Step {n}: `details` is too vague. Give 2-8 concrete instructions naming exact \
         functions, structs, fields or variants and the signature of anything new."
            ));
        }
        if s.target == s.file {
            errors.push(format!(
                "Step {n}: `target` must be a symbol name, not the file path."
            ));
        }

        if s.is_new {
            if exists(&s.file) {
                errors.push(format!(
                    "Step {n}: `{}` already exists, so is_new must be false.",
                    s.file
                ));
            }
            continue;
        }

        if !exists(&s.file) {
            errors.push(format!(
                "Step {n}: file `{}` is not in the project. Use a path from the project map, \
                 or set is_new to true to create it.",
                s.file
            ));
            continue;
        }
        let Some(text) = read(&s.file) else {
            errors.push(format!("Step {n}: `{}` cannot be read as text.", s.file));
            continue;
        };

        for sym in &s.symbols {
            if !text.contains(sym.as_str()) {
                errors.push(format!(
                    "Step {n}: symbol `{sym}` does not exist in `{}`.",
                    s.file
                ));
            }
        }

        if s.anchor.is_empty() && s.target.is_empty() {
            errors.push(format!(
                "Step {n}: `{}` needs a target or an anchor (an existing line near the change).",
                s.file
            ));
        } else if !s.anchor.is_empty() {
            if !text.contains(s.anchor.as_str()) {
                errors.push(format!(
                    "Step {n}: anchor `{}` was not found in `{}`. Copy one existing line exactly.",
                    s.anchor, s.file
                ));
            }
        } else if !mentions_target(&text, &s.target) {
            errors.push(format!(
                "Step {n}: target `{}` was not found in `{}`.",
                s.target, s.file
            ));
        }
    }
    errors
}

/// Heuristic: the longest identifier-like word of `target` must occur in the file.
fn mentions_target(text: &str, target: &str) -> bool {
    const NOISE: &[&str] = &[
        "fn", "pub", "struct", "enum", "impl", "trait", "mod", "async", "def", "class", "function",
        "const", "static", "let", "type", "method", "the", "in", "of", "new", "add", "an", "to",
        "for", "and", "or", "with", "field", "variant", "import", "use",
    ];
    let longest = target
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| t.len() >= 3 && !NOISE.contains(&t.to_lowercase().as_str()))
        .max_by_key(|t| t.len());
    match longest {
        Some(word) => text.contains(word),
        None => true,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// The plan as markdown for the chat bubble and the saved conversation.
pub fn render_markdown(plan: &Plan) -> String {
    let mut out = String::new();
    if !plan.goal.is_empty() {
        out.push_str(&format!("**Goal:** {}\n\n", plan.goal));
    }
    for s in &plan.steps {
        let kind = if s.is_new { "new file" } else { "edit" };
        out.push_str(&format!("{}. `{}` ({kind}): {}\n", s.id, s.file, s.change));
        if !s.target.is_empty() {
            out.push_str(&format!("- target: `{}`\n", s.target));
        }
        if !s.anchor.is_empty() {
            out.push_str(&format!("- anchor: `{}`\n", s.anchor));
        }
        if let Some(v) = &s.verify {
            out.push_str(&format!("- verify: `{v}`\n"));
        }
    }
    if !plan.need_to_check.is_empty() {
        out.push_str(&format!(
            "\n**Needs checking:** {}\n",
            plan.need_to_check.join(", ")
        ));
    }
    out.trim_end().to_string()
}

/// The plan as editable JSON (shown in the review box).
pub fn to_pretty_json(plan: &Plan) -> String {
    serde_json::to_string_pretty(plan).unwrap_or_default()
}

fn short(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max).collect();
        format!("{head}...")
    }
}

// ---------------------------------------------------------------------------
// Running a plan
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum StepStatus {
    Todo,
    Running,
    Done,
    Failed(String),
    Skipped,
}

/// Progress of one plan. Owned by the app while the plan executes.
#[derive(Debug, Clone)]
pub struct PlanRun {
    pub plan: Plan,
    pub status: Vec<StepStatus>,
    /// Index of the step whose executor call is in flight.
    pub cur: Option<usize>,
    /// Retries already used for the current step.
    pub tries: u8,
    /// The user's request and the project instructions, reused for every step.
    pub task: String,
    pub instructions: String,
    /// Why the previous attempt of the next step failed (shown to the model).
    pub last_error: Option<String>,
    /// Problems the validator could not get fixed.
    pub warnings: Vec<String>,
}

impl PlanRun {
    pub fn new(plan: Plan, task: String, instructions: String, warnings: Vec<String>) -> Self {
        let status = vec![StepStatus::Todo; plan.steps.len()];
        Self {
            plan,
            status,
            cur: None,
            tries: 0,
            task,
            instructions,
            last_error: None,
            warnings,
        }
    }

    fn deps_met(&self, idx: usize) -> bool {
        self.plan.steps[idx].depends_on.iter().all(|d| {
            let dep = (*d as usize).saturating_sub(1);
            matches!(
                self.status.get(dep),
                Some(StepStatus::Done | StepStatus::Skipped)
            )
        })
    }

    /// The first step that is waiting and whose dependencies are met.
    pub fn next_todo(&self) -> Option<usize> {
        (0..self.status.len()).find(|&i| self.status[i] == StepStatus::Todo && self.deps_met(i))
    }

    pub fn has_todo(&self) -> bool {
        self.status.contains(&StepStatus::Todo)
    }

    pub fn failed(&self) -> Option<usize> {
        self.status
            .iter()
            .position(|s| matches!(s, StepStatus::Failed(_)))
    }

    pub fn count(&self, pred: impl Fn(&StepStatus) -> bool) -> usize {
        self.status.iter().filter(|s| pred(s)).count()
    }

    pub fn begin(&mut self, idx: usize) {
        self.status[idx] = StepStatus::Running;
        self.cur = Some(idx);
    }

    pub fn mark_done(&mut self) {
        if let Some(i) = self.cur.take() {
            self.status[i] = StepStatus::Done;
        }
        self.tries = 0;
        self.last_error = None;
    }

    /// Put the running step back in the queue with the reason it failed.
    pub fn retry(&mut self, error: String) {
        if let Some(i) = self.cur.take() {
            self.status[i] = StepStatus::Todo;
        }
        self.tries += 1;
        self.last_error = Some(error);
    }

    pub fn mark_failed(&mut self, error: String) {
        if let Some(i) = self.cur.take() {
            self.status[i] = StepStatus::Failed(error);
        }
    }

    /// Stop or stream error while a step runs: keep it as failed so it can be retried.
    pub fn interrupt(&mut self, why: &str) {
        if self.cur.is_some() {
            self.mark_failed(why.to_string());
        }
    }

    /// User pressed Retry: the failed step goes back to the queue.
    pub fn reopen_failed(&mut self) -> bool {
        let Some(i) = self.failed() else { return false };
        if let StepStatus::Failed(e) = std::mem::replace(&mut self.status[i], StepStatus::Todo) {
            self.last_error = Some(e);
        }
        self.tries = 0;
        true
    }

    /// User pressed Skip.
    pub fn skip_failed(&mut self) -> bool {
        let Some(i) = self.failed() else { return false };
        self.status[i] = StepStatus::Skipped;
        self.last_error = None;
        self.tries = 0;
        true
    }

    /// One line per finished step, for the "done so far" part of the next prompt.
    pub fn done_lines(&self) -> Vec<String> {
        self.plan
            .steps
            .iter()
            .zip(&self.status)
            .filter(|(_, st)| **st == StepStatus::Done)
            .map(|(s, _)| format!("{}. {}: {}", s.id, s.file, short(&s.change, 100)))
            .collect()
    }

    /// The last verify command in the plan, if any.
    pub fn verify_command(&self) -> Option<String> {
        self.plan.steps.iter().rev().find_map(|s| s.verify.clone())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const REPLY: &str = r#"<think>hmm</think>
Here is the plan:
```json
{
  "goal": "Add a clear button",
  "steps": [
    {"id": 10, "file": "./src/ui.rs", "is_new": false, "target": "fn view", "anchor": "pub fn view(app: &App)", "change": "add the button", "depends_on": [], "verify": ""},
    {"id": 20, "file": "src/msg.rs", "is_new": false, "target": null, "anchor": "pub enum Message {", "change": "add Clear variant", "depends_on": [10, 99, 20], "verify": "cargo check"}
  ],
  "details": ["Add a Clear button next to the send button", "Emit Message::Clear on press"], "symbols": []
}
```"#;

    fn files() -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("src/ui.rs", "use x;\npub fn view(app: &App) -> E {\n}\n"),
            ("src/msg.rs", "pub enum Message {\n    A,\n}\n"),
        ])
    }

    fn check(plan: &Plan) -> Vec<String> {
        let f = files();
        validate(plan, &|p| f.get(p).map(|s| s.to_string()), &|p| {
            f.contains_key(p)
        })
    }

    #[test]
    fn parses_fenced_json_with_think_and_nulls() {
        let plan = parse(REPLY).unwrap();
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].file, "src/ui.rs");
        assert_eq!(plan.steps[0].verify, None); // "" became None
        assert_eq!(plan.steps[1].target, ""); // null became ""
        assert_eq!(plan.steps[1].verify.as_deref(), Some("cargo check"));
    }

    #[test]
    fn ids_are_renumbered_and_dependencies_cleaned() {
        let plan = parse(REPLY).unwrap();
        assert_eq!(plan.steps[0].id, 1);
        assert_eq!(plan.steps[1].id, 2);
        // 10 -> 1 is kept, 99 is unknown, 20 is the step itself.
        assert_eq!(plan.steps[1].depends_on, vec![1]);
    }

    #[test]
    fn rejects_garbage_and_empty_plans() {
        assert!(parse("no json here").is_err());
        assert!(parse(r#"{"goal": "x", "steps": []}"#).is_err());
    }

    #[test]
    fn grounded_plan_has_no_errors() {
        assert!(check(&parse(REPLY).unwrap()).is_empty());
    }

    #[test]
    fn validator_catches_bad_paths_and_anchors() {
        let mut plan = parse(REPLY).unwrap();
        plan.steps[0].file = "src/nope.rs".into();
        plan.steps[1].anchor = "pub enum Missing {".into();
        let errors = check(&plan);
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].contains("not in the project"));
        assert!(errors[1].contains("anchor"));
    }

    #[test]
    fn new_file_rules() {
        let mut plan = parse(REPLY).unwrap();
        plan.steps[0].is_new = true; // but src/ui.rs exists
        assert!(check(&plan)[0].contains("already exists"));
        plan.steps[0].file = "src/brand_new.rs".into();
        assert!(check(&plan).is_empty());
    }

    #[test]
    fn unsafe_paths_and_too_many_steps() {
        let mut plan = parse(REPLY).unwrap();
        plan.steps[0].file = "../secret".into();
        assert!(check(&plan)[0].contains("safe relative path"));

        let step = plan.steps[1].clone();
        plan.steps = vec![step; MAX_STEPS + 1];
        assert!(check(&plan).iter().any(|e| e.contains("at most")));
    }

    #[test]
    fn target_without_anchor_uses_the_longest_word() {
        let mut plan = parse(REPLY).unwrap();
        plan.steps[0].anchor.clear();
        plan.steps[0].target = "fn view".into(); // "view" is in the file
        assert!(check(&plan).is_empty());
        plan.steps[0].target = "fn render_everything".into();
        assert!(check(&plan)[0].contains("target"));
    }

    #[test]
    fn run_walks_the_steps_in_order() {
        let plan = parse(REPLY).unwrap();
        let mut run = PlanRun::new(plan, "task".into(), String::new(), vec![]);

        assert_eq!(run.next_todo(), Some(0));
        run.begin(0);
        assert_eq!(run.next_todo(), None); // step 0 is running, step 1 waits for it
        run.mark_done();
        assert_eq!(run.done_lines().len(), 1);

        assert_eq!(run.next_todo(), Some(1));
        run.begin(1);
        run.retry("SEARCH not found".into());
        assert_eq!(run.tries, 1);
        assert_eq!(run.last_error.as_deref(), Some("SEARCH not found"));
        assert_eq!(run.next_todo(), Some(1)); // same step again

        run.begin(1);
        run.mark_failed("still broken".into());
        assert_eq!(run.failed(), Some(1));
        assert!(run.reopen_failed());
        assert_eq!(run.next_todo(), Some(1));

        run.begin(1);
        run.mark_done();
        assert_eq!(run.next_todo(), None);
        assert!(!run.has_todo());
        assert_eq!(run.verify_command().as_deref(), Some("cargo check"));
    }

    #[test]
    fn skipped_dependency_unblocks_and_failed_one_blocks() {
        let plan = parse(REPLY).unwrap(); // step 2 depends on step 1
        let mut run = PlanRun::new(plan, String::new(), String::new(), vec![]);
        run.begin(0);
        run.mark_failed("x".into());
        assert_eq!(run.next_todo(), None); // blocked by the failed step
        assert!(run.has_todo());
        assert!(run.skip_failed());
        assert_eq!(run.next_todo(), Some(1));
    }

    #[test]
    fn interrupt_keeps_the_step_retryable() {
        let plan = parse(REPLY).unwrap();
        let mut run = PlanRun::new(plan, String::new(), String::new(), vec![]);
        run.begin(0);
        run.interrupt("Stopped");
        assert!(matches!(run.status[0], StepStatus::Failed(_)));
        assert!(run.cur.is_none());
        run.interrupt("again"); // nothing running: no-op
    }

    #[test]
    fn markdown_and_json_round_trip() {
        let plan = parse(REPLY).unwrap();
        let md = render_markdown(&plan);
        assert!(md.contains("**Goal:** Add a clear button"));
        assert!(md.contains("1. `src/ui.rs` (edit): add the button"));
        let again = parse(&to_pretty_json(&plan)).unwrap();
        assert_eq!(again, plan);
    }

    #[test]
    fn schema_lists_required_fields() {
        let s = schema();
        assert_eq!(s["required"][0], "goal");
        assert!(s["properties"]["steps"]["items"]["properties"]["anchor"].is_object());
    }
}
