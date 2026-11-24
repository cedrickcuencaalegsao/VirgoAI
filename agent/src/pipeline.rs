use super::{ChatMessage, Role};
use crate::plan::{self, Step};
use code::CodeDoc;
use config::Config;
use ollama::types::{ChatMsg, ChatRequest, Options};

/// How many earlier user/assistant messages are sent along as context.
const HISTORY_LIMIT: usize = 10;
const MAX_PLAN_TOKENS: i32 = 6144; // planner thinking counts too
const MAX_ANSWER_TOKENS: i32 = 4096;
const MAX_STEP_TOKENS: i32 = 2048;
const MAX_SMALL_TOKENS: i32 = 512;
const MAX_LINE_TOKENS: i32 = 1024;

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

const GROUNDING: &str = "\n\n# Grounding rules\n\
- Only state facts found in the provided files, memory notes or request. \
Never invent file paths, functions, fields, versions or APIs.\n\
- Write file paths and identifiers in `backticks` only if you saw them in the provided text.\n\
- If something you need is not provided, say \"I don't see X in the provided files\" \
and ask the user to attach or @mention it. Do not guess.\n\
- Separate what the files show from your suggestions.";

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

The user wants the code changed. You are given a project map, the full contents of some files, and for large files an OUTLINE (line: definition). Plan against the real code, reuse existing names, follow the project's conventions.

OUTPUT FORMAT (overrides the format above). Reply with ONLY one JSON object, no prose, no code fence:
{"goal": "one sentence", "steps": [{"id": 1, "file": "exact/path.rs", "is_new": false, "target": "existing fn or struct name", "anchor": "one real code line copied verbatim", "symbols": ["App", "Message"], "details": ["Cut the fields `input` and `messages` out of `struct App`", "Add `pub struct ChatState { pub input: String, pub messages: Vec<ChatMessage> }` with `#[derive(Default)]`"], "change": "one-sentence summary", "depends_on": [], "verify": "cargo check"}], "need_to_check": []}

Rules:
- `details`: 2-8 imperative instructions. Each names identifiers that appear in the code shown and gives the signature of anything new. Never write "organize", "improve" or "refactor" without saying exactly what moves where.
- `anchor` is a line of code, never a heading or number. `target` is a symbol, never a path.
- One step = ONE file = ONE edit block, at most about 60 changed lines.
- If the request needs more than 7 steps, plan ONLY phase 1 and start `goal` with "Phase 1 of N:".
- Every step must leave the project compiling. For moves: step 1 creates the new file (is_new), step 2 adds `mod x;` and switches callers, step 3 removes the old code.
- Put `cargo check` in `verify` on every step that changes Rust code.
- `need_to_check` holds file paths only.
- Use exact paths from the project map. Never invent files or functions.
"#;

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
Write one fenced block per file. Open the block with three backticks, the word `edit`, a space \
and the path relative to the project root. Close it with three backticks. Example:\n\
````````edit src/main.rs\n\
<<<<<<< SEARCH\n\
exact lines copied from the current file\n\
=======\n\
replacement lines\n\
>>>>>>> REPLACE\n\
```````\n\
Rules:\n\
- ALWAYS include the three marker lines `<<<<<<< SEARCH`, `=======` and `>>>>>>> REPLACE`. \
A block without them is ignored.\n\
- The FIRST line of a block must be exactly three backticks immediately followed by `edit` \
and the path, for example ```edit tester/Cargo.toml . Never put the word `edit` on its own line \
inside a block, and never open a block with bare backticks.\n\
- A command goes in its own block: ```run on the first line, the command on the second line, \
then closing backticks. Never write a command as plain text.\n\
- Do not write a file path as a heading above the block.\n\
- SEARCH must match the current file exactly (including indentation) and include 2-3 lines \
of context so it is unique. You may use several SEARCH/REPLACE pairs in one block.\n\
- Write every line ONCE. Never repeat a line or a group of lines you already wrote. \
Do not copy the SEARCH lines into REPLACE more than once.\n\
- When the last REPLACE is done, write `>>>>>>> REPLACE`, close the fence and STOP.\n\
- To create a new file leave SEARCH empty. To delete lines leave REPLACE empty.\n\
- To delete a whole file write a block ```delete path/to/file``` with nothing inside.\n\
- Only edit files whose contents were provided.\n\
- To verify your work you may add a block with the info string `run` containing ONE shell \
command (for example `cargo check`); the user will approve it and you will receive the output.```";

