# v0.13 — Chat ↔ Workflow Unified Execution + Schema @0.6

**Date:** 2026-02-26
**Author:** Claude Opus 4.5
**Status:** PLAN READY FOR EXECUTION
**Target:** v0.13.0

---

## Executive Summary

Ce plan unifie l'exécution Chat et Workflow YAML avec le nouveau schema `nika/workflow@0.6`.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  OBJECTIF v0.13                                                               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Chat → ChatWorkflow → Executor → DataStore → Export YAML @0.6                ║
║                                                                               ║
║  L'utilisateur parle dans le chat, Nika construit un DAG en temps réel,       ║
║  puis exporte en workflow YAML réutilisable et éditable.                      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Key Insight:** Le runtime `ChatWorkflow` est DÉJÀ IMPLÉMENTÉ (1014 lignes, 40+ tests).
Le gap est uniquement le **wiring TUI** + les **features schema @0.6**.

---

## Part 1: État Actuel (Gap Analysis)

### Implémenté ✅

| Component | Location | Status |
|-----------|----------|--------|
| ChatWorkflow | `src/runtime/chat_workflow.rs` | 1014 lignes, 40+ tests |
| StableDag wrapper | `ChatWorkflow.dag` | Node/Edge management |
| Mention System | `src/binding/mention.rs` | @N, @last, @all, @N..M |
| nika:run builtin | `src/runtime/builtin/run.rs` | Execute nested workflows |
| ChatDagPanel | `src/tui/widgets/chat_dag_panel.rs` | DAG visualization |
| DataStore | `src/store/datastore.rs` | Lock-free task results |

### Manquant ❌

| Component | Gap | Priority |
|-----------|-----|----------|
| ChatView ↔ ChatWorkflow | ChatView uses ChatAgent directly, not ChatWorkflow | P0 |
| to_yaml() export | No method to export ChatWorkflow to YAML | P0 |
| ChatDagPanel sync | Syncs from messages, not from ChatWorkflow.dag | P1 |
| Schema @0.6 AST | memory, agents, skills not in ast/ | P1 |
| Memory loader | Load files declared in memory: block | P2 |
| Agent definitions | Parse and resolve agent references | P2 |
| Skill definitions | Parse and apply skill augmentation | P2 |

---

## Part 2: Schema nika/workflow@0.6

### New Top-Level Fields

```yaml
schema: "nika/workflow@0.6"
workflow: my-workflow

# NEW in @0.6
memory:
  files:
    brand: ./context/brand.md        # Markdown → string
    persona: ./context/persona.json  # JSON → parsed object
    examples: ./context/*.md         # Glob → array of strings
  session: .nika/sessions/prev.json  # Session restore

agents:
  researcher:
    file: ./agents/researcher.agent.yaml  # External definition
  translator:
    system: "You are a translator..."     # Inline definition
    provider: claude
    model: claude-sonnet-4-6
    max_turns: 3

skills:
  tdd: ./skills/tdd.skill.yaml
  seo: ./skills/seo.skill.yaml

# Existing
mcp: { ... }
tasks: [ ... ]
flows: [ ... ]
```

### New Task Syntax

```yaml
tasks:
  - id: research
    agent:
      use: researcher           # NEW: reference defined agent
      skill: seo                # NEW: apply skill
      prompt: |
        Research trends.
        Brand: {{memory.files.brand}}
```

### New Binding Expressions

| Expression | Resolves To |
|------------|-------------|
| `{{memory.files.brand}}` | Content of `./context/brand.md` |
| `{{memory.files.persona.name}}` | JSON path in persona.json |
| `{{memory.session.focus_areas}}` | Data from previous session |
| `{{use.task_id}}` | Result from previous task (existing) |

---

## Part 3: Implementation Plan

### Phase 1: Wire ChatWorkflow to ChatView (P0)

**Estimated:** 2-3 hours
**Files:** `src/tui/views/chat.rs`, `src/tui/app.rs`

#### Task 1.1: Add ChatWorkflow field to ChatView

```rust
// src/tui/views/chat.rs

use crate::runtime::chat_workflow::{ChatWorkflow, ChatMessage, Role};

pub struct ChatView {
    // ... existing fields ...

    /// v0.13: DAG workflow built from chat messages
    pub workflow: ChatWorkflow,
}

impl Default for ChatView {
    fn default() -> Self {
        Self {
            // ... existing ...
            workflow: ChatWorkflow::new(),
        }
    }
}
```

