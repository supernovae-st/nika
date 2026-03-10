# Migration Plan: `memory:` → `context:`

**Version:** v0.14.0 (Schema v0.7)
**Date:** 2026-02-27
**Status:** Approved (Part of v0.14 Complete Plan)

> **Note:** This document is now part of the comprehensive v0.14 plan.
> See [2026-02-27-v014-complete-plan.md](./2026-02-27-v014-complete-plan.md) for full scope including:
> - Workflow Composition (`include:` + `invoke_workflow:`)
> - Jobs Daemon (cron/webhook/watch/interval)
> - CLI DX (global flags, completion, config)
> - Enhanced Doctor command

---

## Objectif

Renommer `memory:` en `context:` dans le schéma Nika pour:
1. Aligner avec MCP (Model Context Protocol)
2. Cohérence avec `.nika/context/` directory
3. Sémantique correcte (ce sont des fichiers de contexte, pas de mémoire persistante)
4. Réserver `memory:` pour la vraie persistance future (vector stores, cross-workflow)

---

## Inventaire des Fichiers Impactés

### Tier 1: Core AST (Breaking Changes)

| Fichier | Changements |
|---------|-------------|
| `src/ast/memory.rs` | Renommer → `context.rs`, `MemoryConfig` → `ContextConfig` |
| `src/ast/mod.rs` | `pub use context::ContextConfig;` |
| `src/ast/workflow.rs` | `memory: Option<ContextConfig>` → `context: Option<ContextConfig>` |

### Tier 2: Runtime

| Fichier | Changements |
|---------|-------------|
| `src/runtime/memory_loader.rs` | Renommer → `context_loader.rs`, `LoadedMemory` → `LoadedContext`, `load_memory()` → `load_context()` |
| `src/runtime/mod.rs` | `pub use context_loader::{load_context, LoadedContext};` |
| `src/runtime/runner.rs` | Appels à `load_memory()` → `load_context()` |
| `src/runtime/boot.rs` | Références à `memory.yaml` et logique de chargement |
| `src/runtime/resolver.rs` | Références à `MemoryLoadError` |

### Tier 3: Bindings & Templates

| Fichier | Changements |
|---------|-------------|
| `src/binding/template.rs` | `{{memory.files.*}}` → `{{context.files.*}}`, regex patterns |
| `src/store/datastore.rs` | `memory` field → `context`, `set_memory()` → `set_context()`, `resolve_memory_path()` → `resolve_context_path()`, `get_memory_file()` → `get_context_file()` |

### Tier 4: Errors

| Fichier | Changements |
|---------|-------------|
| `src/error.rs` | `MemoryLoadError` → `ContextLoadError`, code `NIKA-250` reste |

### Tier 5: Schema

| Fichier | Changements |
|---------|-------------|
| `schemas/nika-workflow.schema.json` | `"memory"` → `"context"`, `MemoryConfig` → `ContextConfig` |

### Tier 6: TUI

| Fichier | Changements |
|---------|-------------|
| `src/tui/views/chat.rs` | Références à `memory.json` |
| `src/tui/widgets/mission_control.rs` | `Session` enum comment |

### Tier 7: CLI

| Fichier | Changements |
|---------|-------------|
| `src/main.rs` | `memory.yaml` → `context.yaml` dans `nika init` |

### Tier 8: Examples & Docs

| Fichier | Changements |
|---------|-------------|
| `examples/drafts/*.nika.yaml` | `memory:` → `context:` |
| `docs/plans/*.md` | Documentation updates |
| `CLAUDE.md` | Schema documentation |
| `CHANGELOG.md` | v0.14 entry |

---

## Plan d'Exécution Détaillé

### Phase 1: AST Core (Breaking)

```rust
// AVANT: src/ast/memory.rs
pub struct MemoryConfig {
    pub files: FxHashMap<String, String>,
    pub session: Option<String>,
}

// APRÈS: src/ast/context.rs
pub struct ContextConfig {
    pub files: FxHashMap<String, String>,
    pub session: Option<String>,
}
```

**Fichiers:**
1. `mv src/ast/memory.rs src/ast/context.rs`
2. Edit `src/ast/context.rs`: `MemoryConfig` → `ContextConfig`
3. Edit `src/ast/mod.rs`:
   - `pub mod context;` (was `memory`)
   - `pub use context::ContextConfig;`
