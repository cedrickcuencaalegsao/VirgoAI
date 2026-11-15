# memory

Long-term memory for Virgo AI: markdown notes on disk, keyword retrieval into prompts, and a graph for the Memory screen.

## Modules

### `store` — notes on disk

`MemoryStore` manages a folder of `.md` files.

| Method | Description |
| --- | --- |
| `new(dir)` / `dir()` | Create / get the folder path. |
| `ensure()` | Creates the folder and seeds `profile.md` if no notes exist. |
| `list()` | Relative names of `.md` files up to 3 folders deep, sorted. |
| `read` / `write` / `append` / `delete` | Note operations. `write` and `append` create parent folders. |
| `load_all()` | Every readable note as `Vec<Note>`. |

Free functions:

- `normalize_name(input)` – turns user input into a safe relative path ending in `.md`. Rejects `.` and `..` segments so notes cannot escape the folder; unsafe characters become `-`.
- `today()` – UTC date as `YYYY-MM-DD`, with no date crate.

`Note { name, content }` – `name` is relative, e.g. `notes/rust.md`.

### `retrieval` — what goes into the prompt

```rust
let text = retrieval::build_context(&notes, "how did we design the cache?", budget_chars);
```

1. **Always included** (if present): `profile.md` and `project/project.md`.
2. **Everything else** is split at `## ` headings and scored by keyword hits (stop words and terms under 3 characters are ignored). A note whose file name contains a query term gets +5.
3. Sections are added highest score first, each limited to 1,500 characters (a window around the first match), until the budget is used.

Scoring is deliberately simple so embeddings can replace it without changing callers.

### `graph` — the memory map

`Graph::build(&notes)` produces nodes, edges and a layout:

- **Groups** come from the top-level folder (`core` for `profile.md`/`user.md`, `inbox`, `notes` otherwise).
- **Edges**: `[[note]]` and `[[note|label]]` links (matched by full path, path without `.md`, or bare name) are *explicit*. Notes with no links get a faint edge to the hub.
- **Hub** is `profile.md` when it exists, otherwise the first note. It stays at the centre.
- **Layout** is a small deterministic Fruchterman–Reingold simulation (80 steps); coordinates are in `-1.0..=1.0`.
- `matches(query)` flags nodes containing the search text; `neighbors(i)` lists connected nodes.

## Example

```rust
use memory::store::MemoryStore;

let store = MemoryStore::new("memory".into());
store.ensure()?;
store.append("notes/rust.md", "- Prefer `?` over unwrap")?;
let graph = memory::graph::Graph::build(&store.load_all());
```

## Dependencies

`walkdir`.