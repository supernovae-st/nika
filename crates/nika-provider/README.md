# nika-provider

LLM provider integration for Nika (rig-core wrapper).

## Overview

This crate wraps `rig-core` v0.31 to provide unified LLM access:

- **6 Providers** - Claude, OpenAI, Mistral, Groq, DeepSeek, Ollama
- **Full Streaming** - Real-time token delivery across all providers
- **Tool Calling** - Native function calling support
- **MCP Integration** - NikaMcpTool for agent tool use

## Architecture

```
nika-provider/
├── lib.rs    # Module exports, provider factory
└── rig.rs    # RigProvider wrapper (761 lines)
              # NikaMcpTool, NikaMcpToolDef
```

## Supported Providers

| Provider | Env Variable | Models |
|----------|--------------|--------|
| Claude | `ANTHROPIC_API_KEY` | claude-3-opus, claude-3-sonnet, claude-3-haiku |
| OpenAI | `OPENAI_API_KEY` | gpt-4, gpt-4-turbo, gpt-3.5-turbo |
| Mistral | `MISTRAL_API_KEY` | mistral-large, mistral-medium |
| Groq | `GROQ_API_KEY` | llama-3, mixtral |
| DeepSeek | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-coder |
| Ollama | Local | llama3, codellama, mistral |

## Usage

```rust
use nika_provider::create_provider;

// Create a Claude provider
let provider = create_provider("claude", Some("claude-3-sonnet"))?;

// Stream a response
let stream = provider.chat_stream(messages).await?;
```

## License

MIT
