# config

Settings, data folder location, and provider endpoint resolution.

## Config file

Stored at `<app dir>/config.toml` (macOS: `~/Library/Application Support/VirgoAI/config.toml`) and created with defaults on first run. On save, the file is set to mode `0600` on Unix because it can contain API keys.

`Config` uses `#[serde(default)]`, so missing keys fall back to defaults and old config files keep working. A sample lives at the repo root (`config.toml`).

| Field | Default | Meaning |
| --- | --- | --- |
| `ollama_url` | Ollama default URL | Local Ollama server. |
| `planner_model` | `qwen3:8b` | Model for scouting and planning. |
| `executor_model` | `qwen2.5-coder:7b` | Model for answers, edits and line suggestions. |
| `memory_dir` | `""` | Empty = `<app dir>/memory`. A leading `~/` is expanded. |
| `num_ctx` | `8192` | Context window sent to Ollama; also sizes the file/memory budgets. |
| `keep_alive` | `10m` | How long Ollama keeps models loaded. |
| `use_planner` | `true` | Plan first, then execute. |
| `planner_think` / `executor_think` | `true` / `false` | Request reasoning from models that support it. |
| `auto_apply` | `true` | Apply model edits as soon as each block completes. |
| `last_project` | `""` | Folder reopened at startup. |
| `active_workspace` | `""` | Project that new chats are saved into. |
| `openai_key`, `anthropic_key`, `xai_key` | `""` | Empty disables the provider. Environment variables `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `XAI_API_KEY` are used as a fallback. |
| `openai_url`, `anthropic_url`, `xai_url` | provider defaults | Override for proxies or OpenAI-compatible servers. |

## API

- `app_dir()` – data root, `<data dir>/VirgoAI`. A legacy `local-agent` folder is migrated once on load.
- `Config::load()` / `save()` / `path()`
- `memory_path()` – resolved memory folder.
- `endpoint(provider)` – an `ollama::types::Endpoint` with URL and key.
- `endpoint_for(model)` – picks the provider from a model name such as `openai:gpt-4o` or `qwen3:8b`.
- `endpoints()` – Ollama plus every provider that has a key.

## Dependencies

`serde`, `dirs`, `toml`, [`ollama`](../ollama).