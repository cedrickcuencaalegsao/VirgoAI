use super::{ChatMessage, Role};
use crate::plan::{self, Step};
use code::CodeDoc;
use config::Config;
use ollama::types::{ChatMsg, ChatRequest, Options};

/// How many earlier user/assistant messages are sent along as context.
const HISTORY_LIMIT: usize = 10;

const PLANNER_SYSTEM: &str = "You are the planning stage of a local assistant. \
Think carefully about the user's task and write a short numbered plan (at most 7 steps) \
that another model will follow. Do NOT carry out the task and do NOT write the final answer \
or full code. Mention any risks or missing information. Use the memory notes if they are relevant. \
Notes named project/... describe the user's current project and its past conversations: reuse the \
decisions, features and conventions recorded there. \
If project files are provided, plan around them and refer to file paths.";

const EXECUTOR_SYSTEM: &str = "You are the execution stage of a local assistant. \
Answer the user's request directly and accurately. If a plan is provided, follow it. \
Use the memory notes when they are relevant. Notes named project/... are the description of the \
user's current project and its saved past conversations. If the user asks whether you remember a \
feature, decision or earlier discussion, answer from those notes, say which note it came from, \
and say clearly when nothing matches instead of guessing. Otherwise do not mention the notes. \
If project files are provided, base your answer on them, mention file paths, and never invent files that are not listed.";

const LINE_EDIT_SYSTEM: &str = "You are a precise code editor. You are given an excerpt of a file \
and ONE target line (marked with >>>). Rewrite only that line following the instruction. If no \
instruction is given, improve the line (fix bugs, clarity, idioms) without changing what it does. \
Reply with ONLY the replacement code inside a single fenced code block, with no explanation. \
Keep the same indentation style. Use exactly one line unless the change truly needs more. \
If the line should be deleted, reply with an empty code block.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// A question or explanation: planner (optional) then a normal answer.
    Answer,
    /// A code change: planner, user reviews the plan, executor returns edit blocks.
    Change,
}

/// Change requests: the planner answers with ONE JSON object (see `plan::schema`).
/// The format rules here override the "numbered plan" wording of `PLANNER_SYSTEM`.
const PLANNER_CHANGE_SUFFIX: &str = r#"

The user wants the code changed. You are given a project map (every file with its main definitions) and the full contents of the files most relevant to the request. Plan against the real code: reuse existing functions and types by name and follow the project's conventions.

OUTPUT FORMAT (this overrides the format above). Reply with ONLY one JSON object, no prose and no code fence:
{"goal": "one sentence", "steps": [{"id": 1, "file": "exact/relative/path.rs", "is_new": false, "target": "existing fn or struct this change touches or sits next to", "anchor": "one existing line of the file near the change, copied verbatim", "change": "what changes, one or two sentences", "depends_on": [], "verify": ""}], "need_to_check": []}

Rules:
- One step = ONE file = ONE logical change. At most 7 steps, in dependency order.
- Use exact paths from the project map. Never invent files or functions.
- The anchor must be a line that really exists in the file contents shown above. Copy it exactly.
- For a file that does not exist yet set is_new to true and leave target and anchor empty.
- depends_on lists the ids of earlier steps this step needs.
- Put a shell command such as `cargo check` in verify on the LAST step only, otherwise leave it empty.
- If you need a file that was not shown, list its path in need_to_check instead of guessing."#;

const SCOUT_SYSTEM: &str = "You are the scouting stage of a coding assistant. You get a map of a \
project (files with their main definitions) and a request. Reply with ONLY the relative paths of \
the files that must be READ to plan the request: files to change, files that define types or \
functions the change touches, and one similar existing feature to copy conventions from. At most \
8 paths, one per line, exactly as written in the map. No commentary.";

const EDIT_SYSTEM: &str = "You are the execution stage of a coding agent. Every edit block you \
write is applied to the user's real files the moment the block is complete, so each block must be \
exact. Follow the plan and the request. Never paste whole files, never describe a change instead \
of making it, and do not claim a change succeeded: the app reports the real results. Put at most \
one short sentence before each block.";

