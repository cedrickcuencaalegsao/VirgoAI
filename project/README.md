# project

The open project folder: a scanned file tree, plus helpers that read files and build prompt context from attached files and `@mentions`.

## Scanning

`Project::scan(root)` walks the folder (depth ≤ 10) and returns a `Project` with `entries`, folders first and then files, each alphabetical.

Skipped automatically:

- hidden items (names starting with `.`)
- build and dependency folders: `node_modules`, `target`, `dist`, `build`, `out`, `venv`, `env`, `__pycache__`, `vendor`, `Pods`, `DerivedData`, `coverage`
- binary files by extension (images, audio/video, archives, executables, fonts, PDFs, databases, `lock`)

Limits: 3,000 entries (`truncated` is set when exceeded) and 200,000 bytes read per file.

## API

| Method | Description |
| --- | --- |
| `visible(expanded)` | Entries to show in the tree given the set of expanded folders. |
| `read(rel)` | Reads a file as text. Rejects `..` and files containing NUL bytes. |
| `is_file(rel)` / `contains(rel)` | Existence checks. |
| `resolve_mentions(message)` | Resolves `@path`, `@file.rs` (only if the name is unique) and `@folder` (every file inside). Trailing punctuation is trimmed; order kept, duplicates removed. |
| `suggest(query, limit)` | Autocomplete for `@query`: name prefix, then name contains, then path contains. |
| `build_context(files, budget_chars)` | Reads files into fenced code blocks within a budget. Over-long files are cut with `[truncated]`; files that don't fit go in `skipped`. Returns `FileContext { text, included, skipped }`. |
| `tree_overview(max_chars)` | Plain list of paths for the model. |
| `repo_map(max_chars)` | Every file with up to 16 definition lines (`fn`, `struct`, `class`, `def`…), shrunk to fit. Used for the scout and planner in edit mode. |
| `key_files()` | README, manifests and entry points, used when the scout picks nothing. |

`lang_for(rel)` maps a file extension to a code-fence language tag and is reused by [`code`](../code).

## Dependencies

`walkdir`.