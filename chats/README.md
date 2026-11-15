# chats

Persistent chat history. Each conversation is one JSON file in `<app dir>/chats/` (on macOS, `~/Library/Application Support/VirgoAI/chats/`).

## API

| Function | Description |
| --- | --- |
| `save(&Chat)` | Writes to a temp file then renames, so a crash cannot corrupt an existing chat. |
| `load(id)` | Reads one chat. |
| `delete(id)` | Removes one chat file. |
| `list()` | Returns `Vec<ChatSummary>`, newest first. Reads only header fields (`id`, `title`, `updated`, `project`), skipping message bodies. |
| `detach_project(slug)` | Clears the `project` field on every chat in that project (used when a project is deleted). |
| `new_id()` | Milliseconds since the Unix epoch, as a string. |
| `now_secs()` | Current Unix time in seconds. |
| `title_from(messages)` | First line of the first user message, cut to 40 characters; `"New chat"` if empty. |

## Types

```rust
pub struct Chat {
    pub id: String,
    pub title: String,
    pub updated: u64,
    pub messages: Vec<agent::ChatMessage>,
    pub project: Option<String>,   // project slug, None for general chats
}
```

`ChatSummary` is the lightweight version used by the sidebar.

## Notes

- Chat ids become file names, so only ASCII alphanumerics, `-` and `_` are accepted; anything else returns `InvalidInput`.
- UI-only state such as `show_thinking` is not saved.

## Dependencies

`serde`, `serde_json`, [`config`](../config) (for `app_dir()`), [`agent`](../agent).