const EDIT_PROTOCOL: &str = "# Editing project files\n\
Write one fenced block per file, with the info string `edit` followed by the path relative \
to the project root:\n\
````edit src/main.rs\n\
<<<<<<< SEARCH\n\
exact lines copied from the current file\n\
=======\n\
replacement lines\n\
>>>>>>> REPLACE\n\
```\n\
Rules: SEARCH must match the current file exactly (including indentation) and include \
2-3 lines of context so it is unique. You may use several SEARCH/REPLACE pairs in one block. \
To create a new file leave SEARCH empty. To delete lines leave REPLACE empty. To delete a whole \
file write a block ```delete path/to/file``` with nothing inside. Only edit files whose contents \
were provided. To verify your work you may add a block with the info string `run` containing ONE \
shell command (for example `cargo check`); the user will approve it and you will receive the output.";

/// Appended to the system prompt of a single plan step.
const STEP_RULES: &str = "# This turn: ONE plan step\n\
You are carrying out a single step of an approved plan. Write exactly ONE `edit` block, for the \
file named in the step (several SEARCH/REPLACE pairs inside that block are fine). Do not touch \
other files and do not repeat steps that are already done. SEARCH text must be copied exactly \
from the CURRENT file contents given below, which already include the earlier steps. Do not add \
`run` blocks. After the block write nothing else.";

const REPAIR_SYSTEM: &str = "You fix JSON plans for a coding assistant. You get the user's \
request, the project files, a previous plan and a list of problems found by an automatic checker. \
Return the corrected plan as ONE JSON object with the same shape as before and nothing else. \
Change only what is needed to fix the problems. Paths must exist in the project, and every anchor \
must be a line copied exactly from the file contents provided. One step = one file = one logical \
change, at most 7 steps.";

/// One search/replace block proposed by the model.
#[derive(Debug, Clone)]
pub struct Proposal {
    pub file: String,
    pub search: String,
    pub replace: String,
}

/// Decide whether a message asks for a code change.
pub fn classify(text: &str, has_project: bool) -> Intent {
    if !has_project {
        return Intent::Answer;
    }
    let t = text.trim().to_lowercase();
    const QUESTIONS: &[&str] = &[
        "what",
        "why",
        "how does",
        "how do",
        "explain",
        "where",
        "is ",
        "does ",
        "can you explain",
    ];
    const VERBS: &[&str] = &[
        "add",
        "implement",
        "refactor",
        "fix",
        "change",
        "rename",
        "create",
        "update",
        "remove",
        "delete",
        "replace",
        "rewrite",
        "move",
        "extract",
        "convert",
        "make",
    ];
    if QUESTIONS.iter().any(|q| t.starts_with(q)) {
        return Intent::Answer;
    }
    let has_verb = t
        .split(|c: char| !c.is_alphabetic())
        .any(|w| VERBS.contains(&w));
    if has_verb {
        Intent::Change
    } else {
        Intent::Answer
    }
}

/// Parse `FILE:` + SEARCH/REPLACE blocks out of the executor's reply.
/// Tolerant of markdown decoration (`**FILE:**`, backticks, `File:`), code fences
/// around the output, and bare file paths. `default_file` is used when the model
/// never names a file.
pub fn parse_edits(reply: &str, known: &[String], default_file: Option<&str>) -> Vec<Proposal> {
    let mut out = Vec::new();
    let mut file = default_file.map(str::to_string).unwrap_or_default();
    let mut mode = 0; // 0 = outside, 1 = in SEARCH, 2 = in REPLACE
    let mut search: Vec<&str> = Vec::new();
    let mut replace: Vec<&str> = Vec::new();

    for line in reply.lines() {
        let t = line.trim();
        match mode {
            0 => {
                if t.starts_with("<<<<<<<") {
                    search.clear();
                    replace.clear();
                    mode = 1;
                } else if let Some(f) = file_from_line(t, known) {
                    file = f;
                }
            }
            1 => {
                if t.starts_with("=======") {
                    mode = 2;
                } else {
                    search.push(line);
                }
            }
            _ => {
                if t.starts_with(">>>>>>>") {
                    if !file.is_empty() {
                        out.push(Proposal {
                            file: file.clone(),
                            search: search.join("\n"),
                            replace: replace.join("\n"),
                        });
                    }
                    search.clear();
                    replace.clear();
                    mode = 0;
                } else {
                    replace.push(line);
                }
            }
        }
    }
    out
}

