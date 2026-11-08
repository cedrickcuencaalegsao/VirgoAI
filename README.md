# Virgo AI

A native macOS desktop app built with **Rust** and **iced** that talks to a locally running **Ollama**. A **planner model** thinks the task through, an **executor model** writes the answer, and **markdown notes on your SSD** act as long-term memory. Everything stays on your machine.

## Features

- **Live thinking indicator**: while a model reasons, the bubble shows an animated `Thinking...` with a running timer. The reasoning streams into a collapsible panel; when the answer starts it collapses to `Thought for 4.2s` (click Show to reopen).
- **Planner then executor**: both stages stream. The plan appears in its own card, then the answer streams below it. Stop cancels either stage.
- **Model pickers** fed from Ollama's installed models.
- **Chat history**: every conversation is saved as a JSON file and listed in the sidebar; click to reopen, x to delete.
- **Markdown memory**: notes are `.md` files; `profile.md` is always sent, other notes are chosen by keyword match. Edit them on the Memory page, or use "Remember last reply".
- **Modern dark UI**: custom violet theme, rounded cards, sidebar navigation (Chat, Memory, Settings).

## Project structure

```
virgo-ai/
├── Cargo.toml
├── README.md
├── config.toml              # sample settings
└── src/
    ├── main.rs              # window, theme, subscription
    ├── app.rs               # State, Message, update(), view()
    ├── config.rs            # load/save settings, data folder
    ├── chats.rs             # save/load/list/delete conversations (JSON files)
    ├── ollama/
    │   ├── client.rs        # list models, warm up, streaming chat
    │   └── types.rs         # request/response structs, StreamEvent
    ├── memory/
    │   ├── store.rs         # read/write/append/delete .md files
    │   └── retrieval.rs     # pick relevant notes
    ├── agent/
    │   ├── mod.rs           # Role, ChatMessage
    │   └── pipeline.rs      # planner and executor prompts
    └── ui/
        ├── style.rs         # theme + all custom styles
        ├── nav.rs           # sidebar
        ├── chat.rs          # conversation, thinking panel, composer
        ├── memory.rs        # notes list + editor
        └── settings.rs      # settings form
```

Data lives in `~/Library/Application Support/VirgoAI/` (`config.toml`, `memory/` and `chats/`). If you ran the earlier `local-agent` build, its folder is moved there automatically on first launch.

## Run

```bash
ollama pull qwen3:8b
ollama pull qwen2.5-coder:7b
cargo run --release
```

Ollama must be running in the background. No model needs to be preloaded.

## How thinking works

1. The planner request is sent with `think: true`, so Ollama streams the model's reasoning separately from its answer (`message.thinking` vs `message.content`).
2. The app shows the reasoning in the bubble's collapsible panel and times it.
3. Models that put reasoning inline in `<think>...</think>` tags are handled too.
4. The planner needs a thinking-capable model (qwen3, deepseek-r1, gpt-oss). If you pick one that is not, turn off "Planner think mode" in Settings.
5. "Executor think mode" is off by default; enable it if your executor supports thinking.

## Notes

- Targets **iced 0.13** (the API changes between releases, so keep it pinned).
- Reasoning is shown but never fed back into the conversation history, only plans and answers are.