# agent

Chat data types and the prompt-building pipeline that drives Virgo AI's two-stage (planner → executor) workflow.

This crate builds requests; it does not send them. Sending and streaming is done by [`ollama`](../ollama), and orchestration lives in [`app`](../app).

## What's inside

### `lib.rs`

| Item | Purpose |
| --- | --- |
| `Role` | `User`, `Plan` (planner output), `Assistant` (executor output), `Error`. |
| `ChatMessage` | One chat bubble: `role`, `content`, `thinking` (reasoning, kept separate), `think_secs`, `files` (project files attached), and `show_thinking` (UI-only, not serialized). |
| `ChatMessage::new` / `with_files` | Constructors. |

### `pipeline.rs`

**Intent**

- `Intent::Answer` – a question or explanation.
- `Intent::Change` – a code change (plan → user review → edit blocks).
- `classify(text, has_project)` – keyword heuristic: with no project it is always `Answer`; question openers (`what`, `why`, `explain`…) force `Answer`; action verbs (`add`, `fix`, `refactor`…) give `Change`.

**Request builders** (all return an `ollama::types::ChatRequest`)

| Function | Stage |
| --- | --- |
| `planner_request(cfg, pending)` | Streaming planner call. Adds a change-specific suffix when `intent == Change`. |
| `executor_request(cfg, pending, plan)` | Streaming executor call. Appends the edit protocol when a project is open, and the plan if there is one. |
| `scout_request(cfg, pending)` | Non-streaming call asking the planner which files to read (`think: false`). |
| `line_edit_request(cfg, doc, line, instruction)` | Non-streaming call to rewrite a single line in the code panel. |

System prompts are layered in this order: base prompt → project instructions → memory notes → project context → (executor only) edit protocol → plan.

**Parsers and helpers**

- `parse_scout(reply)` – turns the scout's reply into a list of paths (strips bullets, numbering, backticks).
- `parse_line_edit(reply, original)` – extracts the replacement from a fenced code block; empty block means "delete the line". Restores indentation the model dropped (`fix_indent`).
- `parse_edits(reply, known, default_file)` – legacy `FILE:` + SEARCH/REPLACE parser, tolerant of markdown decoration. The current flow uses [`workspace`](../workspace) fenced blocks instead.
- `history_from(messages)` – last 10 user/assistant turns; plans and errors are dropped.
- `strip_think_tags(text)` – removes inline `<think>…</think>` blocks.

**Data**

- `Pending` – everything captured when the user presses Send (task, memory, history, project context, intent, instructions), held until the executor starts.
- `Proposal` – one SEARCH/REPLACE block (`file`, `search`, `replace`).

## The edit protocol

When a project is open, the executor is told to reply with fenced blocks:

````
```edit src/main.rs
<<<<<<< SEARCH
exact lines from the current file
=======
replacement lines
>>>>>>> REPLACE
```
````

Also supported: ` ```delete path ``` ` and ` ```run ` with one shell command. These are parsed and applied by [`workspace`](../workspace).

## Dependencies

`serde`, [`code`](../code), [`config`](../config), [`ollama`](../ollama).