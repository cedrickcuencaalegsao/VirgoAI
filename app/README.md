# app

The application state machine for Virgo AI: the `App` struct, the `Message` enum, and `App::update`. It contains no widgets; [`virgo_gui`](../virgo_gui) renders `App`, and the root `src/main.rs` wires it into [iced](https://iced.rs).

## Entry points

```rust
let (app, startup_task) = App::new();   // loads config, memory, chats, projects; fetches models
app.update(Message::Send);               // returns an iced Task<Message>
app.subscription();                      // 400 ms Tick only while a model is working
app.theme();                             // style::theme()
```

## Core types

- **`Screen`** – `Chat`, `Projects`, `Project`, `Memory`, `Settings`.
- **`Phase`** – `Idle`, `Scouting`, `Planning`, `Executing`. `is_working()` means a stream is active; `is_busy()` also covers waiting on plan review.
- **`ActionStatus`** – result of a model-proposed action: `Applied(change_id)`, `Failed(error)`, `Ran(exit_code)`.
- **`Message`** – every UI event, grouped as chat, project panel, code panel, memory, projects, settings, and agent actions.

## How a request flows

```
Send
 ├─ classify intent (agent::pipeline::classify; /edit or Edit mode forces Change)
 ├─ gather history, memory (memory::retrieval), project instructions, project context
 │
 ├─ Intent::Change ─► Scout (planner picks files) ─► Plan (streamed) ─► user reviews/edits plan
 │                                                      └─ Run plan ─► Executor
 └─ Intent::Answer ─► Plan (if use_planner) ─► Executor
```

While the executor streams, completed edit/delete blocks are applied immediately when `auto_apply` is on (`apply_streamed`). Each change is verified on disk and recorded in a `ChangeSet` so it can be reverted. Failed edits are sent back to the model with the file's current contents (`retry_failed`), up to `MAX_AGENT_STEPS` (3). Commands from `run` blocks are executed through [`terminal`](../terminal) only when the user presses Run, and their output is fed back to the executor.

## Responsibilities

| Area | Notes |
| --- | --- |
| Chat history | Saved after each turn via [`chats`](../chats); also mirrored as markdown into the project's or global memory. |
| Streaming | `<think>…</think>` inline blocks and provider `Thinking` events go to the reasoning panel; time spent is recorded in `think_secs`. |
| Project folder | Scan, refresh, `@mention` resolution, attachments, size-budgeted context ([`project`](../project)). |
| Code panel | Open file, per-line AI suggestions, apply/undo ([`code`](../code)). |
| Projects | Instructions, knowledge notes, linked folder, per-project chats ([`workspaces`](../workspaces)). |
| Memory | Notes list, editor, and graph rebuild ([`memory`](../memory)). |
| Settings | Persisted through [`config`](../config); API keys for OpenAI, Anthropic and xAI are optional. |

## Safety behaviors worth knowing

- Edits only apply when the open folder matches the chat's linked project folder (`edit_root`).
- Revert skips any file you edited by hand since the AI change.
- Stop aborts the running stream and clears pending state.

## Dependencies

`iced`, `dirs`, `rfd` (folder picker), and the workspace crates `terminal`, `workspace`, `agent`, `chats`, `code`, `config`, `memory`, `ollama`, `project`, `workspaces`, `style`, `shared`.