#### Task 1.2: Wire message handling to ChatWorkflow

```rust
// src/tui/views/chat.rs - in handle_chat_infer() or equivalent

// After user sends message:
let user_idx = self.workflow.add_message_with_mentions(&user_prompt, Role::User)?;

// After assistant responds:
let assistant_idx = self.workflow.add_message(&response, Role::Assistant);
```

#### Task 1.3: Sync ChatDagPanel from ChatWorkflow.dag

```rust
// src/tui/views/chat.rs - sync_dag_from_messages() replacement

fn sync_dag_from_workflow(&mut self) {
    self.dag_panel.clear();

    for node_idx in self.workflow.dag.node_indices() {
        let msg = self.workflow.dag.node_weight(node_idx).unwrap();
        self.dag_panel.add_node(node_idx, msg);
    }

    for edge in self.workflow.dag.edge_references() {
        self.dag_panel.add_edge(edge.source(), edge.target());
    }
}
```

#### Task 1.4: Tests

- `test_chat_message_creates_workflow_node`
- `test_chat_mention_creates_edge`
- `test_parallel_prefix_creates_parallel_node`
- `test_dag_panel_syncs_from_workflow`

---

### Phase 2: Add to_yaml() Export (P0)

**Estimated:** 2-3 hours
**Files:** `src/runtime/chat_workflow.rs`, `src/tui/command.rs`

#### Task 2.1: Implement to_yaml() on ChatWorkflow

```rust
// src/runtime/chat_workflow.rs

impl ChatWorkflow {
    /// Export chat conversation to nika/workflow@0.6 YAML
    pub fn to_yaml(&self, workflow_name: &str) -> String {
        let mut yaml = String::new();

        // Header
        yaml.push_str("schema: \"nika/workflow@0.6\"\n");
        yaml.push_str(&format!("workflow: {}\n", workflow_name));
        yaml.push_str(&format!("description: \"Exported from chat session\"\n\n"));

        // Tasks
        yaml.push_str("tasks:\n");
        for node_idx in self.dag.node_indices() {
            let msg = self.dag.node_weight(node_idx).unwrap();
            yaml.push_str(&self.message_to_yaml_task(msg, node_idx));
        }

        // Flows
        yaml.push_str("\nflows:\n");
        for edge in self.dag.edge_references() {
            let source_id = self.node_to_task_id(edge.source());
            let target_id = self.node_to_task_id(edge.target());
            yaml.push_str(&format!("  - source: {}\n    target: {}\n", source_id, target_id));
        }

        yaml
    }

    fn message_to_yaml_task(&self, msg: &ChatMessage, idx: NodeIndex) -> String {
        let task_id = self.node_to_task_id(idx);
        let verb = self.role_to_verb(msg.role);

        // Convert message content to appropriate verb
        match msg.verb {
            Some(Verb::Infer) => format!("  - id: {}\n    infer: \"{}\"\n", task_id, msg.content),
            Some(Verb::Exec) => format!("  - id: {}\n    exec: \"{}\"\n", task_id, msg.content),
            // ... other verbs
            None => format!("  - id: {}\n    infer: \"{}\"\n", task_id, msg.content),
        }
    }
}
```

#### Task 2.2: Add /export yaml command

```rust
// src/tui/command.rs

pub enum ChatCommand {
    // ... existing ...
    ExportYaml { path: Option<String> },
}

// Parse "/export yaml [path]"
```

#### Task 2.3: Handle export in App

```rust
// src/tui/app.rs

async fn handle_export_yaml(&mut self, path: Option<String>) -> Result<(), NikaError> {
    let yaml = self.chat_view.workflow.to_yaml("chat-export");

    let output_path = path.unwrap_or_else(|| {
        format!(".nika/exports/chat-{}.nika.yaml", chrono::Utc::now().format("%Y%m%d-%H%M%S"))
    });

    tokio::fs::write(&output_path, &yaml).await?;
    self.chat_view.add_system_message(&format!("Exported to {}", output_path));

    Ok(())
}
```

#### Task 2.4: Tests

