# code

The file open in the code panel, plus AI line edits waiting to be applied. Nothing touches the disk until `apply()` is called.

## Model

A `CodeDoc` holds the file's lines and a map of **pending edits**:

```
edits: BTreeMap<line_index, Vec<String>>   // empty Vec = delete the line
```

Each edit replaces one original line with zero or more new lines. In the view, an edited line is shown as one red row (original) followed by green rows (replacement), see `rows()` / `RowKind`.

## API

| Method | Description |
| --- | --- |
| `CodeDoc::open(root, rel)` | Opens a UTF-8 file. Rejects `..` paths and files over 1 MB. |
| `reload()` | Re-reads from disk and drops pending edits. |
| `stage(search, replace)` | Converts a SEARCH/REPLACE block into line edits. Matches ignoring trailing spaces first, then ignoring indentation. Errors if not found or ambiguous. The first matched line gets the replacement; the rest are deleted. |
| `set_edit` / `clear_edit` / `clear_all` | Manage pending edits. |
| `apply()` | Writes all edits, returns how many were applied. Keeps the old content in `previous`. |
| `undo_apply()` | Restores the content from before the last `apply()` (one level). |
| `rows()` | Display rows with kind, gutter number, and whether to show action buttons. |
| `context_around(line, radius)` | Numbered excerpt with the target line marked `>>>`, used in the AI prompt. |
| `text_width()` | Width of the code column so long lines scroll sideways. |
| `display(line)` | Expands tabs and truncates lines over 300 characters (view only). |

## Safety

- `apply()` and `undo_apply()` refuse to run if the file changed on disk since it was opened (`ensure_unchanged_on_disk`); reload and retry.
- CRLF line endings and the trailing newline are preserved when writing.

## Dependencies

[`project`](../project) (for `lang_for`).