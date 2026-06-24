# AI Copilot — GPT Integration

## Dependencies (`ai-engine/Cargo.toml`)

```toml
[features]
default = ["openai"]
openai = ["dep:async-openai"]

[dependencies]
async-openai = { version = "0.41", default-features = false, features = ["chat-completion", "rustls"] }
async-trait = "0.1"
tokio = { workspace = true }
tracing = { workspace = true }
```

| Crate | Purpose |
|-------|---------|
| `async-openai` | Official-compatible async client, `chat-completion` feature only |
| `async-trait` | `LlmBackend` trait for mocks / provider swap |
| `rustls` (via async-openai) | TLS without OpenSSL dependency |

## Module Structure

```
ai-engine/src/
├── copilot.rs    # AiCopilot — single entry point (chat pipeline)
├── client.rs     # LlmBackend trait + OpenAI + Fallback
├── prompt.rs     # PromptContext (redacted) + build_system_prompt
├── firewall.rs   # Input sanitization + injection detection
├── validator.rs  # Output validation
├── config.rs     # AiConfig from env
└── error.rs      # AiError (mapped to AppError in gateway)
```

## Secure Request Flow

```
POST /users/{id}/ai/chat
        │
        ▼
api-gateway builds SafeAIContext (wallet, locale, streak…)
        │
        ▼
AiCopilot::chat(ctx, message)
   1. AiFirewall — injection check + length limit
   2. PromptContext::from(ctx) — strips user_id before LLM
   3. build_system_prompt — system message only
   4. LlmBackend::complete(system, user_message)
   5. AiResponseValidator — reject leaked secrets
        │
        ▼
Response to client
```

**Rule:** `user_id` never leaves the gateway layer. Only `PromptContext` metrics go to OpenAI.

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `OPENAI_API_KEY` | — | Enables GPT backend (fallback if unset) |
| `OPENAI_MODEL` | `gpt-4o-mini` | Chat model |
| `OPENAI_MAX_TOKENS` | `256` | Response cap (cost control) |
| `OPENAI_TIMEOUT_SECS` | `15` | Request timeout |

## Usage

```bash
export OPENAI_API_KEY=sk-...
cargo run -p api-gateway

curl -X POST "http://localhost:3000/users/$USER/ai/chat" \
  -H "Content-Type: application/json" \
  -d '{"message": "Wie erreiche ich mein Ziel schneller?"}'
```

## Testing without API key

```bash
cargo test -p ai-engine --no-default-features
```

Fallback backend returns rule-based motivational replies.

## Architecture Notes

1. **`LlmBackend` trait** — swap OpenAI for Anthropic/local model without touching `AiCopilot`
2. **`AiCopilot` in `AppState`** — gateway only calls `copilot.chat()`, no direct OpenAI access
3. **Feature flag `openai`** — CI/tests run without network or API key
4. **Separate errors** — `AiBlocked` (403) vs `AiUnavailable` (502) for provider failures
5. **Next:** rate limiting per user in gateway (`tower-governor`), conversation history in Redis (Phase 2)
