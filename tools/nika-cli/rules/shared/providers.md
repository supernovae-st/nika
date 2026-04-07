## Providers

| Provider | Env Var | Example Models |
|----------|---------|----------------|
| `anthropic` | `ANTHROPIC_API_KEY` | claude-opus-4-20250514, claude-sonnet-4-20250514, claude-haiku-4-5 |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4.1, o3, o4-mini |
| `mistral` | `MISTRAL_API_KEY` | mistral-large-latest, mistral-small-latest |
| `groq` | `GROQ_API_KEY` | llama-3.3-70b-versatile, mixtral-8x7b-32768 |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-reasoner |
| `gemini` | `GEMINI_API_KEY` | gemini-2.5-pro, gemini-2.5-flash |
| `xai` | `XAI_API_KEY` | grok-3 |
| `native` | *(none)* | Local GGUF via mistral.rs (text only) |
| `mock` | *(none)* | Deterministic test responses — no API calls |

### Slash syntax

```yaml
model: groq/llama-3.3-70b         # provider=groq, model=llama-3.3-70b
model: native/Qwen/Qwen3-8B       # provider=native, model=Qwen/Qwen3-8B
model: h100/Qwen/Qwen3-8B         # named endpoint from nika.toml
```

### Auto-infer

When `model:` specifies a well-known model, `provider:` is inferred automatically:
```yaml
model: claude-sonnet-4-20250514   # → provider: anthropic (auto)
model: gpt-4o                     # → provider: openai (auto)
```
