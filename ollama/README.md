# ollama

HTTP client for Ollama and the external providers Virgo AI can use (OpenAI, Anthropic, xAI). Despite the name, this crate is the single LLM transport layer.

## Providers and model names

`Provider::split(model)` decides where a model runs by its prefix:

| Model name | Provider |
| --- | --- |
| `qwen3:8b` | Ollama (no prefix) |
| `openai:gpt-4o` | OpenAI |
| `anthropic:claude-…` | Anthropic |
| `xai:grok-…` | xAI |

Authentication: OpenAI and xAI use a bearer token; Anthropic uses `x-api-key` plus `anthropic-version`. A missing key returns an error before any request is made.

## API (`client`)

| Function | Description |
| --- | --- |
| `list_models(endpoint)` | Models of one provider. Non-chat models (embeddings, TTS, image, etc.) are filtered out; remote names come back prefixed. |
| `list_all_models(endpoints)` | Queries each endpoint and returns `Models { names, errors }`, so one failing provider does not hide the others. |
| `chat_once(endpoint, request)` | Non-streaming call, returns the reply text. |
| `chat_stream(endpoint, request)` | Streaming call, returns a `Stream<Item = StreamEvent>`. The request is sent when first polled. |
| `warm_up(endpoint, models, keep_alive)` | Loads local models into memory with an empty request. |
| `normalize_base(url)` | Cleans stray whitespace, defaults the scheme to `http`, strips stray `:`/`/` after `://` and trailing slashes. |

### Streaming

`StreamEvent` is `Thinking(String)`, `Token(String)`, `Done`, or `Error(String)`.

- Ollama sends NDJSON, one JSON object per line.
- OpenAI/xAI/Anthropic send server-sent events (`data: {...}`).
- Network chunks can split lines, so bytes are buffered until a full line is available.
- Reasoning is surfaced as `Thinking` from Ollama's `thinking` field, `reasoning_content`/`reasoning` (Grok), and Anthropic `thinking_delta`.

Anthropic requests move system messages into the top-level `system` field and drop leading non-user turns; `max_tokens` is fixed at 8192.

## Types (`types`)

`ChatMsg` (`system`/`user`/`assistant` constructors), `ChatRequest` (`model`, `messages`, `stream`, optional `think`, `options.num_ctx`, `keep_alive`), `Endpoint { provider, base_url, api_key }`, and the Ollama response structs.

## Known issue

`Provider::Ollama.default_url()` returns `"http://localhost::11434"` (double colon). `normalize_base` strips stray `:` after `://` but not this one; if a fresh config cannot reach Ollama, set the URL to `http://localhost:11434` in Settings.

## Dependencies

`serde`, `serde_json`, `reqwest` (json, stream), `futures`.