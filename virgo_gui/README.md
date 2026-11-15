# virgo_gui

The iced views for Virgo AI. Every module exposes a `view(app: &App) -> Element<Message>`; state and logic live in [`app`](../app).

## Modules

| Module | Screen / region |
| --- | --- |
| `nav` | Left sidebar: brand, New chat, navigation, projects with their chats, recent general chats, models in use. |
| `chat` | Conversation: model pickers, message bubbles, plan review, proposed-change chips, attachments, `@` suggestions, composer. Enter sends; Shift+Enter adds a newline. |
| `markdown` | Small renderer for replies: headings, bullets, numbered lists, bold, inline code, rules, and fenced code with a Copy button. Handles unclosed fences while streaming. |
| `actions` | Cards under an assistant reply for model actions: edit (diff preview, Apply/Revert), run (Run), delete. Shows "Apply all" when several are unapplied. |
| `code_panel` | Open file with a bulb on each line, red/green diff rows, Apply/Undo/Discard, and the suggestion bar. Only rows near the scroll position are built (fixed 24 px rows). |
| `project_panel` | Right panel: open a folder, browse the tree, attach files with the paperclip. |
| `terminal_panel` | Bottom terminal log and command input. |
| `workspaces` | Projects list and project home (composer, recent conversations, instructions, knowledge notes, linked folder). |
| `memory` | Memory screen: searchable note list, interactive graph on a canvas, inspector/editor. |
| `settings` | Connection, provider keys, memory folder, context size, planner/executor options. |
| `sidebar` | Older plain-styled memory sidebar; not used by `main.rs`. |

`short(text, max)` in `lib.rs` truncates text with `...` for one-line labels.

## Wiring

`src/main.rs` picks the page by `app.screen` and lays out:

```
[ nav ] [ page ]      Chat page = chat | code_panel (if a file is open) | project_panel
        footer        optional terminal_panel below the chat row
```

## Conventions

- Colours, spacing and shapes come from [`style`](../style); icons from [`icon_manager`](../icon_manager). Don't hard-code either.
- Views never mutate state. They emit `app::Message` values.
- Buttons use `on_press_maybe` to disable themselves while a model is busy.

## Dependencies

`iced` (tokio, svg, canvas) and the workspace crates `app`, `agent`, `chats`, `code`, `config`, `icon_manager`, `memory`, `project`, `shared`, `style`, `workspace`, `workspaces`.