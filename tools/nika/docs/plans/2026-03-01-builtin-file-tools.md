# Plan: Builtin File Tools (nika:read, nika:write, etc.)

**Date:** 2026-03-01
**Version:** v0.15.1
**Status:** Draft

## Contexte

Les outils filesystem existent déjà dans `src/tools/` :
- `ReadTool` - Lire fichiers avec numéros de lignes
- `WriteTool` - Créer/écraser fichiers
- `EditTool` - Modifier fichiers existants (old_string → new_string)
- `GlobTool` - Chercher fichiers par pattern
- `GrepTool` - Chercher contenu avec regex

**Problème:** Ces outils ne sont PAS dans le `BuiltinToolRouter`. Les agents ne peuvent pas les utiliser comme `nika:write`, `nika:read`, etc.

## Objectif

Ajouter 5 nouveaux builtin tools au router :

| Tool | Description | Paramètres |
|------|-------------|------------|
| `nika:read` | Lire fichier | `file_path`, `offset?`, `limit?` |
| `nika:write` | Créer/écraser fichier | `file_path`, `content` |
| `nika:edit` | Modifier fichier | `file_path`, `old_string`, `new_string`, `replace_all?` |
| `nika:glob` | Chercher fichiers | `pattern`, `path?` |
| `nika:grep` | Chercher contenu | `pattern`, `path?`, `type?`, `output_mode?` |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     BuiltinToolRouter                           │
├─────────────────────────────────────────────────────────────────┤
│  Existants (6)           │  Nouveaux (5)                        │
│  ────────────────────────│──────────────────────────────────────│
│  nika:sleep              │  nika:read   ← ReadTool              │
│  nika:log                │  nika:write  ← WriteTool             │
│  nika:emit               │  nika:edit   ← EditTool              │
│  nika:assert             │  nika:glob   ← GlobTool              │
│  nika:prompt             │  nika:grep   ← GrepTool              │
│  nika:run                │                                      │
└─────────────────────────────────────────────────────────────────┘
```

## Tâches

### Phase 1: Adapter les FileTools au trait BuiltinTool (2 tâches)

1. **Créer `runtime/builtin/file_adapter.rs`**
   - Wrapper qui adapte `FileTool` → `BuiltinTool`
   - Gestion du `ToolContext` avec working directory
   - Permission mode: `YoloMode` par défaut pour workflows

2. **Tests unitaires pour l'adapter**
   - Test read/write/edit/glob/grep via adapter
   - Test validation des chemins (security boundary)

### Phase 2: Intégrer au BuiltinToolRouter (2 tâches)

3. **Modifier `runtime/builtin/router.rs`**
   - Ajouter `ToolContext` au router
   - Enregistrer les 5 file tools
   - Total: 11 builtin tools

4. **Tests d'intégration router**
   - `BuiltinToolRouter::is_builtin("nika:write")` → true
   - Dispatch vers les bons tools

### Phase 3: Documentation et exemples (2 tâches)

5. **Mettre à jour la documentation**
   - CLAUDE.md: section builtin tools
   - Schema JSON: documenter les tools

6. **Créer exemples workflows**
   - `examples/builtin-file-tools.nika.yaml`
   - Démontrer read/write/edit/glob/grep

## Fichiers à modifier

| Fichier | Action |
|---------|--------|
| `src/runtime/builtin/mod.rs` | Export file_adapter |
| `src/runtime/builtin/file_adapter.rs` | **NOUVEAU** - Adapter FileTool→BuiltinTool |
| `src/runtime/builtin/router.rs` | Ajouter ToolContext + 5 file tools |
| `tools/nika/CLAUDE.md` | Documenter 11 builtin tools |
| `examples/builtin-file-tools.nika.yaml` | **NOUVEAU** - Exemple |

## Estimation

- **6 tâches**
- **~200 lignes de code** (adapter + router changes)
- **~50 lignes de tests**
- **1-2 heures**

## Risques

1. **Security boundary** - Les file tools valident déjà les chemins dans `ToolContext`
2. **Permission mode** - Utiliser `YoloMode` pour workflows headless
3. **Conflits de noms** - Pas de conflit car préfixe `nika:`

## Validation

- [ ] `cargo test` - Tous les tests passent
- [ ] `cargo clippy` - Zero warnings
- [ ] Exemple workflow fonctionne
- [ ] Agents peuvent utiliser `nika:write` etc.
