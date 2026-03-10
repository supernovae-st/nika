# v0.13 Synthesis Plan: Complete .nika Integration

**Date:** 2026-02-27
**Status:** Draft
**Research Sources:** 5 parallel agents (Perplexity Sonar, Context7, v0.13 Plans, Claude Code Skills, Memory Patterns)

---

## Executive Summary

The v0.13 `.nika/` directory structure is now **complete** with all directories and configuration files in place. This plan synthesizes research findings and outlines remaining implementation work to achieve full v0.13 functionality.

### Current State (Post-Session)

| Component | Status | Notes |
|-----------|--------|-------|
| `.nika/` structure | ✅ Complete | All 10 directories + 4 config files |
| Multi-format loader | ✅ Complete | `.agent.yaml`, `.skill.yaml`, `.md`, folders |
| `from:` syntax | ✅ Complete | Auto-detection in resolver |
| `nika init` | ✅ Complete | Creates full structure with examples |
| Boot sequence | ⏳ TODO | Load config at startup |
| Policy enforcement | ⏳ TODO | `policies.yaml` enforcement |
| Memory system | ⏳ TODO | `memory.yaml` implementation |
| Sub-workflow composition | ⏳ TODO | `nika:run` for nested workflows |

---

## Directory Structure (Final)

```
.nika/
├── config.toml          # Main configuration (theme, editor, providers)
├── user.yaml            # User profile (name, timezone, preferences)
├── memory.yaml          # Memory system configuration
├── policies.yaml        # Security policies (execution, budget, network)
├── agents/              # Agent definitions
│   └── researcher.md    # Example agent (markdown with frontmatter)
├── skills/              # Skill definitions
│   └── code-review.md   # Example skill
├── context/             # Shared context files
│   └── project.md       # Project-level context
├── workflows/           # Sub-workflows (called via nika:run)
│   └── helpers.nika.yaml # Reusable helper workflows
├── memory/              # Persistent memory storage
├── proposed/            # Agent-proposed changes
└── cache/               # Temporary cache
```

---

## Research Findings Synthesis

### 1. Perplexity Sonar Integration

**API Capabilities:**
- Models: `sonar` (cost-effective), `sonar-pro` (advanced), `sonar-reasoning` (complex queries)
- Real-time web search with citation
- Supports streaming responses
- Compatible with OpenAI-style API

**MCP Integration Pattern:**
```yaml
# Example: Perplexity MCP in workflow
mcp:
  perplexity:
    command: npx
    args: ["-y", "@anthropic/perplexity-mcp"]
    env:
      PERPLEXITY_API_KEY: "$PERPLEXITY_API_KEY"

tasks:
  - id: research
    invoke:
      tool: perplexity_search
      server: perplexity
      params:
        query: "{{use.topic}}"
```

**Recommendation:** Add Perplexity as first-class MCP server option in `nika init --with-perplexity`.

### 2. Context7 Integration

**Available Tools:**
- `resolve-library-id`: Package name → Context7 library ID
- `get-library-docs`: Fetch up-to-date documentation

**Auto-Invoke Pattern (Claude Code style):**
```yaml
agents:
  docs-expert:
    from: .nika/agents/docs-expert
    tools:
      - context7:resolve-library-id
      - context7:get-library-docs
    auto_invoke:
      - trigger: "how do I", "best practices", "documentation"
        tool: context7:get-library-docs
```

**Recommendation:** Implement auto-invoke system for v0.14 based on keyword triggers.

### 3. Memory System Architecture

**Scope Hierarchy:**
```
conversation (ephemeral, per-session)
    ↓
task (per-workflow execution)
    ↓
session (persistent across restarts, TTL-based)
    ↓
project (long-term, .nika/memory/)
    ↓
global (~/.nika/memory/)
```

**Storage Backends:**
1. **File** (default): JSON files in `.nika/memory/`
2. **SQLite**: Single database for faster queries
3. **Redis**: Distributed/shared memory

**memory.yaml Implementation:**
```yaml
enabled: true
backend: file
path: .nika/memory/

scopes:
  conversation:
    persist: false
    max_entries: 100

  session:
    persist: true
    ttl_secs: 86400  # 24 hours

  project:
    persist: true
    ttl_secs: 0  # No expiry

# RAG integration (v0.14)
retrieval:
  enabled: false
  embedding_model: text-embedding-3-small
  similarity_threshold: 0.7
```

### 4. Claude Code Skills Structure

**SKILL.md Frontmatter Format:**
```markdown
---
name: skill-name
description: Short description for discovery
trigger: keywords, that, activate, this, skill
tools: [tool1, tool2]
---

# Skill Instructions

Full skill content as system prompt...
```

**Discovery Mechanism:**
1. Scan `.nika/skills/` for `*.skill.yaml`, `*.md`, `SKILL.md`
2. Parse frontmatter for metadata
3. Register in skill registry
4. Match triggers against user prompts

### 5. Policy Enforcement

**policies.yaml Enforcement Points:**

| Policy | Enforcement Point | Implementation |
|--------|-------------------|----------------|
| `execution.allow_commands` | `exec:` verb | Glob pattern matching |
| `execution.block_commands` | `exec:` verb | Hard block, no override |
| `execution.confirm_destructive` | `exec:` verb | TUI confirmation dialog |
| `budget.daily_token_limit` | All `infer:` | Token counter middleware |
| `network.block_domains` | `fetch:` verb | URL parsing + deny |

