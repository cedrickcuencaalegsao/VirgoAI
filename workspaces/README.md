# workspaces

Projects: dedicated workspaces with their own instructions, knowledge notes, saved conversations and an optional linked folder. Not to be confused with [`workspace`](../workspace), which applies file edits.

## On-disk layout

```
<app dir>/projects/<slug>/
├── project.md          # description + decisions; first "# Title" is the display name; always sent to the model
├── instructions.md     # applied to every conversation in the project
├── folder.txt          # path of the linked code folder (optional)
├── memory/             # hand-written knowledge notes (memory/inbox.md receives "Remember last reply")
└── conversations/      # markdown copies of chats: <title>-<chat id>.md
```

## API

**Projects**

- `list()` – sorted by name. `create(name)` – makes the folder, `project.md`, empty `instructions.md` and `memory/`. `delete(slug)`. `exists(slug)`.
- `slugify("My App!")` → `my-app`. Slugs may contain only ASCII alphanumerics, `-` and `_`.

**Instructions and folder**

- `instructions(slug)` / `save_instructions(slug, text)`
- `linked_folder(slug)` / `link_folder(slug, path)`

**Knowledge**

- `notes(slug)` – knowledge note names, `project.md` first. Excludes `instructions.md` and old chat copies at the project root.
- `load_notes(slug, skip_chat)` – knowledge plus saved conversations, each named `project/<name>` for retrieval. The chat currently open is skipped because its history is already sent.
- `append_memory(slug, text)` – appends to `memory/inbox.md`.
- `store(slug)` – a [`memory::store::MemoryStore`](../memory) rooted at the project.

**Conversation copies**

- `save_chat(slug, chat)` – writes `conversations/<title>-<id>.md`.
- `save_chat_to(store, chat)` – same, for general chats in the global memory.
- `chat_markdown(chat)` – renders `## You`, `## Plan`, `## Virgo` sections. Headings inside message bodies are demoted to `####` so those stay the only level-2 headings.
- `chat_id(note)` – extracts the id from a conversation file name.

Constants: `INSTRUCTIONS`, `PROJECT_NOTE`.

## Why conversations become notes

Saved chats are loaded as retrieval notes, so asking "do you remember the clear-button feature?" can be answered from earlier conversations in the same project.

## Dependencies

[`agent`](../agent), [`chats`](../chats), [`memory`](../memory), [`config`](../config).