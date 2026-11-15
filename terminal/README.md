# terminal

Runs one shell command in the project folder and captures its output.

## API

```rust
let result: RunResult = terminal::run(project_root, "cargo check".into()).await;
```

`RunResult { output, code, timed_out }`

- `output` – combined stdout and stderr (stderr is merged with `2>&1`), limited to the last 20,000 characters.
- `code` – exit code, or `None` if the process couldn't start, timed out, or was killed by a signal.
- `timed_out` – `true` after 120 seconds. The child is killed on drop.

`tail(text, max)` returns the last `max` characters prefixed with `...`. It is used to keep command output and file excerpts small before they go back to the model.

## How commands run

- Executed through `$SHELL -lc` (fallback `/bin/sh`), a **login shell** so tools like `~/.cargo/bin` are on `PATH` even when the app is launched from Finder.
- The command is wrapped in `( … ) 2>&1` with newlines around it, so a trailing `# comment` can't swallow the closing parenthesis.
- stdin is closed, so interactive programs won't hang waiting for input.
- `NO_COLOR=1` and `CARGO_TERM_COLOR=never` keep the output free of ANSI codes.

## Safety

This crate does not restrict commands. The app only runs a model-proposed command after the user presses **Run**; commands typed in the terminal panel run as typed.

## Dependencies

`tokio` (process, time, io-util).