/// A line that names a file: `FILE: x`, `**FILE:** x`, `### File: x`, or a bare known path.
fn file_from_line(t: &str, known: &[String]) -> Option<String> {
    let cleaned = t.trim_start_matches('#').replace(['*', '`'], "");
    let cleaned = cleaned.trim();
    let explicit = cleaned.to_lowercase().starts_with("file:");
    let candidate = if explicit {
        cleaned.get(5..)?.trim()
    } else {
        cleaned
    };
    let candidate = candidate
        .trim_matches(|c| c == '"' || c == '\'' || c == ':')
        .trim();
    if candidate.is_empty() {
        return None;
    }
    if let Some(k) = known.iter().find(|k| k.as_str() == candidate) {
        return Some(k.clone());
    }
    let suffix = format!("/{candidate}");
    if let Some(k) = known.iter().find(|k| k.ends_with(&suffix)) {
        return Some(k.clone());
    }
    explicit.then(|| candidate.to_string())
}

/// Everything captured when the user hits Send, kept until the executor starts.
pub struct Pending {
    pub task: String,
    pub memory: String,
    pub history: Vec<ChatMsg>,
    pub project: String,
    pub intent: Intent,
    /// Instructions of the active project (empty = none).
    pub instructions: String,
}

/// Recent user/assistant turns, newest last. Plans and errors are left out.
pub fn history_from(messages: &[ChatMessage]) -> Vec<ChatMsg> {
    let mut turns: Vec<ChatMsg> = messages
        .iter()
        .filter(|m| !m.content.trim().is_empty())
        .filter_map(|m| match m.role {
            Role::User => Some(ChatMsg::user(m.content.clone())),
            Role::Assistant => Some(ChatMsg::assistant(m.content.clone())),
            Role::Plan | Role::Error => None,
        })
        .collect();

    if turns.len() > HISTORY_LIMIT {
        let start = turns.len() - HISTORY_LIMIT;
        turns = turns.split_off(start);
    }
    turns
}

pub fn planner_request(cfg: &Config, pending: &Pending) -> ChatRequest {
    let change = pending.intent == Intent::Change;
    let mut system = String::from(PLANNER_SYSTEM);
    if change {
        system.push_str(PLANNER_CHANGE_SUFFIX);
    }
    append_instructions(&mut system, &pending.instructions);
    append_memory(&mut system, &pending.memory);
    append_project(&mut system, &pending.project);

    let mut messages = vec![ChatMsg::system(system)];
    messages.extend(pending.history.iter().cloned());
    messages.push(ChatMsg::user(pending.task.clone()));

    ChatRequest {
        model: cfg.planner_model.clone(),
        messages,
        stream: true,
        think: cfg.planner_think.then_some(true),
        // Change plans must be JSON that follows the schema. The model's own
        // temperature is kept: thinking models behave badly at 0.
        format: change.then(plan::schema),
        options: Options::new(cfg.num_ctx, None),
        keep_alive: cfg.keep_alive.clone(),
    }
}

/// Non-streaming call: ask the planner to fix a plan the checker rejected.
/// `context` is the project text the planner already saw (map + file contents).
pub fn repair_request(
    cfg: &Config,
    task: &str,
    context: &str,
    previous: &str,
    problems: &[String],
) -> ChatRequest {
    let list = problems
        .iter()
        .map(|p| format!("- {p}"))
        .collect::<Vec<_>>()
        .join("\n");
    let user = format!(
        "# Request\n{task}\n\n# Project\n{context}\n\n# Previous plan\n{previous}\n\n\
         # Problems found\n{list}\n\nReturn the corrected plan as JSON."
    );
    ChatRequest {
        model: cfg.planner_model.clone(),
        messages: vec![ChatMsg::system(REPAIR_SYSTEM), ChatMsg::user(user)],
        stream: false,
        think: Some(false),
        format: Some(plan::schema()),
        options: Options::new(cfg.num_ctx, Some(0.0)),
        keep_alive: cfg.keep_alive.clone(),
    }
}

/// What the executor needs to carry out ONE step.
pub struct StepInput<'a> {
    pub project_name: &'a str,
    pub instructions: &'a str,
    pub task: &'a str,
    pub goal: &'a str,
    pub step: &'a Step,
    pub total: usize,
    /// One line per step that is already applied.
    pub done: &'a [String],
    /// The step's file as a fenced block, read from disk right now.
    pub file_ctx: &'a str,
    /// Why the previous attempt at this step failed.
    pub retry_error: Option<&'a str>,
}

