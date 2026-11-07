use super::{ChatMessage, Role};
use crate::config::Config;
use crate::ollama::types::{ChatMsg, ChatRequest, Options};

/// How many earlier user/assistant messages are sent along as context.
const HISTORY_LIMIT: usize = 10;

const PLANNER_SYSTEM: &str = "You are the planning stage of a local assistant. \
Think carefully about the user's task and write a short numbered plan (at most 7 steps) \
that another model will follow. Do NOT carry out the task and do NOT write the final answer \
or full code. Mention any risks or missing information. Use the memory notes if they are relevant.";

const EXECUTOR_SYSTEM: &str = "You are the execution stage of a local assistant. \
Answer the user's request directly and accurately. If a plan is provided, follow it. \
Use the memory notes when they are relevant, and do not mention them unless asked.";

/// Everything captured when the user hits Send, kept until the executor starts.
pub struct Pending {
    pub task: String,
    pub memory: String,
    pub history: Vec<ChatMsg>,
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
    append_memory(&mut system, &pending.memory);

    let mut messages = vec![ChatMsg::system(system)];
    messages.extend(pending.history.iter().cloned());
    messages.push(ChatMsg::user(pending.task.clone()));

    ChatRequest {
        model: cfg.planner_model.clone(),
        messages,
        stream: false,
        think: cfg.planner_think.then_some(true),
        options: Options { num_ctx: cfg.num_ctx },
        keep_alive: cfg.keep_alive.clone(),
    }
}

pub fn executor_request(cfg: &Config, pending: &Pending, plan: Option<&str>) -> ChatRequest {
    let mut system = String::from(EXECUTOR_SYSTEM);
    append_memory(&mut system, &pending.memory);
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
        think: None,
        options: Options { num_ctx: cfg.num_ctx },
        keep_alive: cfg.keep_alive.clone(),
    }
}

fn append_memory(system: &mut String, memory: &str) {
    if !memory.trim().is_empty() {
        system.push_str("\n\n# Memory notes (from the user's local files)\n");
        system.push_str(memory);
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