/// Appended to the system prompt of a single plan step.
const STEP_RULES: &str = "# This turn: ONE plan step\n\
You are carrying out a single step of an approved plan. Write exactly ONE fenced block for the \
file named in the step. Do not touch other files and do not repeat steps that are already done. \
The file contents below are CURRENT and already include the earlier steps. Do not add `run` \
blocks and write nothing after the closing fence.\n\
Write each line of code exactly once. If the change is already present in the file, \
do not add it again.\n\
Pick ONE of these two forms:\n\
A) New file, or a file under about 40 lines: write the COMPLETE new contents:\n\
``````edit path/to/file.rs\n\
<<<<<<< SEARCH\n\
=======\n\
(the whole file, with your change included)\n\
>>>>>>> REPLACE\n\
`````\n\
B) Longer file: SEARCH/REPLACE with SEARCH copied exactly from the file:\n\
````edit path/to/file.rs\n\
<<<<<<< SEARCH\n\
    let old = 1;\n\
=======\n\
    let old = 1;\n\
    let added = 2;\n\
>>>>>>> REPLACE\n\
```\n\
Never output only the changed line without the markers.```";

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
        "write",
        "generate",
        "build",
        "setup",
        "install",
        "integrate",
        "split",
        "modify",
        "edit",
        "wire",
        "introduce",
        "scaffold",
        "migrate",
    ];
    if QUESTIONS.iter().any(|q| t.starts_with(q)) {
        return Intent::Answer;
    }
    // "set up" is two words, so check it separately.
    let has_verb = t.contains("set up")
        || t.split(|c: char| !c.is_alphabetic())
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
    system.push_str(GROUNDING);
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
        format: change.then(plan::schema),
        // Model's own temperature is kept (thinking models behave badly at 0).
        options: Options::new(cfg.num_ctx, None).max_tokens(MAX_PLAN_TOKENS),
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
        // 0.0 made small models loop on the same JSON entry.
        options: Options::new(cfg.num_ctx, Some(0.2)).max_tokens(MAX_PLAN_TOKENS),
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
    let mut user = format!(
        "Folder: {}\n\n# Request\n{}\n",
        input.project_name, input.task
    );
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
        if s.is_new {
            "new file"
        } else {
            "existing file"
        }
    ));
    if !s.target.is_empty() {
        user.push_str(&format!("Target: {}\n", s.target));
    }
    if !s.anchor.is_empty() {
        user.push_str(&format!("Anchor line: {}\n", s.anchor));
    }
    user.push_str(&format!("Change: {}\n", s.change));

    if !s.symbols.is_empty() {
        user.push_str(&format!("Symbols: {}\n", s.symbols.join(", ")));
    }
    if !s.details.is_empty() {
        user.push_str("Instructions:\n");
        for (i, d) in s.details.iter().enumerate() {
            user.push_str(&format!("{}. {d}\n", i + 1));
        }
    }

    if s.is_new || input.file_ctx.trim().is_empty() {
        user.push_str(&format!(
            "\n`{}` does not exist yet: create it with an edit block whose SEARCH is empty.\n",
            s.file
        ));
    } else {
        user.push_str(&format!(
            "\n# Current contents of {}\n{}",
            s.file, input.file_ctx
        ));
    }

    if let Some(error) = input.retry_error {
        user.push_str(&format!(
            "\n# The previous attempt at this step failed\n{error}\n\
             Some blocks may already be applied. The contents above are CURRENT. \
             Write only the edit that is still missing, with SEARCH copied exactly from them. \
             Do not repeat lines that are already in the file.\n"
        ));
    }
    user.push_str(
        "\nWrite the edit block for this step now. Each line once. \
         Stop right after the closing fence.",
    );

    ChatRequest {
        model: cfg.executor_model.clone(),
        messages: vec![ChatMsg::system(system), ChatMsg::user(user)],
        stream: true,
        think: cfg.executor_think.then_some(true),
        format: None,
        options: Options::new(
            cfg.num_ctx,
            // Greedy decoding (0.1) loops on small models; a retry needs even more variety.
            Some(if input.retry_error.is_some() {
                0.7
            } else {
                0.3
            }),
        )
        .max_tokens(MAX_STEP_TOKENS),
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
    system.push_str(GROUNDING);
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
    system.push_str("\n\nNever repeat a sentence, line of code or block you already wrote.");

    let mut messages = vec![ChatMsg::system(system)];
    messages.extend(pending.history.iter().cloned());
    messages.push(ChatMsg::user(pending.task.clone()));

    ChatRequest {
        model: cfg.executor_model.clone(),
        messages,
        stream: true,
        think: cfg.executor_think.then_some(true),
        format: None,
        options: Options::new(cfg.num_ctx, Some(0.3)).max_tokens(MAX_ANSWER_TOKENS),
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
        options: Options::new(cfg.num_ctx, Some(0.2)).max_tokens(MAX_LINE_TOKENS),
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
    Ok(fix_indent(original, dedupe_adjacent(replacement)))
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
        think: Some(false),
        format: None,
        options: Options::new(cfg.num_ctx, Some(0.2)).max_tokens(MAX_SMALL_TOKENS),
        keep_alive: cfg.keep_alive.clone(),
    }
}

