# Migration Plan: Merge nika-runtime into nika-cli

> **Date**: 2025-12-22
> **Status**: In Progress
> **Philosophy**: v0 - Break things to make them better

## Overview

Merge nika-runtime (multi-provider engine) into nika-cli (validator + runner).

### Why?

- nika-runtime uses **v3 architecture** (obsolete)
- nika-cli uses **v4.6 architecture** (current)
- Parser/Validator from nika-runtime are INCOMPATIBLE
- Only value: provider stubs (claude, openai, ollama, mistral, scaleway)

### Decisions

| Decision | Choice |
|----------|--------|
| Sync vs Async | **Async** (tokio + reqwest) |
| OpenAI | **Real implementation** |
| Ollama/Mistral | **Stubs** |
| Scaleway | **Removed** (not needed) |
| Tracing | **Added** (structured logging) |

## Phase 1: Dependencies

**Files**: `Cargo.toml`

```toml
# Add to [dependencies]
reqwest = { version = "0.12", features = ["json"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

**Verification**:
- [ ] `cargo check` passes
- [ ] No version conflicts

## Phase 2: Provider Trait Async

**Files**: `src/provider/mod.rs`, `src/provider/claude.rs`, `src/provider/mock.rs`

### 2.1 Add Capabilities struct

```rust
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub tool_use: bool,
    pub vision: bool,
    pub streaming: bool,
    pub extended_thinking: bool,
    pub json_mode: bool,
    pub max_context: usize,
}
```

### 2.2 Make Provider trait async

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn capabilities(&self) -> Capabilities;
    fn is_available(&self) -> bool { true }
    async fn execute(&self, request: PromptRequest) -> Result<PromptResponse>;
}
```

### 2.3 Update existing providers

- `ClaudeProvider` - add async, add capabilities()
- `MockProvider` - add async, add capabilities()

**Verification**:
- [ ] Trait compiles
- [ ] Existing providers implement new trait
- [ ] `cargo check` passes

## Phase 3: New Providers

**Files**: `src/provider/openai.rs`, `src/provider/ollama.rs`, `src/provider/mistral.rs`

### 3.1 OpenAI Provider (REAL)

```rust
pub struct OpenAIProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl OpenAIProvider {
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY not set"))?;
        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model: "gpt-4o".to_string(),
        })
    }
}
```

### 3.2 Ollama Provider (STUB)

```rust
pub struct OllamaProvider {
    host: String,
    model: String,
}
```

### 3.3 Mistral Provider (STUB)

```rust
pub struct MistralProvider {
    api_key: String,
    model: String,
}
```

**Verification**:
- [ ] Each provider compiles
- [ ] Each provider implements Provider trait
- [ ] Unit tests pass

## Phase 4: Async Runner

**Files**: `src/runner.rs`, `src/main.rs`

### 4.1 Make runner async

```rust
// Change all provider calls to async
pub async fn run(&self, workflow: &Workflow) -> Result<RunResult> {
    // ...
    let response = self.provider.execute(request).await?;
    // ...
}
```

### 4.2 Update main.rs

```rust
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    // ...
    let result = runner.run(&workflow).await?;
}
```

**Verification**:
- [ ] `cargo check` passes
- [ ] `cargo test` passes
- [ ] CLI runs with `--provider mock`

## Phase 5: Provider Factory

**Files**: `src/provider/mod.rs`

```rust
pub fn create_provider(name: &str) -> Result<Box<dyn Provider>> {
    match name.to_lowercase().as_str() {
        "claude" => Ok(Box::new(ClaudeProvider::new())),
        "openai" => Ok(Box::new(OpenAIProvider::new()?)),
        "ollama" => Ok(Box::new(OllamaProvider::new())),
        "mistral" => Ok(Box::new(MistralProvider::new()?)),
        "mock" => Ok(Box::new(MockProvider::new())),
        _ => anyhow::bail!("Unknown provider: {}", name),
    }
}
```

**Verification**:
- [ ] All providers can be created
- [ ] Unknown provider returns error

## Phase 6: Cleanup

### 6.1 Delete nika-runtime

```bash
rm -rf nika-runtime/
```

### 6.2 Update CLAUDE.md

Remove all references to nika-runtime.

**Verification**:
- [ ] nika-runtime deleted
- [ ] CLAUDE.md updated
- [ ] No broken references

## Phase 7: Final Validation

### 7.1 Tests

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

### 7.2 CLI Validation

```bash
cargo run -- validate nika-docs/spec/examples/*.nika.yaml
cargo run -- run nika-docs/spec/examples/hello-world.nika.yaml --provider mock
```

### 7.3 Provider Tests

```bash
# Mock (always works)
cargo run -- run test.nika.yaml --provider mock

# OpenAI (requires OPENAI_API_KEY)
OPENAI_API_KEY=xxx cargo run -- run test.nika.yaml --provider openai

# Ollama (stub - returns placeholder)
cargo run -- run test.nika.yaml --provider ollama

# Mistral (stub - returns placeholder)
MISTRAL_API_KEY=xxx cargo run -- run test.nika.yaml --provider mistral
```

## Success Criteria

- [ ] All tests pass
- [ ] All providers work (real or stub)
- [ ] CLI validates and runs workflows
- [ ] No regression in existing functionality
- [ ] nika-runtime completely removed
- [ ] Documentation updated

## Rollback Plan

If something breaks:
1. `git stash` or `git checkout .`
2. nika-runtime is still in git history if needed
