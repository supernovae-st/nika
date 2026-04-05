# Provider Setup Guide

Nika supports 7 cloud LLM providers, 1 local inference engine, and 11 MCP service providers. This guide covers how to configure each one, manage API keys securely, test connections, and choose the right provider for your workflows.

## How Providers Work

When a workflow contains `infer:` or `agent:` tasks, Nika needs to know which LLM provider to use. The resolution order is:

1. **Task-level override** -- `provider: openai` on a specific task
2. **Workflow-level default** -- `provider: anthropic` at the top of the workflow
3. **CLI override** -- `nika run file.yaml --provider groq`
4. **Auto-detection** -- Nika checks which API keys are available

If multiple API keys are set, auto-detection follows this priority: Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI.

## LLM Providers (7)

### Anthropic (Claude)

```bash
export ANTHROPIC_API_KEY="sk-ant-api03-..."
```

| Detail | Value |
|--------|-------|
| Provider ID | `anthropic` |
| Aliases | `claude` |
| Env Var | `ANTHROPIC_API_KEY` |
| Key Prefix | `sk-ant-` |
| Models | Claude Opus 4, Claude Sonnet 4, Claude Haiku 3.5 |
| Vision | Yes |
| Extended Thinking | Yes |

Example workflow:

```yaml
schema: nika/workflow@0.12
provider: anthropic
model: claude-sonnet-4-6

tasks:
  - id: analyze
    infer:
      prompt: "Analyze this code for potential bugs."
      system: "You are a senior code reviewer."
      temperature: 0.2
```

