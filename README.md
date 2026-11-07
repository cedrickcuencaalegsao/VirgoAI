# Virgo AI (Rust + iced + Ollama)

A native macOS desktop app built with **Rust** and **iced** that talks to a locally running **Ollama**. It uses markdown files on your SSD as long-term memory, and splits work between a **planner model** (thinking) and an **executor model** (doing).

No web server, no cloud. Everything stays on your machine.

---

## Core functionalities

1. **Chat UI**: a native iced window with streaming responses.
2. **Model picker**: lists installed models from Ollama (`GET /api/tags`); choose a planner and an executor independently.
3. **Planner → Executor pipeline**: the planner model writes a step-by-step plan, then the executor model carries it out using that plan.
4. **Markdown memory**: notes live as `.md` files on disk. Relevant notes are injected into the prompt on every turn.
5. **Remember**: save a new fact or note into memory from the UI (appends to a markdown file).
6. **Memory browser**: sidebar to view, search, edit, and delete notes.
7. **Settings**: model names, memory folder, `num_ctx`, `keep_alive`, Ollama URL.

---

## Project structure

```
virgo_ai/
├── Cargo.toml
├── README.md
├── config.toml              # default settings (models, paths, ollama url)
└── src/
    ├── main.rs              # entry point, launches the iced app
    ├── app.rs               # State, Message enum, update(), view()
    ├── config.rs            # load/save config.toml
    ├── ollama/
    │   ├── mod.rs
    │   ├── client.rs        # chat (stream), list models, warm-up
    │   └── types.rs         # request/response structs (serde)
    ├── memory/
    │   ├── mod.rs
    │   ├── store.rs         # read/write/append/delete .md files
    │   └── retrieval.rs     # pick relevant notes (keyword, later embeddings)
    ├── agent/
    │   ├── mod.rs
    │   └── pipeline.rs      # planner -> executor flow, prompt building
    └── ui/
        ├── mod.rs
        ├── chat.rs          # message list + input box
        ├── sidebar.rs       # memory browser
        └── settings.rs      # settings panel
```

Memory is stored outside the repo, in the macOS app data folder:

```
~/Library/Application Support/virgo_ai/memory/
├── profile.md
├── projects.md
└── notes/
    └── *.md
```

---

## Getting started

### 1. Prerequisites

```bash
# Xcode command line tools
xcode-select --install

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Ollama (https://ollama.com), then pull a planner and an executor
ollama pull qwen3:8b
ollama pull qwen2.5-coder:7b
```

Make sure Ollama is running in the background (the menu bar app, or `ollama serve`). It does not need a model preloaded: the app picks the model per request.

### 2. Get the project

Unzip `virgo_ai.zip` (or create it yourself with `cargo new local-agent` and copy the files in). `Cargo.toml` needs:

```toml
[dependencies]
iced = { version = "0.13", features = ["tokio"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
futures = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
dirs = "5"
walkdir = "2"
```

Check crates.io for the latest versions. iced's API changes between releases, so pin the version you build against.

### 3. Run

```bash
cargo run --release
```

---

## How it works

### iced architecture (Elm-style)

```
State  --view()-->  UI  --user action-->  Message  --update()-->  State
```

- **State**: chat history, selected models, memory notes, settings.
- **Message**: `InputChanged`, `Send`, `PlanReady(String)`, `Token(String)`, `ModelsLoaded(Vec<String>)`, `RememberNote`, ...
- **update()**: handles messages and returns `Task`s for async work (HTTP calls to Ollama).
- **view()**: builds the widgets from the current state.

### One turn of the agent

1. User sends a message.
2. `retrieval.rs` selects relevant notes from the memory folder.
3. **Planner call**: `POST /api/chat` with the planner model, memory + task, asking for a plan only.
4. **Executor call**: `POST /api/chat` with the executor model, memory + task + plan, streamed token by token into the UI.
5. Optional: user clicks **Remember** to append the result or a fact to a markdown note.

### Ollama notes

- Every request names its model, so switching planner and executor is just changing the `model` field.
- Set `keep_alive` (for example `"10m"`) so models stay loaded between calls.
- Set `num_ctx` explicitly. The default context is small and will silently truncate injected memory.
- On a Mac, RAM is shared with the GPU. Two 7-8B models at once need around 10-12 GB; on 16 GB machines expect load time when switching.

---

## Build roadmap

1. **Window + chat**: iced window, input box, send a prompt to one model, show the full reply.
2. **Streaming**: stream tokens into the message list.
3. **Model picker**: fetch `/api/tags`, dropdowns for planner and executor.
4. **Memory read**: load all `.md` files into the system prompt.
5. **Planner → executor**: two-pass pipeline.
6. **Memory write**: Remember button and append to notes.
7. **Retrieval**: keyword search first, then embeddings (`nomic-embed-text` via `/api/embed`).
8. **Polish**: settings screen, memory browser, markdown rendering, error states.

---

## Configuration (`config.toml`)

```toml
ollama_url = "http://localhost:11434"
planner_model = "qwen3:8b"
executor_model = "qwen2.5-coder:7b"
memory_dir = "~/Library/Application Support/virgo_ai/memory"
num_ctx = 8192
keep_alive = "10m"
```

---

## License

Your choice (MIT is a common default for personal projects).