/// Streaming executor call for a single plan step. The prompt is small on purpose:
/// no history, no memory, no repo map, only this step and the file's current text.
/// The stable system part comes first so Ollama can reuse its prompt cache.
pub fn step_request(cfg: &Config, input: &StepInput) -> ChatRequest {
    let mut system = String::from(EDIT_SYSTEM);
    append_instructions(&mut system, input.instructions);
    system.push_str("\n\n");
    system.push_str(EDIT_PROTOCOL);
    system.push_str("\n\n");
    system.push_str(STEP_RULES);

    let s = input.step;
    let mut user = format!("Folder: {}\n\n# Request\n{}\n", input.project_name, input.task);
    if !input.goal.is_empty() {
        user.push_str(&format!("\n# Plan goal\n{}\n", input.goal));
    }

    user.push_str("\n# Done so far\n");
    if input.done.is_empty() {
        user.push_str("(nothing yet)\n");
    } else {
        for line in input.done {
            user.push_str(&format!("- {line}\n"));
        }
    }

    user.push_str(&format!(
        "\n# Current step {}/{}\nFile: {} ({})\n",
        s.id,
        input.total,
        s.file,
        if s.is_new { "new file" } else { "existing file" }
    ));
    if !s.target.is_empty() {
        user.push_str(&format!("Target: {}\n", s.target));
    }
    if !s.anchor.is_empty() {
        user.push_str(&format!("Anchor line: {}\n", s.anchor));
    }
    user.push_str(&format!("Change: {}\n", s.change));

    if s.is_new || input.file_ctx.trim().is_empty() {
        user.push_str(&format!(
            "\n`{}` does not exist yet: create it with an edit block whose SEARCH is empty.\n",
            s.file
        ));
    } else {
        user.push_str(&format!("\n# Current contents of {}\n{}", s.file, input.file_ctx));
    }

    if let Some(error) = input.retry_error {
        user.push_str(&format!(
            "\n# The previous attempt at this step failed\n{error}\n\
             Some blocks may already be applied. The contents above are CURRENT. \
             Write only the edit that is still missing, with SEARCH copied exactly from them.\n"
        ));
    }
    user.push_str("\nWrite the edit block for this step now.");

    ChatRequest {
        model: cfg.executor_model.clone(),
        messages: vec![ChatMsg::system(system), ChatMsg::user(user)],
        stream: true,
        think: cfg.executor_think.then_some(true),
        format: None,
        options: Options::new(cfg.num_ctx, Some(0.1)),
        keep_alive: cfg.keep_alive.clone(),
    }
}

pub fn executor_request(cfg: &Config, pending: &Pending, plan: Option<&str>) -> ChatRequest {
    let base = if pending.intent == Intent::Change {
        EDIT_SYSTEM
    } else {
        EXECUTOR_SYSTEM
    };
    let mut system = String::from(base);
    append_instructions(&mut system, &pending.instructions);
    append_memory(&mut system, &pending.memory);
    append_project(&mut system, &pending.project);
    if !pending.project.trim().is_empty() {
        system.push_str("\n\n");
        system.push_str(EDIT_PROTOCOL);
    }
    if let Some(plan) = plan {
        system.push_str("\n\n# Plan to follow\n");
        system.push_str(plan);
    }

    let mut messages = vec![ChatMsg::system(system)];
    messages.extend(pending.history.iter().cloned());
    messages.push(ChatMsg::user(pending.task.clone()));

    ChatRequest {
        model: cfg.executor_model.clone(),
        messages,
        stream: true,
        think: cfg.executor_think.then_some(true),
        format: None,
        options: Options::new(cfg.num_ctx, (pending.intent == Intent::Change).then_some(0.1)),
        keep_alive: cfg.keep_alive.clone(),
    }
}

/// Ask the executor model to rewrite one line of the open file.
pub fn line_edit_request(
    cfg: &Config,
    doc: &CodeDoc,
    line: usize,
    instruction: &str,
) -> ChatRequest {
    let target = doc.lines.get(line).map(String::as_str).unwrap_or("");
    let instruction = instruction.trim();

    let mut user = format!(
        "File: {} ({})\n\nExcerpt:\n{}\nTarget line {}:\n```\n{}\n```\n\n",
        doc.rel,
        doc.lang(),
        doc.context_around(line, 12),
        line + 1,
        target
    );
    if instruction.is_empty() {
        user.push_str("Instruction: improve this line.");
    } else {
        user.push_str(&format!("Instruction: {instruction}"));
    }

    ChatRequest {
        model: cfg.executor_model.clone(),
        messages: vec![ChatMsg::system(LINE_EDIT_SYSTEM), ChatMsg::user(user)],
        stream: false,
        think: cfg.executor_think.then_some(true),
        format: None,
        options: Options::new(cfg.num_ctx, Some(0.1)),
        keep_alive: cfg.keep_alive.clone(),
    }
}