---

## Implementation Roadmap

### Phase 1: Boot Sequence (v0.13.1)

**Goal:** Load `.nika/config.toml` at TUI startup.

```rust
// In src/tui/app.rs
pub fn new() -> Result<Self, NikaError> {
    // 1. Discover .nika/ root
    let nika_root = discover_nika_root()?;

    // 2. Load config
    let config = NikaConfig::load_or_default(&nika_root)?;

    // 3. Apply theme
    let theme = Theme::from_config(&config);

    // 4. Load user profile
    let user = UserProfile::load(&nika_root)?;

    Ok(Self { config, theme, user, .. })
}
```

**Files to modify:**
- `src/tui/app.rs` — Add config loading
- `src/tui/config.rs` — Add NikaConfig struct
- `src/core/config.rs` — Root discovery with `.nika/`

### Phase 2: Policy Enforcement (v0.13.2)

**Goal:** Enforce `policies.yaml` during workflow execution.

```rust
// In src/runtime/executor.rs
impl Executor {
    async fn check_exec_policy(&self, command: &str) -> Result<(), NikaError> {
        let policies = self.policies.as_ref();

        // Check block list first (highest priority)
        for pattern in &policies.execution.block_commands {
            if glob_match(pattern, command) {
                return Err(NikaError::PolicyViolation {
                    reason: format!("Command '{}' is blocked by policy", command),
                });
            }
        }

        // Check allow list
        if !policies.execution.allow_commands.is_empty() {
            let allowed = policies.execution.allow_commands.iter()
                .any(|p| glob_match(p, command));
            if !allowed {
                return Err(NikaError::PolicyViolation {
                    reason: format!("Command '{}' is not in allow list", command),
                });
            }
        }

        // Confirm destructive commands
        if policies.execution.confirm_destructive && is_destructive(command) {
            self.confirm_destructive(command).await?;
        }

        Ok(())
    }
}
```

### Phase 3: Memory System (v0.13.3)

**Goal:** Implement persistent memory with scopes.

```rust
// In src/memory/mod.rs
pub struct MemorySystem {
    config: MemoryConfig,
    backend: Box<dyn MemoryBackend>,
    scopes: HashMap<String, MemoryScope>,
}

impl MemorySystem {
    pub async fn get(&self, scope: &str, key: &str) -> Option<Value> {
        self.backend.get(scope, key).await
    }

    pub async fn set(&self, scope: &str, key: &str, value: Value) -> Result<(), NikaError> {
        let config = self.scopes.get(scope).unwrap_or_default();

        // Check entry limit
        if let Some(max) = config.max_entries {
            // Evict oldest if needed
        }

        self.backend.set(scope, key, value, config.ttl_secs).await
    }

    pub async fn flush_scope(&self, scope: &str) -> Result<(), NikaError> {
        self.backend.clear_scope(scope).await
    }
}
```

**Backends:**
1. `FileBackend` — JSON files per scope
2. `SqliteBackend` — Single SQLite database
3. `RedisBackend` — Redis connection (optional feature)

### Phase 4: Sub-Workflow Composition (v0.13.4)

**Goal:** Enable `nika:run` builtin for nested workflows.

Already implemented via `nika:run` builtin. Verify:
- Parent workflow can call child workflow
- Results propagate back to parent
- Traces remain separate but linked

```yaml
# Parent workflow
tasks:
  - id: get_context
    infer: "Gather context"

  - id: summarize
    invoke:
      tool: nika:run
      params:
        workflow: .nika/workflows/helpers.nika.yaml
        # Optional: specify which task to run
        task: summarize
        input: "{{use.context}}"
```

---

## Test Plan

### Unit Tests

| Test | Description | File |
|------|-------------|------|
| `test_load_config_default` | Default config when no .nika/ | `src/tui/config.rs` |
| `test_load_config_from_file` | Parse config.toml | `src/tui/config.rs` |
| `test_policy_block_command` | Block list enforcement | `src/runtime/executor.rs` |
| `test_policy_allow_command` | Allow list enforcement | `src/runtime/executor.rs` |
| `test_memory_scope_isolation` | Scopes don't leak | `src/memory/mod.rs` |
| `test_memory_ttl_expiry` | TTL-based cleanup | `src/memory/mod.rs` |

### Integration Tests

| Test | Description | Workflow |
|------|-------------|----------|
| E2E boot sequence | TUI loads config | Manual |
| E2E policy enforcement | Commands blocked | `test-policy.nika.yaml` |
| E2E nested workflow | nika:run composition | `test-nested.nika.yaml` |
| E2E memory persistence | Survives restart | `test-memory.nika.yaml` |

---

## Timeline Estimate

| Phase | Complexity | Dependencies |
|-------|------------|--------------|
| Phase 1: Boot Sequence | Low | None |
| Phase 2: Policy Enforcement | Medium | Phase 1 |
| Phase 3: Memory System | High | Phase 1 |
| Phase 4: Sub-Workflow | Low | Already implemented |

---

## References

- Perplexity API: https://docs.perplexity.ai/
- Context7 MCP: https://github.com/context7/context7-mcp
- Claude Code Skills: https://docs.anthropic.com/claude/docs/claude-code
- Nika v0.13 Plans: `docs/plans/2026-02-27-v013-*`

---

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
