# 06 -- Provider System

## Overview

Nika's provider system abstracts 7 cloud LLM providers and 1 local inference backend behind a unified interface. The implementation uses [rig-core](https://github.com/0xPlaygrounds/rig) v0.32 as the provider abstraction layer.

---

## Provider Architecture

```
Workflow YAML
    |
    v
provider: "claude" / task.provider: "openai"
    |
    v
RigProvider::from_name(name)
    |
    v
core::find_provider(name) -- canonical provider lookup
    |
    v
+-- RigProvider::Claude(anthropic::Client)
+-- RigProvider::OpenAI(openai::Client)
+-- RigProvider::Mistral(mistral::Client)
+-- RigProvider::Groq(groq::Client)
+-- RigProvider::DeepSeek(deepseek::Client)
+-- RigProvider::Gemini(gemini::Client)
+-- RigProvider::XAi(xai::Client)
+-- RigProvider::Native(NativeRuntime)  [feature: native-inference]
```

### Provider Caching

The `TaskExecutor` caches `RigProvider` instances in a `DashMap<String, RigProvider>`. The cache is keyed by provider name string. Once a provider is created, it is reused for all subsequent requests with the same name. This avoids redundant API key lookups and client construction.

---

## Cloud Providers (7)

| ID | Name | Aliases | Env Var | Key Prefix | Models |
|----|------|---------|---------|------------|--------|
| `anthropic` | Anthropic Claude | `claude` | `ANTHROPIC_API_KEY` | `sk-ant-` | Opus, Sonnet, Haiku |
| `openai` | OpenAI | `gpt` | `OPENAI_API_KEY` | `sk-` | GPT-4, GPT-4o |
| `mistral` | Mistral AI | -- | `MISTRAL_API_KEY` | -- | Large, Medium, Small |
| `groq` | Groq | -- | `GROQ_API_KEY` | `gsk_` | Llama, Mixtral |
| `deepseek` | DeepSeek | `deep-seek` | `DEEPSEEK_API_KEY` | `sk-` | Chat, Coder |
| `gemini` | Google Gemini | `google` | `GEMINI_API_KEY` | -- | Pro, Flash, Ultra |
| `xai` | xAI Grok | `grok` | `XAI_API_KEY` | -- | Grok-3, Grok-4 |

### Provider Resolution

Provider names are resolved through `core::find_provider()`, which is the single source of truth. The function accepts both canonical IDs and aliases:

- `"claude"` -> `Provider { id: "anthropic", ... }`
- `"gpt"` -> `Provider { id: "openai", ... }`
- `"grok"` -> `Provider { id: "xai", ... }`
- `"google"` -> `Provider { id: "gemini", ... }`
- `"deep-seek"` -> `Provider { id: "deepseek", ... }`

### API Key Resolution

Keys are resolved in priority order:

1. **Environment variables** (highest priority): `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.
2. **System keychain** (feature: `native-keychain`): OS-native credential storage (macOS Keychain, Windows Credential Manager, Linux Secret Service)
3. **Nika daemon** (via nika-daemon crate): Unified secret management via IPC
4. **Config file** (lowest priority): `~/.config/nika/config.toml`

### Key Validation

Some providers have known key prefixes that enable format validation without making API calls:

```rust
pub fn validate_key_format(provider_id: &str, key: &str) -> bool {
    match find_provider(provider_id) {
        Some(p) => match p.key_prefix {
            Some(prefix) => key.starts_with(prefix),
            None => true,  // No prefix = no format validation
        },
        None => true,
    }
}
```

---

## MCP Providers (11)

These are not LLM providers but MCP server providers that require API keys:

| ID | Name | Env Var | Description |
|----|------|---------|-------------|
| `neo4j` | Neo4j | `NEO4J_PASSWORD` | Graph database |
| `github` | GitHub | `GITHUB_TOKEN` | GitHub API |
| `slack` | Slack | `SLACK_BOT_TOKEN` | Slack API |
| `perplexity` | Perplexity | `PERPLEXITY_API_KEY` | Search API |
| `firecrawl` | Firecrawl | `FIRECRAWL_API_KEY` | Web crawling |
| `supadata` | Supadata | `SUPADATA_API_KEY` | Data extraction |
| `dataforseo` | DataForSEO | `DATAFORSEO_API_KEY` | SEO data |
| `ahrefs` | Ahrefs | `AHREFS_API_KEY` | SEO analytics |
| `postgres` | PostgreSQL | `POSTGRES_URL` | Database |
| `filesystem` | Filesystem | -- | Local files |
| `memory` | Memory | -- | In-memory storage |

---

## Local Provider (1)

### Native Inference

**Feature:** `native-inference`

**Dependency:** `mistralrs` v0.7

Local LLM inference via mistral.rs. Models are GGUF files stored in `~/.nika/models/`.

```yaml
provider: native
model: Qwen/Qwen2.5-7B-Instruct
```

### Model Management

```bash
nika model pull mistral-7b          # Download GGUF model
nika model list                      # List installed models
nika model info mistral-7b           # Show model details
nika model vision Qwen/Qwen2.5-VL-7B-Instruct --isq Q4K  # Vision model
```

### Known Models (15+)

The `catalogs/models.rs` module defines curated models with metadata:

```rust
pub struct KnownModel {
    pub id: &'static str,
    pub name: &'static str,
    pub architecture: ModelArchitecture,
    pub model_type: ModelType,
    pub default_quantization: Quantization,
    pub recommended_ram_gb: f32,
}
```

Model types: `TextGeneration`, `VisionHf` (HuggingFace + ISQ for vision)

Quantization options: `Q2K`, `Q3K`, `Q4_0`, `Q4K`, `Q5K`, `Q6K`, `Q8_0`, `F16`, `F32`

Auto-quantization selects based on available RAM:
```rust
pub fn auto_select_quantization(available_ram_gb: f32) -> Quantization {
    if available_ram_gb >= 32.0 { Q6K }
    else if available_ram_gb >= 16.0 { Q4K }
    else { Q3K }
}
```

### Vision via Native (Not GGUF)

GGUF models are text-only. For native vision, use `NativeModelKind::VisionHf`:

```bash
nika model vision Qwen/Qwen2.5-VL-7B-Instruct --isq Q4K
```

Recommended vision models:
- Gemma 3 4B (~3 GB)
- Qwen2.5-VL 7B (~5 GB)
- Gemma 3 12B (~8 GB)

---

## Mock Provider

The `mock` provider enables testing without API keys:

```yaml
provider: mock
```

Mock behavior:
- Returns deterministic JSON with common test fields (`title`, `summary`, `items`)
- For vision content, includes content metadata in mock response
- No API call is made
- Zero cost, instant response

---

## Cost Tracking

### ProviderKind

```rust
pub enum ProviderKind {
    Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, XAi, Native,
}
```

### ModelPricing

```rust
pub struct ModelPricing {
    pub input_per_million: f64,   // USD per million input tokens
    pub output_per_million: f64,  // USD per million output tokens
}
```

### Cost Calculation

```rust
pub fn calculate_cost(
    provider: ProviderKind,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> f64
```

Pricing tables are stored as `LazyLock<HashMap<&str, ModelPricing>>` per provider. Native provider is always free (`ModelPricing::new(0.0, 0.0)`).

### Cost Display

```rust
pub fn format_cost(cost: f64) -> String
// $0.00 -> "free"
// $0.001234 -> "$0.0012"
// $1.50 -> "$1.50"
```

---

## MCP Tool Integration via rig-core

Nika wraps MCP tools as rig-core `ToolDyn` implementations to bridge the rmcp version gap (rig-core uses rmcp 0.13, Nika uses rmcp 0.16):

```
NikaMcpToolDef (Nika's definition)
       |
       v
NikaMcpTool (implements rig::tool::ToolDyn)
       |
       v
rig::AgentBuilder.tool()
```

### NikaMcpTool

```rust
pub struct NikaMcpTool {
    def: NikaMcpToolDef,
    client: Arc<McpClient>,
    media_staging: AgentMediaStaging,
}
```

When a tool call returns binary content blocks (images, files), they are collected in the shared `media_staging` DashMap since rig's `ToolDyn::call()` can only return `String`.

### Stop Sequences Workaround

rig-core v0.32 has no `.stop_sequences()` on AgentBuilder. Nika injects them via `additional_params` which is `#[serde(flatten)]`-ed into the request body:

```rust
fn stop_sequences_params(provider: &str, sequences: &[String]) -> Option<serde_json::Value> {
    let key = match provider {
        "anthropic" | "claude" => "stop_sequences",
        "gemini" => "stopSequences",
        _ => "stop",  // OpenAI, Mistral, Groq, DeepSeek, xAI
    };
    Some(serde_json::json!({ key: sequences }))
}
```

---

## Provider Auto-Detection

`RigProvider::auto()` detects the best available provider by checking environment variables in order:

1. `ANTHROPIC_API_KEY` -> Claude
2. `OPENAI_API_KEY` -> OpenAI
3. `MISTRAL_API_KEY` -> Mistral
4. `GROQ_API_KEY` -> Groq
5. `DEEPSEEK_API_KEY` -> DeepSeek
6. `GEMINI_API_KEY` -> Gemini
7. `XAI_API_KEY` -> xAI
8. Check for local models -> Native

If no provider is available, returns `NIKA-032 MissingApiKey`.

---

## MCP Aliases (100)

The `catalogs/mcp_aliases.rs` module defines 100 short-name aliases for popular MCP servers, organized by category:

```rust
pub struct McpAlias {
    pub name: &'static str,
    pub category: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    pub env_var: Option<&'static str>,
    pub description: &'static str,
    pub pricing: McpPricing,
}
```

Categories include: databases, search, development, communication, analytics, infrastructure, media, and more.

Usage in workflows:
```yaml
mcp:
  github:     # Resolves to npx @modelcontextprotocol/server-github
    env:
      GITHUB_TOKEN: $GITHUB_TOKEN
```

See [11-configuration-reference.md](./11-configuration-reference.md) for MCP alias configuration details.