- `test_to_yaml_single_infer`
- `test_to_yaml_with_mentions_creates_flows`
- `test_to_yaml_parallel_tasks`
- `test_export_yaml_command_writes_file`

---

### Phase 3: Schema @0.6 AST Types (P1)

**Estimated:** 3-4 hours
**Files:** `src/ast/workflow.rs`, `src/ast/memory.rs` (new), `src/ast/agent_def.rs` (new)

#### Task 3.1: Add schema constant

```rust
// src/ast/workflow.rs

pub const SCHEMA_V06: &str = "nika/workflow@0.6";
```

#### Task 3.2: Create Memory AST types

```rust
// src/ast/memory.rs (NEW FILE)

use std::collections::HashMap;
use serde::Deserialize;

/// Memory configuration for workflow (v0.6)
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MemoryConfig {
    /// Files to load at workflow start
    #[serde(default)]
    pub files: HashMap<String, String>,  // alias → path

    /// Session file to restore
    pub session: Option<String>,
}
```

#### Task 3.3: Create AgentDef AST types

```rust
// src/ast/agent_def.rs (NEW FILE)

use serde::Deserialize;

/// Agent definition (v0.6)
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AgentDef {
    /// External file reference
    External { file: String },

    /// Inline definition
    Inline {
        system: String,
        #[serde(default = "default_provider")]
        provider: String,
        model: Option<String>,
        max_turns: Option<u32>,
        temperature: Option<f32>,
    },
}

fn default_provider() -> String {
    "claude".to_string()
}
```

#### Task 3.4: Create SkillDef AST types

```rust
// src/ast/skill_def.rs (NEW FILE)

use serde::Deserialize;

/// Skill reference (v0.6)
pub type SkillDef = String;  // Path to skill file

/// Skill application in task
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SkillRef {
    Single(String),
    Multiple(Vec<String>),
}
```

#### Task 3.5: Update Workflow struct

```rust
// src/ast/workflow.rs

use super::memory::MemoryConfig;
use super::agent_def::AgentDef;

#[derive(Debug, Clone, Deserialize)]
pub struct Workflow {
    pub schema: String,
    pub workflow: String,
    pub description: Option<String>,

    // NEW in @0.6
    #[serde(default)]
    pub memory: Option<MemoryConfig>,

    #[serde(default)]
    pub agents: HashMap<String, AgentDef>,

    #[serde(default)]
    pub skills: HashMap<String, String>,

    // Existing
    #[serde(default)]
    pub mcp: HashMap<String, McpConfigInline>,

    pub tasks: Vec<Task>,

    #[serde(default)]
    pub flows: Vec<Flow>,
}
```

#### Task 3.6: Update AgentParams for agent.use

```rust
// src/ast/action.rs

#[derive(Debug, Clone, Deserialize)]
pub struct AgentParams {
    // Existing
    pub prompt: String,
    #[serde(default)]
    pub mcp: Vec<String>,
    pub max_turns: Option<u32>,

    // NEW in @0.6
    /// Reference to defined agent
    #[serde(rename = "use")]
    pub use_agent: Option<String>,

    /// Skill(s) to apply
    pub skill: Option<SkillRef>,
}
```

---

### Phase 4: Memory Loader (P2)

**Estimated:** 2-3 hours
**Files:** `src/runtime/memory_loader.rs` (new)

#### Task 4.1: Implement MemoryLoader

