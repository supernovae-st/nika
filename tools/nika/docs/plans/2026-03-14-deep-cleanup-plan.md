# Deep Cleanup Plan — v0.28 Monolith Hygiene

**Date:** 2026-03-14
**Status:** Proposed
**Scope:** Nika v0.28 — Deep cleanup, zero behavioral changes

---

## Context

On vient de construire le pipeline AST moderne (Phases 1-4 du bridge pattern):

```
YAML → raw::parse() → analyzer::analyze() → lower() → Workflow → Runtime
```

Mais le codebase a accumulé du bruit pendant la construction:
- L'ancien chemin serde existe encore en parallele du nouveau
- Des dependances circulaires entre modules
- Du dead code, des commentaires mensongers, un feature flag mort

Ce plan nettoie tout ca. **Pas de workspace split** — le monolithe avec feature
gates fonctionne bien (incremental build 3s, clean build 1m44s). On nettoie
de l'interieur.

---

## Why NOT a Workspace Split

L'agent Rust Architect a analyse les metriques:

| Metrique | Actuel | Seuil de split | Verdict |
|----------|--------|----------------|---------|
| Incremental build | **3 secondes** | >15 secondes | Pas besoin |
| Clean build | **1m 44s** | >5 minutes | OK (deps dominent) |
| LOC total | **220k** | >400k | Marge de croissance |
| Consommateurs externes | **0** | >=1 | Aucun |
| Taille equipe | **1-2** | >=3 concurrents | Pas la |

Un split creerait:
- Overhead permanent de coordination entre crates
- Boilerplate Cargo.toml duplique
- Cross-crate refactoring plus difficile (1-line change → multi-crate migration)
- Dependency chain serielle (0 gain de compilation parallele)

**Triggers pour revisiter:** incremental >10s, LOC >400k, 3eme developpeur.

---

## The 7 Problems

### Problem 1: Dual Parsing Paths

On a DEUX chemins pour parser un Workflow YAML:

**Chemin moderne** (3 usages en production — `main.rs` seulement):
```rust
// src/ast/mod.rs:128-147
let workflow = parse_workflow(&yaml)?;
// Pipeline: YAML → raw::parse → analyzer::analyze → lower → Workflow
```

**Chemin legacy** (15 usages en production + 31 en tests):
```rust
// src/tui/app/commands.rs:25, src/tui/mod.rs:281, etc.
let workflow: Workflow = serde_yaml::from_str(&yaml)?;
// Pipeline: YAML → serde Deserialize → WorkflowRaw → Workflow
```

Le chemin legacy necessite:
- `WorkflowRaw` struct (`ast/workflow.rs:85-168`) — **existe uniquement pour serde**
- Custom `impl Deserialize for Workflow` (`ast/workflow.rs:170-192`)
- `#[derive(Deserialize)]` sur `Task`, `Flow`, `FlowEndpoint` (`ast/workflow.rs:69, 270, 525`)

**Pourquoi c'est un probleme:** On maintient deux chemins de parsing en parallele.
Le moderne a du span tracking, de la validation, du feature gating. Le legacy n'a rien
de tout ca. Quand la TUI parse un workflow, elle bypass toute la validation qu'on a
construite.

**Migration:**

| Fichier | Type | Action |
|---------|------|--------|
| `src/tui/app/commands.rs:25` | Production | `serde_yaml::from_str` → `parse_workflow` |
| `src/tui/app/routing.rs:611` | Production | `serde_yaml::from_str` → `parse_workflow` |
| `src/tui/standalone.rs:491` | Production | `serde_yaml::from_str` → `parse_workflow` |
| `src/tui/mod.rs:281` | Production | `serde_yaml::from_str` → `parse_workflow` |
| `src/tui/views/home.rs:604` | Production | `serde_yaml::from_str` → `parse_workflow` |
| `src/tui/views/studio.rs:2421` | Production | `serde_yaml::from_str` → `parse_workflow` |
| `src/ast/include_loader.rs:240` | Production | `serde_yaml::from_str` → `parse_workflow` |
| `src/runtime/resolver.rs:219` | Production | `serde_yaml::from_str` → `parse_workflow` |
| `src/runtime/builtin/run.rs:270` | Production | `serde_yaml::from_str` → `parse_workflow` |
| `src/main.rs:2829,3028,3079,5415,5464` | Production | `serde_yaml::from_str` → `parse_workflow` |
| `src/ast/workflow.rs:565-1571` | Tests (31) | `serde_yaml::from_str` → `parse_workflow` |