To use extended thinking (Claude's chain-of-thought reasoning):

```yaml
  - id: deep_analysis
    infer:
      prompt: "Solve this complex problem step by step."
      extended_thinking: true
      thinking_budget: 10000
```

### OpenAI

```bash
export OPENAI_API_KEY="sk-proj-..."
```

| Detail | Value |
|--------|-------|
| Provider ID | `openai` |
| Aliases | `gpt` |
| Env Var | `OPENAI_API_KEY` |
| Key Prefix | `sk-` |
| Models | GPT-4o, GPT-4o-mini, GPT-4.1, o1, o3 |
| Vision | Yes |

Example:

```yaml
provider: openai
model: gpt-4o

tasks:
  - id: generate
    infer:
      prompt: "Write a product description."
      temperature: 0.7
      max_tokens: 1000
      response_format: json
```

### Mistral AI

```bash
export MISTRAL_API_KEY="your-key-here"
```

| Detail | Value |
|--------|-------|
| Provider ID | `mistral` |
| Aliases | None |
| Env Var | `MISTRAL_API_KEY` |
| Key Prefix | None (any format) |
| Models | Mistral Large, Medium, Small, Codestral |
| Vision | Yes |

Example:

```yaml
provider: mistral
model: mistral-large-latest

tasks:
  - id: translate
    infer:
      prompt: "Translate to French: {{with.text}}"
```

### Groq

```bash
export GROQ_API_KEY="gsk_..."
```

| Detail | Value |
|--------|-------|
| Provider ID | `groq` |
| Aliases | None |
| Env Var | `GROQ_API_KEY` |
| Key Prefix | `gsk_` |
| Models | Llama 4 Maverick, Mixtral 8x7B |
| Vision | Yes |

Groq is optimized for extremely fast inference. Ideal for latency-sensitive workflows:

```yaml
provider: groq
model: llama-4-maverick

tasks:
  - id: quick_classify
    infer:
      prompt: "Classify this text as positive, negative, or neutral: {{with.input}}"
      temperature: 0.0
```

### DeepSeek

```bash
export DEEPSEEK_API_KEY="sk-..."
```

| Detail | Value |
|--------|-------|
| Provider ID | `deepseek` |
| Aliases | `deep-seek` |
| Env Var | `DEEPSEEK_API_KEY` |
| Key Prefix | `sk-` |
| Models | DeepSeek Chat, DeepSeek Coder |
| Vision | No |

```yaml
provider: deepseek
model: deepseek-chat

tasks:
  - id: code_gen
    infer:
      prompt: "Write a Python function to parse CSV files."
```

Note: DeepSeek does not support vision (image) inputs. If you need multimodal capabilities, use Claude, OpenAI, Gemini, or Mistral.

### Google Gemini

```bash
export GEMINI_API_KEY="your-key-here"
```

| Detail | Value |
|--------|-------|
| Provider ID | `gemini` |
| Aliases | `google` |
| Env Var | `GEMINI_API_KEY` |
| Key Prefix | None |
| Models | Gemini Pro, Gemini Flash, Gemini Ultra |
| Vision | Yes |

```yaml
provider: gemini
model: gemini-2.5-flash

tasks:
  - id: multimodal
    infer:
      content:
        - type: image
          source: "{{with.photo.media[0].hash}}"
        - type: text
          text: "Describe what you see in this image."
```

### xAI (Grok)

```bash
export XAI_API_KEY="your-key-here"
```

| Detail | Value |
|--------|-------|
| Provider ID | `xai` |
| Aliases | `grok` |
| Env Var | `XAI_API_KEY` |
| Key Prefix | None |
| Models | Grok-3, Grok-4 |
| Vision | Yes |

```yaml
provider: xai
model: grok-3

tasks:
  - id: research
    infer:
      prompt: "What are the latest developments in quantum computing?"
```

## Local Inference (Native)

Run models locally using GGUF format via mistral.rs. No API key needed.

```bash
# Optional: set a default model path
export NIKA_NATIVE_MODEL_PATH="~/.nika/models/"
```

| Detail | Value |
|--------|-------|
| Provider ID | `native` |
| Aliases | `local` |
| Env Var | `NIKA_NATIVE_MODEL_PATH` (optional) |
| Key Prefix | None |
| Requires Key | No |

### Downloading Models

```bash
# List available models
nika model list

# Download a model
nika model pull qwen3-4b

# Use with vision (requires HuggingFace safetensors, not GGUF)
nika model vision Qwen/Qwen2.5-VL-7B-Instruct --isq Q4K
```

### Using in Workflows

```yaml
provider: native
model: qwen3-4b

tasks:
  - id: local_infer
    infer:
      prompt: "Explain recursion in simple terms."
```

Recommended models by RAM:

| Available RAM | Model | Quality |
|---------------|-------|---------|
| 4 GB | Phi-4 Mini (Q4K) | Good for simple tasks |
| 8 GB | Qwen3 4B (Q4K) | Good general purpose |
| 16 GB | Qwen3 8B (Q4K) | Very good quality |
| 32 GB+ | Llama4 Scout (Q4K) | Near-cloud quality |

## Managing API Keys

### Environment Variables (Simplest)

Add to your shell profile (`~/.zshrc`, `~/.bashrc`):

```bash
export ANTHROPIC_API_KEY="sk-ant-api03-..."
export OPENAI_API_KEY="sk-proj-..."
```

Then reload: `source ~/.zshrc`

### System Keychain (Most Secure)

Store keys in your operating system's secure keychain:

```bash
# Store a key
nika keys set anthropic
# Prompts: Enter API key for Anthropic Claude:

# Store multiple
nika keys set openai
nika keys set mistral
```

Keys stored in the keychain are never written to disk in plain text.

### Migrate from Environment to Keychain

If you have keys in environment variables and want to move them to the keychain:

```bash
nika init --migrate-keys
```

Or migrate individually:

```bash
nika provider migrate
```

## Checking Provider Status

See which providers are configured and ready:

```bash
nika provider list
```

Example output:

```
  LLM Providers
  ─────────────────────────────────

  ✓ anthropic    Claude models (Opus, Sonnet, Haiku)      ANTHROPIC_API_KEY ✓
  ✓ openai       GPT-4, GPT-4o, and other OpenAI models   OPENAI_API_KEY ✓
  ✗ mistral      Mistral Large, Medium, Small models       MISTRAL_API_KEY ✗
  ✓ groq         Fast inference with Llama, Mixtral        GROQ_API_KEY ✓
  ✗ deepseek     DeepSeek Chat and Coder models            DEEPSEEK_API_KEY ✗
  ✗ gemini       Gemini Pro, Flash, and Ultra models       GEMINI_API_KEY ✗
  ✗ xai          Grok models (Grok-3, Grok-4)             XAI_API_KEY ✗

  Local Providers
  ─────────────────────────────────

  ○ native       Local GGUF models via mistral.rs          (no key needed)
```

### Testing a Provider

Verify that a provider connection works:

```bash
nika provider test anthropic
```

This sends a minimal test request and reports success or failure with details.

## Using Multiple Providers in One Workflow

You can mix providers within a single workflow by setting provider overrides at the task level:

```yaml
schema: nika/workflow@0.12
workflow: multi-provider
provider: anthropic  # Default

tasks:
  - id: fast_classify
    provider: groq
    model: llama-4-maverick
    infer:
      prompt: "Classify: {{with.text}}"
      temperature: 0.0

  - id: deep_analysis
    provider: anthropic
    model: claude-sonnet-4-6
    depends_on: [fast_classify]
    with:
      category: $fast_classify
    infer:
      prompt: "Analyze this {{with.category}} text in depth."
      temperature: 0.3

  - id: local_summary
    provider: native
    model: qwen3-4b
    depends_on: [deep_analysis]
    with:
      analysis: $deep_analysis
    infer:
      prompt: "Summarize: {{with.analysis}}"
```

This pattern is powerful for cost optimization: use fast/cheap providers for simple tasks (classification, extraction) and premium providers for complex reasoning.

## Provider Selection Strategy

| Use Case | Recommended Provider | Why |
|----------|---------------------|-----|
| Complex reasoning | Anthropic (Claude Opus/Sonnet) | Best at nuanced, multi-step reasoning |
| Speed-critical | Groq | Fastest inference latency |
| Cost-sensitive | DeepSeek, Groq | Lowest per-token pricing |
| Code generation | Anthropic, OpenAI, DeepSeek | Strong code capabilities |
| Vision/multimodal | Anthropic, OpenAI, Gemini | Best image understanding |
| Offline/privacy | Native (local) | No data leaves your machine |
| Structured JSON | Anthropic, OpenAI | Best schema adherence |

## MCP Service Providers (11)

MCP (Model Context Protocol) providers are external services accessed through the `invoke:` verb. They require separate configuration in the workflow's `mcp:` block:

| Provider | Env Var | Purpose |
|----------|---------|---------|
| Neo4j | `NEO4J_PASSWORD` | Graph database |
| GitHub | `GITHUB_TOKEN` | Repos, issues, PRs |
| Slack | `SLACK_BOT_TOKEN` | Workspace messaging |
| Perplexity | `PERPLEXITY_API_KEY` | Web search |
| Firecrawl | `FIRECRAWL_API_KEY` | Web scraping |
| Supadata | `SUPADATA_API_KEY` | Video transcription |
| DataForSEO | `DATAFORSEO_API_KEY` | SEO data |
| Ahrefs | `AHREFS_API_KEY` | Backlink analysis |
| PostgreSQL | `POSTGRES_URL` | SQL database |
| Filesystem | `FILESYSTEM_ALLOWED_PATHS` | Local file access (no key needed) |
| Memory | `MEMORY_STORAGE_PATH` | Persistent memory (no key needed) |

Example MCP configuration in a workflow:

```yaml
schema: nika/workflow@0.12
workflow: with-mcp

mcp:
  github:
    command: npx
    args: ["-y", "@modelcontextprotocol/server-github"]
    env:
      GITHUB_TOKEN: "{{$env.GITHUB_TOKEN}}"

tasks:
  - id: list_issues
    invoke:
      tool: "github::list_issues"
      params:
        repo: "owner/repo"
        state: open
```

## Auto-Detection

When no provider is specified (neither in the workflow nor on the command line), Nika auto-detects available providers by checking environment variables in order:

1. `ANTHROPIC_API_KEY` (Anthropic/Claude)
2. `OPENAI_API_KEY` (OpenAI)
3. `MISTRAL_API_KEY` (Mistral)
4. `GROQ_API_KEY` (Groq)
5. `DEEPSEEK_API_KEY` (DeepSeek)
6. `GEMINI_API_KEY` (Google Gemini)
7. `XAI_API_KEY` (xAI/Grok)

The first available provider is used. To make this explicit, always set `provider:` in your workflow file.

## Diagnosing Provider Issues

### nika doctor

Run the built-in diagnostic tool:

```bash
nika doctor
```

This checks:
- Which API keys are set
- Whether key formats are valid (correct prefix)
- Project configuration health
- Trace directory status

### Common Issues

**"NIKA-032: Missing API key for provider 'anthropic'"**
The API key is not set. Export it: `export ANTHROPIC_API_KEY="sk-ant-..."`

**"NIKA-030: Provider 'anthropic' not configured"**
The provider name is misspelled or not recognized. Use `nika provider list` to see valid names.

**"NIKA-031: Provider API error"**
The API returned an error. Check:
- Key is valid and not expired
- You have sufficient quota/credits
- The model name is correct

**Key format validation fails**
Each provider has an expected key prefix. If your key does not match, Nika warns you. Double-check you are using the right key for the right provider.

## Model Selection

Each provider offers multiple models with different capabilities and cost profiles. Here is a practical guide to choosing the right model.

### Anthropic Models

| Model | Best For | Context | Cost |
|-------|----------|---------|------|
| `claude-opus-4` | Complex reasoning, research, analysis | 200K | $$$ |
| `claude-sonnet-4-6` | General purpose, coding, balanced | 200K | $$ |
| `claude-haiku-3-5` | Fast responses, simple tasks, classification | 200K | $ |

```yaml
provider: anthropic
model: claude-sonnet-4-6  # Good balance of quality and cost
```

### OpenAI Models

| Model | Best For | Context | Cost |
|-------|----------|---------|------|
| `o3` | Complex reasoning with chain-of-thought | 200K | $$$$ |
| `gpt-4o` | General purpose, multimodal, coding | 128K | $$ |
| `gpt-4o-mini` | Fast, cheap, simple tasks | 128K | $ |
| `gpt-4.1` | Latest GPT-4 variant | 128K | $$ |

```yaml
provider: openai
model: gpt-4o
```

### Groq Models

| Model | Best For | Context | Speed |
|-------|----------|---------|-------|
| `llama-4-maverick` | General purpose, fast inference | 128K | Very fast |
| `mixtral-8x7b-32768` | Multi-task, long context | 32K | Fast |

```yaml
provider: groq
model: llama-4-maverick  # Best speed-to-quality ratio
```

### Specifying Models in Workflows

Set a default model at the workflow level and override per task:

```yaml
schema: nika/workflow@0.12
provider: anthropic
model: claude-haiku-3-5        # Default: fast and cheap

tasks:
  - id: simple_task
    infer:
      prompt: "Classify this text."        # Uses default haiku

  - id: complex_task
    model: claude-sonnet-4-6             # Override: more capable
    infer:
      prompt: "Provide detailed analysis."

  - id: hardest_task
    model: claude-opus-4                  # Override: most capable
    infer:
      prompt: "Solve this multi-step problem."
```

### CLI Model Override

Override the model from the command line:

```bash
nika run workflow.nika.yaml --model gpt-4o
```

This overrides the workflow-level default but not task-level overrides.

## Key Security Best Practices

### Never Commit API Keys

Add API key files to `.gitignore`:

```
# .gitignore
.env
.env.local
*.key
```

### Use Separate Keys for Development and Production

Most providers offer separate API keys for different environments. Use distinct keys to:
- Track costs separately
- Limit production key permissions
- Revoke development keys without affecting production

### Rotate Keys Regularly

If you suspect a key is compromised:

1. Generate a new key in the provider's dashboard
2. Update your environment: `export ANTHROPIC_API_KEY="new-key"`
3. Store in keychain: `nika keys set anthropic`
4. Revoke the old key in the provider's dashboard

### Monitor Usage

Most providers offer usage dashboards. Set up billing alerts to avoid surprises, especially when running workflows with `agent:` tasks that can make many API calls.

## Environment Variable Reference

Complete list of all environment variables Nika reads:

```bash
# LLM Providers
ANTHROPIC_API_KEY          # Claude
OPENAI_API_KEY             # OpenAI
MISTRAL_API_KEY            # Mistral
GROQ_API_KEY               # Groq
DEEPSEEK_API_KEY           # DeepSeek
GEMINI_API_KEY             # Google Gemini
XAI_API_KEY                # xAI (Grok)

# Local Inference
NIKA_NATIVE_MODEL_PATH     # Path to GGUF models

# MCP Services
NEO4J_PASSWORD             # Neo4j graph database
GITHUB_TOKEN               # GitHub API
SLACK_BOT_TOKEN            # Slack workspace
PERPLEXITY_API_KEY         # Perplexity search
FIRECRAWL_API_KEY          # Firecrawl scraping
SUPADATA_API_KEY           # Supadata transcription
DATAFORSEO_API_KEY         # DataForSEO
AHREFS_API_KEY             # Ahrefs SEO
POSTGRES_URL               # PostgreSQL connection string
FILESYSTEM_ALLOWED_PATHS   # Filesystem MCP paths
MEMORY_STORAGE_PATH        # Memory MCP storage
```