```rust
// src/runtime/memory_loader.rs (NEW FILE)

use std::collections::HashMap;
use std::path::Path;
use serde_json::Value;
use glob::glob;

use crate::ast::memory::MemoryConfig;
use crate::error::NikaError;

/// Loaded memory context
#[derive(Debug, Clone, Default)]
pub struct LoadedMemory {
    pub files: HashMap<String, Value>,
    pub session: Option<Value>,
}

/// Load memory files at workflow start
pub async fn load_memory(config: &MemoryConfig, base_path: &Path) -> Result<LoadedMemory, NikaError> {
    let mut memory = LoadedMemory::default();

    for (alias, path_pattern) in &config.files {
        let full_path = base_path.join(path_pattern);

        if path_pattern.contains('*') {
            // Glob pattern → array of strings
            let files = load_glob_files(&full_path.to_string_lossy())?;
            memory.files.insert(alias.clone(), Value::Array(files));
        } else {
            // Single file
            let content = load_single_file(&full_path).await?;
            memory.files.insert(alias.clone(), content);
        }
    }

    if let Some(session_path) = &config.session {
        let full_path = base_path.join(session_path);
        if full_path.exists() {
            let content = tokio::fs::read_to_string(&full_path).await?;
            memory.session = Some(serde_json::from_str(&content)?);
        }
    }

    Ok(memory)
}

async fn load_single_file(path: &Path) -> Result<Value, NikaError> {
    let content = tokio::fs::read_to_string(path).await?;

    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => Ok(serde_json::from_str(&content)?),
        Some("yaml") | Some("yml") => Ok(serde_yaml::from_str(&content)?),
        _ => Ok(Value::String(content)),  // Markdown, txt, etc.
    }
}

fn load_glob_files(pattern: &str) -> Result<Vec<Value>, NikaError> {
    let mut results = Vec::new();
    for entry in glob(pattern)? {
        let path = entry?;
        let content = std::fs::read_to_string(&path)?;
        results.push(Value::String(content));
    }
    Ok(results)
}
```

#### Task 4.2: Integrate with Runner

```rust
// src/runtime/runner.rs

use crate::runtime::memory_loader::{load_memory, LoadedMemory};

impl Runner {
    pub async fn run(&self) -> Result<String, NikaError> {
        // NEW: Load memory if present
        let memory = if let Some(mem_config) = &self.workflow.memory {
            load_memory(mem_config, &self.base_path).await?
        } else {
            LoadedMemory::default()
        };

        // Store memory in DataStore for binding resolution
        self.datastore.set_memory(memory);

        // ... rest of execution
    }
}
```

---

### Phase 5: Agent & Skill Resolution (P2)

**Estimated:** 2-3 hours
**Files:** `src/runtime/agent_resolver.rs` (new)

#### Task 5.1: Implement AgentResolver

```rust
// src/runtime/agent_resolver.rs (NEW FILE)

use std::path::Path;
use crate::ast::agent_def::AgentDef;
use crate::error::NikaError;

/// Resolved agent configuration
#[derive(Debug, Clone)]
pub struct ResolvedAgent {
    pub system_prompt: String,
    pub provider: String,
    pub model: Option<String>,
    pub max_turns: u32,
    pub temperature: Option<f32>,
}

pub async fn resolve_agent(
    def: &AgentDef,
    base_path: &Path,
) -> Result<ResolvedAgent, NikaError> {
    match def {
        AgentDef::External { file } => {
            let full_path = base_path.join(file);
            let content = tokio::fs::read_to_string(&full_path).await?;
            let inline: AgentDefInline = serde_yaml::from_str(&content)?;
            Ok(ResolvedAgent {
                system_prompt: inline.system,
                provider: inline.provider,
                model: inline.model,
                max_turns: inline.max_turns.unwrap_or(10),
                temperature: inline.temperature,
            })
        }
        AgentDef::Inline { system, provider, model, max_turns, temperature } => {
            Ok(ResolvedAgent {
                system_prompt: system.clone(),
                provider: provider.clone(),
                model: model.clone(),
                max_turns: max_turns.unwrap_or(10),
                temperature: *temperature,
            })
        }
    }
}
```

#### Task 5.2: Implement SkillLoader

```rust
// src/runtime/skill_loader.rs (NEW FILE)

use std::path::Path;
use crate::error::NikaError;

/// Load skill content and append to system prompt
pub async fn load_skill(
    skill_path: &str,
    base_path: &Path,
) -> Result<String, NikaError> {
    let full_path = base_path.join(skill_path);
    let content = tokio::fs::read_to_string(&full_path).await?;

    // Skills are YAML with a `content` field containing the prompt augmentation
    let skill: SkillFile = serde_yaml::from_str(&content)?;
    Ok(skill.content)
}

#[derive(Debug, Deserialize)]
struct SkillFile {
    name: String,
    description: Option<String>,
    content: String,  // The actual skill prompt
}
```

---

## Part 4: File Summary

### New Files

| File | Purpose |
|------|---------|
| `src/ast/memory.rs` | MemoryConfig AST type |
| `src/ast/agent_def.rs` | AgentDef AST type |
| `src/ast/skill_def.rs` | SkillDef AST type |
| `src/runtime/memory_loader.rs` | Load memory files |
| `src/runtime/agent_resolver.rs` | Resolve agent definitions |
| `src/runtime/skill_loader.rs` | Load skill augmentations |