/// Paths from the scout reply. The caller checks that they exist.
pub fn parse_scout(reply: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
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
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

pub fn collapse_repeats(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut in_search = false;
    let mut in_replace = false;
    let mut prev: Option<&str> = None;
    let mut copies = 0usize;

    for line in text.lines() {
        let t = line.trim();

        if t.starts_with("<<<<<<<") {
            in_search = true;
            in_replace = false;
            prev = None;
            out.push(line);
            continue;
        }
        if t.starts_with("=======") {
            in_search = false;
            in_replace = true;
            prev = None;
            out.push(line);
            continue;
        }
        if t.starts_with(">>>>>>>") {
            in_search = false;
            in_replace = false;
            prev = None;
            out.push(line);
            continue;
        }
        if in_search {
            out.push(line);
            continue;
        }

        let meaningful = t.chars().count() >= 6 && t.chars().any(char::is_alphanumeric);
        if !meaningful {
            prev = None;
            copies = 0;
            out.push(line);
            continue;
        }

        let max_copies = if in_replace { 1 } else { 2 };
        if prev == Some(line) {
            copies += 1;
            if copies > max_copies {
                continue; // drop the extra copy
            }
        } else {
            prev = Some(line);
            copies = 1;
        }
        out.push(line);
    }

    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// True when the tail of a (still streaming) reply is a loop: the same line 4+ times,
/// or the same block of 2-4 lines 3+ times in a row.
pub fn looks_looping(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .rev()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(24)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    for period in 1..=4usize {
        let reps = if period == 1 { 4 } else { 3 };
        let need = period * reps;
        if lines.len() < need {
            continue;
        }
        let tail = &lines[lines.len() - need..];
        let block = &tail[..period];
        let substantial = block.iter().map(|l| l.chars().count()).sum::<usize>() >= 6
            && block.iter().any(|l| l.chars().any(char::is_alphanumeric));
        if substantial && tail.chunks(period).all(|c| c == block) {
            return true;
        }
    }
    false
}

/// Remove identical neighbouring lines from a short replacement.
fn dedupe_adjacent(lines: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for l in lines {
        let t = l.trim();
        let meaningful = t.chars().count() >= 6 && t.chars().any(char::is_alphanumeric);
        if meaningful && out.last().is_some_and(|p| *p == l) {
            continue;
        }
        out.push(l);
    }
    out
}

/// `edit path`, `delete path` or `run`, as the model may have written it on its own line.
/// Returns the info string the fence should have (`edit src/a.rs`, `delete a.rs`, `run`).
fn action_header(line: &str) -> Option<String> {
    let t = line.trim().trim_matches('`').trim();
    if t.eq_ignore_ascii_case("run") {
        return Some("run".to_string());
    }
    for kw in ["edit", "delete"] {
        let n = kw.len() + 1;
        if t.get(..n)
            .is_some_and(|h| h[..kw.len()].eq_ignore_ascii_case(kw) && h.ends_with(' '))
        {
            let path = t[n..]
                .trim()
                .trim_matches(|c| c == '`' || c == '"' || c == '\'');
            let plausible = !path.is_empty()
                && !path.contains(char::is_whitespace)
                && (path.contains('.') || path.contains('/'));
            if plausible {
                return Some(format!("{kw} {path}"));
            }
        }
    }
    None
}

/// Rebuild the fences small models get wrong, so the action parser can read them:
///   ```            ->  ```edit path
///   edit path          <<<<<<< SEARCH ...
///   <<<<<<< SEARCH
/// and a block with no fence at all (`edit path` followed by markers).
/// Well-formed replies pass through unchanged.
pub fn repair_fences(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 2);
    let mut in_fence = false;
    let mut synthetic = false; // we opened this fence ourselves
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        let ticks = t.chars().take_while(|c| *c == '`').count();

        if in_fence {
            if synthetic {
                out.push(line.to_string());
                if t.starts_with(">>>>>>>") {
                    out.push("```".to_string());
                    in_fence = false;
                    synthetic = false;
                    // The model may add its own closing fence: swallow it.
                    if lines
                        .get(i + 1)
                        .is_some_and(|l| l.trim().len() >= 3 && l.trim().chars().all(|c| c == '`'))
                    {
                        i += 1;
                    }
                }
                i += 1;
                continue;
            }
            if ticks >= 3 && t.chars().all(|c| c == '`') {
                in_fence = false;
            }
            out.push(line.to_string());
            i += 1;
            continue;
        }

        if ticks >= 3 {
            // Opening fence. A bare one followed by a header line gets the header merged in.
            if t[ticks..].trim().is_empty() {
                if let Some(header) = lines.get(i + 1).and_then(|l| action_header(l)) {
                    out.push(format!("```{header}"));
                    in_fence = true;
                    i += 2;
                    continue;
                }
            }
            in_fence = true;
            out.push(line.to_string());
            i += 1;
            continue;
        }

        // No fence at all: `edit path` directly followed by a SEARCH marker.
        if let Some(header) = action_header(line) {
            let next_is_marker = lines
                .get(i + 1)
                .is_some_and(|l| l.trim().starts_with("<<<<<<<"));
            if header.starts_with("edit ") && next_is_marker {
                out.push(format!("```{header}"));
                in_fence = true;
                synthetic = true;
                i += 1;
                continue;
            }
        }

        out.push(line.to_string());
        i += 1;
    }

    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}