Apres migration:
- **Supprimer** `WorkflowRaw` struct (ast/workflow.rs:85-168)
- **Supprimer** custom `Deserialize` impl (ast/workflow.rs:170-192)
- **Supprimer** `#[derive(Deserialize)]` sur `Workflow` (ast/workflow.rs:69)
- **Garder** `#[derive(Serialize)]` sur `Workflow` (toujours utile pour export)
- **Garder** `#[derive(Deserialize)]` sur `Task`, `Flow` si d'autres parsers les utilisent

**Note:** `serde_yaml::from_str` reste pour le parsing YAML generique (McpConfig,
serde_json::Value, WiringSpec, UseEntry, etc.). C'est seulement le chemin
`serde_yaml::from_str::<Workflow>` qui disparait.

---

### Problem 2: Circular Dependencies

**Cycle 1: store ↔ runtime**
```
src/store/datastore.rs:19 → use crate::runtime::context_loader::LoadedContext
src/runtime/runner.rs:26   → use crate::store::{RunContext, TaskResult}
```

Le store (couche donnees) importe du runtime (couche execution). C'est inverti.

**Fix:** Deplacer `LoadedContext` de `runtime/context_loader.rs` vers `store/` ou
`binding/`. Le type est une structure de donnees, pas de la logique d'execution.

**Cycle 2: secrets → tui**
```
src/secrets/fallback.rs:18 → use crate::tui::widgets::provider_modal::NikaKeyring
```

Le module secrets importe un **widget TUI** pour acceder au keyring.

**Fix:** Deplacer `NikaKeyring` de `tui/widgets/provider_modal/` vers `secrets/`.
Le TUI importe ensuite de secrets.

---

### Problem 3: Dead Feature Flag

```toml
# Cargo.toml:31
docker = ["tui"]  # Docker build: TUI enabled, no keychain
```

Zero `#[cfg(feature = "docker")]` dans le code. Jamais utilise.

**Fix:** Supprimer la ligne.

---

### Problem 4: petgraph Internals in Public API

```rust
// src/lib.rs:162
pub use petgraph::stable_graph::{EdgeIndex as StableEdgeIndex, NodeIndex as StableNodeIndex};
```

On expose les types internes de petgraph dans l'API publique de Nika.
Personne en dehors de `dag/` ne les utilise directement.

**Fix:** Supprimer le re-export. Les types restent dans `dag/` via `use petgraph::...`.

---

### Problem 5: 49 `#[allow(dead_code)]` + Misleading Comments

- 49 `#[allow(dead_code)]` — le compilateur dit "c'est mort" et on dit "ignore"
- 50 commentaires "deprecated" sur du code actif
- 24 TODO/FIXME dont la plupart sont stale

**Fix:** Pour chaque `#[allow(dead_code)]`:
- Si le code est utilise en test → `#[cfg(test)]`
- Si le code est feature-gated → verifier le gate
- Si c'est vraiment mort → supprimer

Pour les commentaires "deprecated": supprimer le mot ou expliquer le vrai statut.
Pour les TODO: resoudre ou supprimer avec un commentaire explicatif.

---

### Problem 6: serde_yaml Alias Confusion

```rust
// src/lib.rs:49
pub use serde_saphyr as serde_yaml;
```

On a migre vers `serde_saphyr` mais on garde l'ancien nom partout.
Quand on lit le code, on pense utiliser le crate deprecated `serde_yaml`.

**Decision:** On GARDE l'alias. Raisons:
1. 315+ usages dans le codebase — renommer partout est du bruit de diff massif
2. L'alias est documente (lib.rs:44-47)
3. L'ecosysteme Rust utilise `serde_yaml::` comme convention
4. Le cout de confusion est faible vs le cout de migration

---

### Problem 7: Over-Exposed Public API

`lib.rs` re-exporte ~40 types. `provider/rig.rs` a 53 items pub.
Nika est un binaire, pas une library.

**Fix:** Audit de `lib.rs`:
- Garder les re-exports utilises par les integration tests (`tests/`)
- `pub(crate)` pour le reste
- Supprimer les re-exports de petgraph (Problem 4)

---

## Execution Plan

### Commit 1: Migrate TUI Workflow parsing to parse_workflow()

Migrer les 6 fichiers TUI de `serde_yaml::from_str::<Workflow>` vers `parse_workflow()`.
Error handling change: `serde_yaml::Error` → `NikaError` (deja le type retourne).

