use super::{ChatMessage, Role};
use crate::code::CodeDoc;
use crate::config::Config;
use crate::ollama::types::{ChatMsg, ChatRequest, Options};

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

const PLANNER_CHANGE_SUFFIX: &str = "\n\nThe user wants the code changed. You are given a project \
map (every file with its main definitions) and the full contents of the files most relevant to the \
request. Plan against the real code: reuse existing functions and types by name, follow the \
project's conventions, and never invent files or functions that are not shown. Write numbered \
steps in dependency order. Start each step with `- path/to/file:` using exact relative paths from \
the map, name the function or struct to change, and say what changes. Mark a file that does not \
exist yet with (new). If you need a file that was not shown, list it under 'Need to check' \
instead of guessing.";

const SCOUT_SYSTEM: &str = "You are the scouting stage of a coding assistant. You get a map of a \
project (files with their main definitions) and a request. Reply with ONLY the relative paths of \
the files that must be READ to plan the request: files to change, files that define types or \
functions the change touches, and one similar existing feature to copy conventions from. At most \
8 paths, one per line, exactly as written in the map. No commentary.";

const EDIT_SYSTEM: &str = "You are a code editor. Follow the plan and the user's request. \
For every change output exactly this format:\n\
FILE: <relative path>\n\
<<<<<<< SEARCH\n\
<existing lines, copied exactly from the file>\n\
=======\n\
<new lines>\n\
>>>>>>> REPLACE\n\
Rules: SEARCH must match the file exactly (including indentation) and be unique in the file. \
Keep blocks small. Use an empty REPLACE section to delete lines. Output only these blocks, no prose.";

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
pub fn parse_edits(reply: &str) -> Vec<Proposal> {
    let mut out = Vec::new();
    let mut file = String::new();
    let mut mode = 0; // 0 = outside, 1 = in SEARCH, 2 = in REPLACE
    let mut search: Vec<&str> = Vec::new();
    let mut replace: Vec<&str> = Vec::new();

    for line in reply.lines() {
        let t = line.trim();
        match mode {
            0 => {
                if let Some(f) = t.strip_prefix("FILE:") {
                    file = f
                        .trim()
                        .trim_matches('`')
                        .trim_matches('*')
                        .trim()
                        .to_string();
                } else if t.starts_with("<<<<<<<") {
                    mode = 1;
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

/// Everything captured when the user hits Send, kept until the executor starts.
pub struct Pending {
    pub task: String,
    pub memory: String,
    pub history: Vec<ChatMsg>,
    pub project: String,
    pub intent: Intent,
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
    let mut system = String::from(PLANNER_SYSTEM);
    if pending.intent == Intent::Change {
        system.push_str(PLANNER_CHANGE_SUFFIX);
    }
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
        options: Options {
            num_ctx: cfg.num_ctx,
        },
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
    append_memory(&mut system, &pending.memory);
    append_project(&mut system, &pending.project);
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
        options: Options {
            num_ctx: cfg.num_ctx,
        },
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
        options: Options {
            num_ctx: cfg.num_ctx,
        },
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

fn append_project(system: &mut String, project: &str) {
    if !project.trim().is_empty() {
        system.push_str("\n\n# Project (the user's open folder)\n");
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
        options: Options { num_ctx: cfg.num_ctx },
        keep_alive: cfg.keep_alive.clone(),
    }
}

/// Paths from the scout reply. The caller checks that they exist.
pub fn parse_scout(reply: &str) -> Vec<String> {
    strip_think_tags(reply)
        .lines()
        .map(|l| {
            let mut s = l.trim().trim_start_matches(|c| c == '-' || c == '*' || c == ' ');
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