/// Pull the replacement lines out of the model's reply (a fenced code block).
/// An empty block means "delete the line".
pub fn parse_line_edit(reply: &str, original: &str) -> Result<Vec<String>, String> {
    let text = strip_think_tags(reply);

    let lines: Vec<&str> = if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        // Skip the rest of the opening fence line (the language tag).
        let body = after.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
        let body = match body.find("```") {
            Some(end) => &body[..end],
            None => body,
        };
        body.lines().collect()
    } else {
        let trimmed = text.trim();
        if trimmed.lines().count() > 3 {
            return Err("The model did not return a code block".into());
        }
        trimmed.lines().collect()
    };

    let mut replacement: Vec<String> = lines.iter().map(|l| l.trim_end().to_string()).collect();
    while replacement.last().is_some_and(|l| l.is_empty()) {
        replacement.pop();
    }
    while replacement.first().is_some_and(|l| l.is_empty()) {
        replacement.remove(0);
    }
    Ok(fix_indent(original, replacement))
}

/// Models often drop the leading indentation of a single line: restore it.
fn fix_indent(original: &str, replacement: Vec<String>) -> Vec<String> {
    let indent: String = original.chars().take_while(|c| c.is_whitespace()).collect();
    if indent.is_empty() {
        return replacement;
    }
    let first_indented = replacement
        .first()
        .map_or(true, |l| l.is_empty() || l.starts_with(char::is_whitespace));
    if first_indented {
        return replacement;
    }
    replacement
        .into_iter()
        .map(|l| {
            if l.is_empty() {
                l
            } else {
                format!("{indent}{l}")
            }
        })
        .collect()
}

fn append_memory(system: &mut String, memory: &str) {
    if !memory.trim().is_empty() {
        system.push_str("\n\n# Memory notes (from the user's local files)\n");
        system.push_str(memory);
    }
}

fn append_instructions(system: &mut String, instructions: &str) {
    if !instructions.trim().is_empty() {
        system.push_str("\n\n# Project instructions (always follow these)\n");
        system.push_str(instructions.trim());
    }
}

fn append_project(system: &mut String, project: &str) {
    if !project.trim().is_empty() {
        system.push_str(
            "\n\n# Project (the user's open folder)\n\
             You CAN see this project. Below is its file tree. File contents appear \
             under '## File contents' only for files the user attached or @mentioned. \
             If you need a file that is not shown, ask the user to attach it or \
             @mention it by path. Never say you cannot access the project.\n\n",
        );
        system.push_str(project);
    }
}

