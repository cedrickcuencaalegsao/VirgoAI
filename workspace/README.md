# workspace

Parses actions proposed by the model, applies them to disk with verification, and can undo them. It has no dependencies.

Not to be confused with [`workspaces`](../workspaces), which manages Virgo's *projects*.

## Action format

The model writes fenced blocks. The info string selects the action:

````
```edit src/lib.rs
<<<<<<< SEARCH
exact existing lines
=======
replacement lines
>>>>>>> REPLACE
```

```delete old/file.rs
```

```run
cargo check
```
````

- Several SEARCH/REPLACE pairs may appear in one `edit` block.
- Empty SEARCH creates a new file (only if the file is empty or missing).
- Empty REPLACE deletes the matched lines.
- Fences of four or more backticks are supported.

## Parsing

- `parse_actions(reply)` – all actions in a finished reply.
- `completed_actions(partial)` – only blocks whose closing fence has arrived. Always a prefix of `parse_actions` on the same text, so it's safe to apply while streaming.

Returns `Action::Edit { path, hunks }`, `Action::Delete(path)` or `Action::Run(command)`.

## Applying

| Function | Behavior |
| --- | --- |
| `apply_edit(root, rel, hunks)` | Matches each hunk exactly once; falls back to whitespace-insensitive line matching. Errors on no match, multiple matches, or a no-op edit. Writes atomically and reads the file back to verify before returning a `FileChange`. |
| `apply_delete(root, rel)` | Deletes a UTF-8 text file and checks it is gone. |
| `apply_hunks(original, hunks)` | Pure transformation; preserves CRLF. |

Atomic write: temp file in the same folder → `sync_all` → rename, keeping file permissions.

## Path safety (`safe_join`)

Only plain relative paths are accepted: no `..`, no absolute paths, nothing under `.git`. Symlinks are resolved and the real parent folder must remain inside the real project root.

## Undo

`ChangeSet` collects `FileChange { rel, before, after }` entries (keeping the first `before` and latest `after` per file). `revert(root)` restores them in reverse and returns `(restored, skipped)`. A file whose current content no longer equals `after` was edited by hand and is **skipped**, never overwritten.

## Tests

```bash
cargo test -p workspace
```

Covers modify, create, delete and revert, bad paths, failed writes, no-op edits, project isolation, symlink escape, and streaming parse behavior.