4. Edit `src/ast/workflow.rs`:
   - `pub context: Option<super::context::ContextConfig>,`
   - Update tests

### Phase 2: Runtime

```rust
// AVANT: src/runtime/memory_loader.rs
pub struct LoadedMemory { ... }
pub async fn load_memory(config: &MemoryConfig, ...) -> Result<LoadedMemory, NikaError>

// APRÈS: src/runtime/context_loader.rs
pub struct LoadedContext { ... }
pub async fn load_context(config: &ContextConfig, ...) -> Result<LoadedContext, NikaError>
```

**Fichiers:**
1. `mv src/runtime/memory_loader.rs src/runtime/context_loader.rs`
2. Edit `src/runtime/context_loader.rs`:
   - `LoadedMemory` → `LoadedContext`
   - `load_memory` → `load_context`
   - Update doc comments
3. Edit `src/runtime/mod.rs`:
   - `pub mod context_loader;`
   - `pub use context_loader::{load_context, LoadedContext};`
4. Edit `src/runtime/runner.rs`:
   - `use crate::runtime::context_loader::*;`
   - Update calls

### Phase 3: Error Codes

```rust
// AVANT
#[error("[NIKA-250] Failed to load memory file '{alias}'")]
MemoryLoadError { alias: String, path: String, reason: String }

// APRÈS
#[error("[NIKA-250] Failed to load context file '{alias}'")]
ContextLoadError { alias: String, path: String, reason: String }
```

**Note:** Code `NIKA-250` reste inchangé pour backward compatibility des logs.

### Phase 4: Bindings & DataStore

```rust
// AVANT: src/binding/template.rs
static ref MEMORY_BINDING_RE: Regex = ...;  // {{memory.files.X}}

// APRÈS
static ref CONTEXT_BINDING_RE: Regex = ...;  // {{context.files.X}}
// + MEMORY_BINDING_RE pour backward compatibility
```

```rust
// AVANT: src/store/datastore.rs
pub fn set_memory(&self, memory: LoadedMemory)
pub fn get_memory_file(&self, alias: &str) -> Option<Value>
pub fn resolve_memory_path(&self, path: &str) -> Option<Value>

// APRÈS
pub fn set_context(&self, context: LoadedContext)
pub fn get_context_file(&self, alias: &str) -> Option<Value>
pub fn resolve_context_path(&self, path: &str) -> Option<Value>

// + Backward compat aliases
pub fn set_memory(&self, memory: LoadedContext) { self.set_context(memory) }
```

### Phase 5: JSON Schema

```json
// AVANT
"memory": {
  "$ref": "#/$defs/MemoryConfig"
}

// APRÈS
"context": {
  "$ref": "#/$defs/ContextConfig"
}

// $defs
"ContextConfig": {
  "type": "object",
  "properties": {
    "files": { ... },
    "session": { ... }
  }
}
```

### Phase 6: Schema Version Bump

```rust
// AVANT
pub const SCHEMA_V06: &str = "nika/workflow@0.6";

// APRÈS
pub const SCHEMA_V07: &str = "nika/workflow@0.7";
```

### Phase 7: Backward Compatibility

Pour éviter de casser les workflows existants:

```rust
// Dans workflow.rs - Support les deux syntaxes
#[derive(Debug, Deserialize)]
struct WorkflowRaw {
    // New name (preferred)
    #[serde(default)]
    pub context: Option<ContextConfig>,

    // Old name (deprecated, alias)
    #[serde(default)]
    pub memory: Option<ContextConfig>,
}

impl Workflow {
    pub fn from_raw(raw: WorkflowRaw) -> Self {
        // Prefer context:, fallback to memory:
        let context = raw.context.or(raw.memory);
        // ...
    }
}
```

```rust
// Dans template.rs - Support les deux patterns
// {{context.files.X}} (new)
// {{memory.files.X}} (deprecated, still works)
```

### Phase 8: CLI `nika init`

```rust
// AVANT
let memory_config_path = nika_dir.join("memory.yaml");

// APRÈS
let context_config_path = nika_dir.join("context.yaml");
```