/// Remove `<think>...</think>` blocks that some models put inline in their reply.
pub fn strip_think_tags(text: &str) -> String {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";

    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        match rest[start..].find(CLOSE) {
            Some(end) => rest = &rest[start + end + CLOSE.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// Non-streaming call: the planner picks which files to read.
pub fn scout_request(cfg: &Config, pending: &Pending) -> ChatRequest {
    let user = format!(
        "# Project map\n{}\n\n# Request\n{}\n\nWhich files must be read to plan this?",
        pending.project, pending.task
    );
    ChatRequest {
        model: cfg.planner_model.clone(),
        messages: vec![ChatMsg::system(SCOUT_SYSTEM), ChatMsg::user(user)],
        stream: false,
        think: Some(false), // a path list needs no long reasoning
        format: None,
        options: Options::new(cfg.num_ctx, Some(0.0)),
        keep_alive: cfg.keep_alive.clone(),
    }
}

/// Paths from the scout reply. The caller checks that they exist.
pub fn parse_scout(reply: &str) -> Vec<String> {
    strip_think_tags(reply)
        .lines()
        .map(|l| {
            let mut s = l
                .trim()
                .trim_start_matches(|c| c == '-' || c == '*' || c == ' ');
            if let Some((n, rest)) = s.split_once(". ") {
                if n.chars().all(|c| c.is_ascii_digit()) {
                    s = rest;
                }
            }
            s.trim().trim_matches('`').to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::plan;

//     fn cfg() -> Config {
//         Config {
//             planner_model: "planner".into(),
//             executor_model: "executor".into(),
//             num_ctx: 8192,
//             keep_alive: "10m".into(),
//             planner_think: true,
//             executor_think: false,
//         }
//     }

//     fn pending(intent: Intent) -> Pending {
//         Pending {
//             task: "add a clear button".into(),
//             memory: String::new(),
//             history: Vec::new(),
//             project: "Folder: demo".into(),
//             intent,
//             instructions: String::new(),
//         }
//     }

//     #[test]
//     fn change_plans_use_the_schema_answers_do_not() {
//         let change = planner_request(&cfg(), &pending(Intent::Change));
//         assert!(change.format.is_some());
//         assert!(change.messages[0].content.contains("OUTPUT FORMAT"));
//         let answer = planner_request(&cfg(), &pending(Intent::Answer));
//         assert!(answer.format.is_none());
//         assert!(!answer.messages[0].content.contains("OUTPUT FORMAT"));
//     }

//     #[test]
//     fn request_json_skips_empty_optional_fields() {
//         let answer = serde_json::to_value(planner_request(&cfg(), &pending(Intent::Answer))).unwrap();
//         assert!(answer.get("format").is_none());
//         assert!(answer["options"].get("temperature").is_none());
//         let repair = serde_json::to_value(repair_request(&cfg(), "t", "ctx", "{}", &["bad".into()])).unwrap();
//         assert!(repair["format"]["properties"]["steps"].is_object());
//         assert_eq!(repair["options"]["temperature"], 0.0);
//         assert_eq!(repair["stream"], false);
//     }

//     #[test]
//     fn repair_prompt_lists_the_problems_and_the_context() {
//         let r = repair_request(&cfg(), "task", "FILE TEXT", "{old}", &["Step 1: bad path".into()]);
//         let user = &r.messages[1].content;
//         assert!(user.contains("- Step 1: bad path"));
//         assert!(user.contains("FILE TEXT"));
//         assert!(user.contains("{old}"));
//     }

//     fn sample_step() -> Step {
//         Step {
//             id: 2,
//             file: "src/ui.rs".into(),
//             is_new: false,
//             target: "fn view".into(),
//             anchor: "pub fn view(app: &App)".into(),
//             change: "add the button".into(),
//             depends_on: vec![1],
//             verify: None,
//         }
//     }

//     #[test]
//     fn step_prompt_is_scoped_to_one_step() {
//         let step = sample_step();
//         let done = vec!["1. src/msg.rs: add Clear variant".to_string()];
//         let input = StepInput {
//             project_name: "demo",
//             instructions: "",
//             task: "add a clear button",
//             goal: "Clear button",
//             step: &step,
//             total: 3,
//             done: &done,
//             file_ctx: "### src/ui.rs\n```rust\nfn view() {}\n```\n\n",
//             retry_error: None,
//         };
//         let r = step_request(&cfg(), &input);
//         assert_eq!(r.model, "executor");
//         assert!(r.stream && r.format.is_none());
//         assert_eq!(r.messages.len(), 2); // system + user, no history
//         assert!(r.messages[0].content.contains("ONE plan step"));
//         let user = &r.messages[1].content;
//         assert!(user.contains("# Current step 2/3"));
//         assert!(user.contains("- 1. src/msg.rs: add Clear variant"));
//         assert!(user.contains("Anchor line: pub fn view(app: &App)"));
//         assert!(user.contains("# Current contents of src/ui.rs"));
//         assert!(!user.contains("previous attempt"));
//     }

//     #[test]
//     fn step_prompt_handles_new_files_and_retries() {
//         let mut step = sample_step();
//         step.is_new = true;
//         let input = StepInput {
//             project_name: "demo",
//             instructions: "Use tabs.",
//             task: "t",
//             goal: "",
//             step: &step,
//             total: 1,
//             done: &[],
//             file_ctx: "",
//             retry_error: Some("SEARCH text not found"),
//         };
//         let r = step_request(&cfg(), &input);
//         assert!(r.messages[0].content.contains("Use tabs."));
//         let user = &r.messages[1].content;
//         assert!(user.contains("(nothing yet)"));
//         assert!(user.contains("SEARCH is empty"));
//         assert!(user.contains("SEARCH text not found"));
//     }

//     #[test]
//     fn a_plan_parsed_from_the_planner_reply_survives_the_round_trip() {
//         let reply = r#"{"goal":"g","steps":[{"id":1,"file":"a.rs","is_new":true,"target":"","anchor":"","change":"make it"}]}"#;
//         let plan = plan::parse(reply).unwrap();
//         assert_eq!(plan.steps[0].file, "a.rs");
//     }
// }