**Files:** `tui/app/commands.rs`, `tui/app/routing.rs`, `tui/standalone.rs`,
`tui/mod.rs`, `tui/views/home.rs`, `tui/views/studio.rs`

### Commit 2: Migrate runtime + include_loader + main.rs to parse_workflow()

Migrer les 8 fichiers restants en production.

**Files:** `runtime/resolver.rs`, `runtime/builtin/run.rs`, `ast/include_loader.rs`,
`main.rs` (5 sites)

### Commit 3: Migrate ast/workflow.rs tests to parse_workflow()

Migrer les 31 tests qui utilisent encore `serde_yaml::from_str::<Workflow>`.
Certains tests parsent des `Task` ou `Flow` individuellement — ceux-la gardent serde.

### Commit 4: Remove legacy Workflow Deserialize

- Supprimer `WorkflowRaw` struct (ast/workflow.rs:85-168)
- Supprimer `impl Deserialize for Workflow` (ast/workflow.rs:170-192)
- Supprimer `#[derive(Deserialize)]` sur Workflow (ast/workflow.rs:69)
- Garder `#[derive(Serialize)]` pour l'export
- Garder Deserialize sur Task/Flow/FlowEndpoint (utilises ailleurs)

### Commit 5: Fix store ↔ runtime circular dependency

Deplacer `LoadedContext` vers `store/` ou `binding/types.rs`.

### Commit 6: Fix secrets → tui circular dependency

Deplacer `NikaKeyring` de `tui/widgets/provider_modal/` vers `secrets/keyring.rs`.

### Commit 7: Remove dead weight

- Supprimer `docker` feature flag (Cargo.toml:31)
- Supprimer petgraph re-exports (lib.rs:162)
- Audit des 49 `#[allow(dead_code)]` (supprimer ou justifier)

### Commit 8: Clean misleading comments

- Supprimer/corriger les 50 commentaires "deprecated" mensongers
- Resoudre ou supprimer les 24 TODO/FIXME stale

### Commit 9: Reduce lib.rs public API surface

- Audit des ~40 re-exports
- `pub(crate)` pour les details d'implementation
- Garder les exports necessaires pour `tests/`

---

## What Stays (serde reste)

serde et serde_saphyr **restent** dans le projet. Ils sont utilises pour:
- Parsing McpConfig (`core/mcp_config.rs`)
- Parsing serde_json::Value generique (`init/mod.rs`, `error.rs`, `studio.rs`)
- Parsing WiringSpec/UseEntry (`binding/entry.rs`)
- Serialization to_string (`registry/`, `sync/`, `ast/decompose.rs`)
- Parsing context loader content (`runtime/context_loader.rs`)
- Derive Deserialize sur Task, Flow, FlowEndpoint (utilises par d'autres parsers)

C'est seulement le chemin `serde_yaml::from_str::<Workflow>` qui est remplace par
`parse_workflow()` — notre pipeline moderne avec span tracking et validation.

---

## Perf Fixes (Separate PR)

Issues identifiees par l'agent Rust Perf, a traiter dans un PR separe:

| Priority | Fix | File | Impact |
|----------|-----|------|--------|
| HIGH | Feature-gate git2+openssl | Cargo.toml | -30-60s compile |
| HIGH | value_to_display() → Cow<str> | binding/template.rs:226 | Avoid String clone |
| MED | parse_template_expr() SmallVec | binding/template.rs:204 | Avoid Vec alloc |
| MED | Box large AnalyzedTaskAction variants | ast/analyzed/task.rs:28 | 200→80 bytes enum |
| MED | get_ready_tasks() pending set | runtime/runner.rs:246 | O(N)→O(1) |
| LOW | BindingPath.segments SmallVec | binding/types.rs:51 | Fewer allocations |
| LOW | lto = true for release | Cargo.toml | Smaller binary |

---

## Success Criteria

After this cleanup:

- [ ] `grep -rn 'serde_yaml::from_str.*Workflow' src/` returns 0 non-test results
- [ ] `grep -rn 'WorkflowRaw' src/` returns 0 results
- [ ] `grep -rn 'use crate::runtime' src/store/` returns 0 results
- [ ] `grep -rn 'use crate::tui' src/secrets/` returns 0 results
- [ ] `grep -rn 'docker' Cargo.toml` returns 0 results
- [ ] `grep -rn 'pub use petgraph' src/lib.rs` returns 0 results
- [ ] `cargo test` passes (6,157+ tests)
- [ ] `cargo clippy -- -D warnings` is clean
- [ ] Zero behavioral changes — same inputs, same outputs