**Structure `.nika/` mise à jour:**
```
.nika/
├── config.toml
├── context.yaml      # Was: memory.yaml
├── user.yaml
├── policies.yaml
├── agents/
├── skills/
├── context/          # Unchanged
└── ...
```

### Phase 9: Documentation

| Document | Action |
|----------|--------|
| `CLAUDE.md` | Update schema v0.7 section |
| `tools/nika/CLAUDE.md` | Update examples |
| `CHANGELOG.md` | Add v0.14 entry |
| `docs/plans/*` | Update references |
| ADR files | Update if needed |

---

## Tests Impactés

### Tests à mettre à jour:

```rust
// src/ast/context.rs (was memory.rs)
test_context_config_default
test_context_config_deserialize_empty
test_context_config_deserialize_files
test_context_config_deserialize_session
test_context_config_deserialize_full
test_context_config_glob_pattern

// src/ast/workflow.rs
test_workflow_parse_v07_with_context  // was v06_with_memory

// src/runtime/context_loader.rs
test_loaded_context_default
test_loaded_context_get_file
test_load_context_full
test_load_context_with_session
test_load_context_missing_session_ok
test_load_context_missing_file_error

// src/store/datastore.rs
test_datastore_context_files
test_datastore_context_session
test_resolve_context_path_files
test_resolve_context_path_session

// src/binding/template.rs
test_template_context_files_binding
test_template_context_session_binding
test_template_mixed_use_and_context
test_template_context_missing_file
```

---

## Migration des Workflows Existants

### Script de migration (optionnel)

```bash
#!/bin/bash
# migrate-memory-to-context.sh

# Find all .nika.yaml files and replace memory: with context:
find . -name "*.nika.yaml" -exec sed -i '' 's/^memory:/context:/g' {} \;
find . -name "*.nika.yaml" -exec sed -i '' 's/{{memory\./{{context./g' {} \;

echo "Migration complete. Please review changes."
```

### Exemple workflow migré

```yaml
# AVANT (v0.6)
schema: nika/workflow@0.6
memory:
  files:
    brand: ./context/brand.md
  session: .nika/sessions/prev.json
tasks:
  - id: gen
    infer: "Using {{memory.files.brand}}"

# APRÈS (v0.7)
schema: nika/workflow@0.7
context:
  files:
    brand: ./context/brand.md
  session: .nika/sessions/prev.json
tasks:
  - id: gen
    infer: "Using {{context.files.brand}}"
```

---

## Checklist de Validation

### Pre-merge:
- [ ] Tous les tests passent (2,997+)
- [ ] Zero clippy warnings
- [ ] Backward compat: `memory:` toujours parsé
- [ ] Backward compat: `{{memory.*}}` toujours résolu
- [ ] JSON Schema valide
- [ ] Documentation à jour

### Post-merge:
- [ ] Exemples workflows fonctionnent
- [ ] `nika init` crée `context.yaml`
- [ ] TUI affiche correctement
- [ ] CHANGELOG mis à jour

---

## Risques et Mitigations

| Risque | Mitigation |
|--------|------------|
| Breaking change pour workflows existants | Backward compat via alias `memory:` |
| Confusion temporaire | Clear deprecation warnings |
| Tests cassés | Update all tests in same PR |
| Documentation désynchronisée | Update all docs in same PR |

---

## Timeline

| Phase | Durée estimée |
|-------|---------------|
| Phase 1-3 (Core) | 2h |
| Phase 4-5 (Bindings/Schema) | 1h |
| Phase 6-7 (Compat/CLI) | 1h |
| Phase 8-9 (Docs/Tests) | 2h |
| **Total** | **~6h** |

---

## Décision: Schema Version

**Option A:** v0.7 (minor bump)
- Backward compat via alias
- Pas de breaking pour l'utilisateur

**Option B:** v0.14.0 (release)
- Major feature rename
- Clean break

**Recommandation:** Option A (v0.7) car backward compat est maintenue.

---

## Références

- [Perplexity Research: Memory vs Context](./research-memory-vs-context.md)
- [MCP Specification](https://modelcontextprotocol.io/)
- [LangChain Context Patterns](https://docs.langchain.com/)
- [LlamaIndex Memory vs Context](https://docs.llamaindex.ai/)