### Modified Files

| File | Changes |
|------|---------|
| `src/ast/workflow.rs` | Add memory, agents, skills fields + SCHEMA_V06 |
| `src/ast/action.rs` | Add use_agent, skill to AgentParams |
| `src/ast/mod.rs` | Export new modules |
| `src/runtime/chat_workflow.rs` | Add to_yaml() method |
| `src/runtime/runner.rs` | Integrate memory loader |
| `src/runtime/executor.rs` | Resolve agent/skill before execution |
| `src/tui/views/chat.rs` | Add workflow field, wire messages |
| `src/tui/command.rs` | Add ExportYaml command |
| `src/tui/app.rs` | Handle export, sync DAG |

---

## Part 5: Test Plan

### Unit Tests (per phase)

| Phase | Test File | Tests |
|-------|-----------|-------|
| 1 | `src/tui/views/chat.rs` | 4 tests |
| 2 | `src/runtime/chat_workflow.rs` | 4 tests |
| 3 | `src/ast/memory.rs`, etc. | 6 tests |
| 4 | `src/runtime/memory_loader.rs` | 5 tests |
| 5 | `src/runtime/agent_resolver.rs` | 4 tests |

### Integration Tests

| Test | Description |
|------|-------------|
| `tests/wiring_checkpoint_10.rs` | ChatView ↔ ChatWorkflow wiring |
| `tests/schema_v06_test.rs` | Parse @0.6 workflows |
| `tests/memory_integration_test.rs` | Memory loading end-to-end |
| `tests/chat_export_test.rs` | Chat → YAML export round-trip |

### Manual Validation

1. `nika chat` → type messages → `/export yaml` → file created
2. Open exported file in Studio → edit → `nika run` → executes
3. DAG panel updates in real-time as messages are sent

---

## Part 6: Execution Order

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  EXECUTION ORDER                                                              ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Phase 1: Wire ChatWorkflow to ChatView ................ [2-3h] [P0] [DAY 1] ║
║    └── Unlocks: DAG panel shows real-time workflow                            ║
║                                                                               ║
║  Phase 2: Add to_yaml() Export ......................... [2-3h] [P0] [DAY 1] ║
║    └── Unlocks: /export yaml command                                          ║
║                                                                               ║
║  Phase 3: Schema @0.6 AST Types ........................ [3-4h] [P1] [DAY 2] ║
║    └── Unlocks: Parse memory/agents/skills in YAML                            ║
║                                                                               ║
║  Phase 4: Memory Loader ................................ [2-3h] [P2] [DAY 2] ║
║    └── Unlocks: {{memory.files.x}} bindings                                   ║
║                                                                               ║
║  Phase 5: Agent & Skill Resolution ..................... [2-3h] [P2] [DAY 3] ║
║    └── Unlocks: agent: { use: researcher, skill: tdd }                        ║
║                                                                               ║
║  Total: ~12-16 hours over 3 days                                              ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Part 7: Success Criteria

- [ ] `nika chat` messages create nodes in ChatWorkflow.dag
- [ ] @N mentions create edges between nodes
- [ ] `/export yaml` outputs valid `nika/workflow@0.6` file
- [ ] Exported YAML loads in Studio without errors
- [ ] Exported YAML runs with `nika run`
- [ ] DAG panel updates in real-time from workflow.dag
- [ ] `memory:` block files are loaded at workflow start
- [ ] `{{memory.files.x}}` bindings resolve correctly
- [ ] `agents:` definitions are parsed and resolved
- [ ] `agent: { use: name }` references defined agent
- [ ] `skills:` are loaded and appended to system prompts
- [ ] All tests pass (target: 3,100+ tests)
- [ ] Zero clippy warnings

---

## References

- Gap Analysis: `docs/plans/2026-02-26-chat-yaml-gap-analysis.md`
- Thread Safety: `docs/plans/2026-02-24-thread-safety-architecture.md`
- v0.6 Example: `examples/proposed-v06-full-example.nika.yaml`
- ChatWorkflow: `src/runtime/chat_workflow.rs`
- Mention System: `src/binding/mention.rs`
