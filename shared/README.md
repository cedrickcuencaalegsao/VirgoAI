# shared

Tiny crate of identifiers used by more than one crate, so they don't depend on each other just to share a scrollable id.

| Function | Id | Used for |
| --- | --- | --- |
| `chat_scroll_id()` | `chat-scroll` | Chat history scrolling (`app` snaps to the end; `virgo_gui::chat` sets it). |
| `code_scroll_id()` | `code-scroll` | Code panel scrolling. |
| `terminal_scroll_id()` | `term-scroll` | Terminal output scrolling. |

```rust
use shared::chat_scroll_id;
scrollable::snap_to(chat_scroll_id(), scrollable::RelativeOffset::END)
```

Note: `virgo_gui::chat::scroll_id()` defines the same `"chat-scroll"` id independently. Both resolve to the same widget id; prefer `shared::chat_scroll_id()` for new code.

## Dependencies

`iced`.