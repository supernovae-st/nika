# Plan d'Exécution v0.8.0 - Détaillé

**Date:** 2026-02-23
**Status:** En cours d'exécution

---

## Résumé des Problèmes à Résoudre

| # | Problème | Fichier(s) | Effort |
|---|----------|------------|--------|
| 1 | Monitor View non unifié | `views/monitor.rs` (nouveau) | 2h |
| 2 | ToolChoice non supporté | `ast/agent.rs`, `rig_agent_loop.rs` | 1h |
| 3 | Temperature non wirée | `ast/agent.rs`, `rig_agent_loop.rs` | 30min |
| 4 | Token tracking = 0 avec tools | Limitation rig-core | Documenté |

---

## Phase 1: Créer Monitor View Unifié

### Objectif
Créer `views/monitor.rs` qui implémente le trait `View` et orchestre les 4 panels existants.

### Fichiers à Modifier
- `src/tui/views/mod.rs` - Ajouter export `MonitorView`
- `src/tui/views/monitor.rs` - **NOUVEAU**

### Design
```rust
pub struct MonitorView {
    progress_panel: ProgressPanel,
    graph_panel: GraphPanel,
    context_panel: ContextPanel,
    reasoning_panel: ReasoningPanel,
    focused_panel: MonitorPanel,
}

enum MonitorPanel {
    Progress,  // [1]
    Graph,     // [2]
    Context,   // [3]
    Reasoning, // [4]
}
```

### Tests à Ajouter
- `test_monitor_view_creation()`
- `test_monitor_view_panel_focus()`
- `test_monitor_view_render()`

---

## Phase 2: Ajouter ToolChoice à AgentParams

### Objectif
Permettre de contrôler quand l'agent utilise les tools.

### Fichiers à Modifier
- `src/ast/agent.rs` - Ajouter `tool_choice: Option<ToolChoice>`
- `src/runtime/rig_agent_loop.rs` - Wire `.tool_choice()` sur AgentBuilder

### YAML Syntax
```yaml
agent:
  prompt: "..."
  tool_choice: auto  # auto | required | none
```

### Enum Design
```rust
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    #[default]
    Auto,      // LLM decides when to use tools
    Required,  // Must use at least one tool
    None,      // Never use tools
}
```

### Tests à Ajouter
- `test_parse_tool_choice_auto()`
- `test_parse_tool_choice_required()`
- `test_parse_tool_choice_none()`
- `test_tool_choice_default()`

---

## Phase 3: Wirer Temperature

### Objectif
Permettre de contrôler la température du modèle.

### Fichiers à Modifier
- `src/ast/agent.rs` - Ajouter `temperature: Option<f32>`
- `src/runtime/rig_agent_loop.rs` - Wire `.temperature()` sur le model request

### YAML Syntax
```yaml
agent:
  prompt: "..."
  temperature: 0.7  # 0.0 à 2.0
```

### Validation
- Doit être entre 0.0 et 2.0
- Default: None (utilise le default du provider)

### Tests à Ajouter
- `test_parse_temperature()`
- `test_temperature_validation_range()`
- `test_temperature_default()`

---

## Phase 4: Documentation Token Tracking

### Objectif
Documenter clairement la limitation du token tracking avec tools.

### État Actuel
```
+----------------+------------------+------------------+
| Scénario       | Méthode          | Token Tracking   |
+----------------+------------------+------------------+
| Sans tools     | model.stream()   | ✅ Complet       |
| Avec tools     | agent.prompt()   | ⚠️ Retourne 0   |
| Extended think | model.stream()   | ✅ Complet       |
+----------------+------------------+------------------+
```

### Cause
- `rig-core` ne supporte pas `agent.stream_prompt()` pour l'instant
- Quand tools sont présents, on utilise `agent.prompt()` qui ne retourne pas les tokens

### Mitigation (v0.8.0)
- Documenter clairement dans CLAUDE.md
- Ajouter commentaire dans le code
- Créer issue upstream sur rig-core

---

## Ordre d'Exécution

1. ✅ Créer ce plan
2. ⏳ Phase 2: ToolChoice (plus simple, bon warmup)
3. ⏳ Phase 3: Temperature
4. ⏳ Phase 1: Monitor View (plus complexe)
5. ⏳ Phase 4: Documentation
6. ⏳ Run tests finaux

---

## Critères de Succès

- [ ] `cargo test` passe (1,850+ tests)
- [ ] `cargo clippy -- -D warnings` = 0 warnings
- [ ] ToolChoice parsé et wirée
- [ ] Temperature parsée et wirée
- [ ] MonitorView implémente View trait
- [ ] Documentation mise à jour
