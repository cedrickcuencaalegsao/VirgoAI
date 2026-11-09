# Virgo AI

A native macOS desktop app built with **Rust** and **iced** that talks to a locally running **Ollama**. A **planner model** thinks the task through, an **executor model** writes the answer, **markdown notes on your SSD** act as long-term memory, and you can **open a project folder** and bring its files into the chat. Everything stays on your machine.

## Features

- **Project folder panel (right side)**: open a folder, browse its tree, click files to attach them, or type `@` in the chat to mention files and folders (with autocomplete). The model always gets the folder's file tree, plus the contents of attached and mentioned files.
- **Code panel with AI line edits**: click the eye on a file to open it in a code panel. Every line has a bulb button: click it, optionally say what to change, and the model proposes a replacement. The original shows red and the new text green; nothing is written until you press **Apply** (with a one-step **Undo apply**).
- **Live thinking indicator**: an animated `Thinking...` with a timer while a model reasons; the reasoning collapses to `Thought for 4.2s` when the answer starts.
- **Planner then executor**: both stages stream. Stop cancels either one.
- **Readable replies**: markdown rendering (headings, lists, bold, inline code) with a Copy button on every code block and every reply.
- **Chat history**: every conversation is saved and listed in the sidebar.
- **Markdown memory**: notes in a folder on disk; `profile.md` is always sent, other notes by keyword match.
- **Lucide icons** everywhere, added from the command line with `make add-icon`.

## Quick start

```bash
make init      # check Rust, download Lucide icons, fetch crates
make models    # ollama pull qwen3:8b qwen2.5-coder:7b
make run
```

Ollama must be running in the background. No model needs to be preloaded.

## Using a project folder

1. In the right panel, click **Open folder** (or paste a path). The last folder is reopened at startup.
2. Click a folder to expand it. Click a file to attach it; attached files show as chips above the chat box and are re-read and sent with every message until you remove them.
3. Or mention files in the message:
   - `@main.rs` (a file name, if it is unique)
   - `@src/app.rs` (a path)
   - `@src` (every text file in that folder)
4. Hidden items, build folders (`target`, `node_modules`, ...) and binary files are left out.

File contents are limited to about 35% of the context window (`num_ctx`); anything over budget is cut or skipped, and the status bar says so. File contents are added to the prompt for each message but are not stored in the chat history, only the list of file names is.

## Editing a file with AI, line by line

1. Open a project, then click the **eye** next to a file. It opens in the code panel between the chat and the project panel.
2. Click the **bulb** on any line. A bar appears at the bottom of the panel: type what you want (or leave it empty to have the model improve the line) and press **Suggest**. The executor model is used.
3. The result appears in place: the original line in **red**, the replacement in **green**. Use the undo arrow on a red line to revert just that one, or **Discard** to drop everything.
4. Press **Apply (N)** to write all pending changes to the file. **Undo apply** restores the file to how it was before the last apply.

Safety: nothing is written until Apply, and Apply refuses to run if the file changed on disk since you opened it (press reload first). Files over 1 MB are not opened, and only the rows near the scroll position are drawn, so large files stay fast.

## Makefile

| Command | What it does |
| --- | --- |
| `make init` | Checks Rust, curl and Ollama, downloads the Lucide icons the app uses, runs `cargo fetch` |
| `make models` | Pulls the Ollama models in `MODELS` (default `qwen3:8b qwen2.5-coder:7b`) |
| `make run` / `build` / `release` / `typecheck` / `fmt` / `clean` | Cargo shortcuts |
| `make add-icon sun moon` | Downloads Lucide icons to `assets/icons/` and registers them in `src/icons.rs` |
| `make icons` | Re-downloads every icon already in `assets/icons/` |

After `make add-icon hard-drive`, use it in code as `icons::hard_drive()`. Icon names are on <https://lucide.dev/icons>. If an icon name ever clashes with a Make target, pass it as `make add-icon ICONS=name`. Stroke width defaults to 1.5; override with `ICON_STROKE=2`.

## Project structure

```
virgo-ai/
├── Makefile
├── Cargo.toml
├── config.toml              # sample settings
├── assets/icons/*.svg       # Lucide icons
└── src/
    ├── main.rs              # window, theme, subscription
    ├── app.rs               # State, Message, update(), view()
    ├── config.rs            # settings, data folder
    ├── chats.rs             # save/load/list/delete conversations
    ├── project.rs           # scan a folder, read files, @mentions, prompt context
    ├── code.rs              # open file + pending AI line edits, apply/undo
    ├── icons.rs             # Lucide icons embedded with icon_fn!
    ├── ollama/              # client (streaming) and types
    ├── memory/              # markdown notes: store + retrieval
    ├── agent/               # Role, ChatMessage, planner/executor prompts
    └── ui/
        ├── style.rs         # theme + styles
        ├── nav.rs           # sidebar
        ├── chat.rs          # conversation, chips, @ suggestions, composer
        ├── project_panel.rs # right-hand folder panel
        ├── code_panel.rs    # code viewer with per-line suggestion buttons
        ├── markdown.rs      # markdown renderer + code blocks with Copy
        ├── memory.rs        # notes list + editor
        └── settings.rs      # settings form
```

Data lives in `~/Library/Application Support/VirgoAI/` (`config.toml`, `memory/`, `chats/`).

## Notes

- Targets **iced 0.13**; keep it pinned.
- The folder dialog uses the `rfd` crate. If it does not open, paste the path into the panel instead.
- Only plans and answers are fed back into the conversation, never the reasoning text.