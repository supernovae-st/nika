# Phase 0: Stabilize — Agent Launch Prompt

**Copy-paste this prompt to start a new Claude Code session.**

---

## Prompt

```
Tu es un développeur Rust senior qui travaille sur Nika, un moteur de workflows YAML pour l'IA.
Tu vas implémenter Phase 0: Stabilize (v0.50) — réparer ce qui est cassé, documenter ce qui existe.

## Context

- Nika v0.49.3, schema @0.12, 8457 tests, 10 workspace crates
- Le projet est dans /Users/thibaut/dev/supernovae/nika/tools/nika (workspace Cargo root: /Users/thibaut/dev/supernovae/nika/tools)
- Lis d'abord ces fichiers pour comprendre le projet:
  - /Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md (structure codebase + conventions)
  - /Users/thibaut/dev/supernovae/nika/CLAUDE.md (overview projet)
  - /Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-28-v1-master-plan.md (master plan)
  - /Users/thibaut/dev/supernovae/nika/docs/plans/2026-03-28-phase0-stabilize.md (plan détaillé Phase 0)

## Méthodologie STRICTE

1. TOUJOURS lire le fichier AVANT de le modifier
2. UN fix = UN commit (type(scope): description + co-authors)
3. `cargo test --workspace --lib` après CHAQUE commit (JAMAIS `cargo test` sans --lib → keychain popup)
4. `cargo clippy --workspace -- -D warnings` = zero warnings
5. Ne PAS toucher aux fichiers en dehors du scope du task en cours
6. Si un test casse, le fixer AVANT de continuer

## Co-author lines (OBLIGATOIRE sur chaque commit)

```
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

## Commit format

```
type(scope): description
```

Types: feat, fix, refactor, docs, test, chore
Scopes: ast, runtime, cli, tui, mcp, provider, dag, event, lsp

## Les 6 tâches dans l'ordre

### Tâche 1: Fix error code table (5 min)
- Fichier: /Users/thibaut/dev/supernovae/nika/tools/nika/CLAUDE.md
- Ligne ~97: remplacer `| 160-164 | Policy/Boot errors |`
- Par: `| 160-164 | Parse errors (Phase 1 parser, nika-core) |` et ajouter `| 165-169 | Policy/Boot/Startup errors |`
- Vérifier aussi /Users/thibaut/dev/supernovae/dx/.claude/rules/nika.md pour le même fix
- Commit: `docs(cli): fix error code table — NIKA-160 is parse, not policy`

### Tâche 2: Wire `preset:` field on tasks (PRINCIPALE — 2-3h)
- Le bloc `agents:` EXISTE déjà dans l'AST (nika-core/src/ast/agent_def.rs → AgentDef enum)
- Les agents sont résolus dans runtime/resolver.rs → ResolvedAgent
- MAIS les tasks `infer:`, `fetch:`, `exec:` ne peuvent pas dire `preset: think` pour hériter du provider/model/temperature/system

Étapes:
a) Ajouter `preset: Option<String>` au struct Task dans nika-engine/src/ast/workflow.rs
b) Ajouter "preset" à KNOWN_TASK_KEYS dans nika-core/src/ast/raw/parser.rs (~line 1683)
c) Parser le field `preset:` dans parse_task()
d) Propager via l'analyzer et lower.rs
e) Dans runtime/executor/infer.rs (run_infer): AVANT la résolution provider/model, checker task.preset → résoudre le ResolvedAgent → appliquer provider/model/temperature/system comme fallback (task-level > preset > workflow default)
f) Même chose pour runtime/executor/agent.rs (run_agent)
g) Ajouter resolved_agents: Arc<...> au TaskExecutor pour qu'il puisse résoudre les presets
h) Écrire 10 tests:
   - preset résout provider+model
   - task override gagne sur preset
   - preset inconnu → erreur claire
   - pas de preset → comportement actuel (backward compat)
   - preset sur agent: verb fonctionne
i) Commit: `feat(runtime): add preset: field for agent-based model routing`

### Tâche 3: Quick wins from handoff (30 min)
- Jobs exit code bug: tools/nika-cli/src/jobs.rs ~line 79 → retourner erreur quand daemon not running
  Commit: `fix(cli): jobs command returns error when daemon not running`
- Dry-run cost in summary: tools/nika/src/main.rs ~line 2785
  Commit: `feat(cli): show estimated cost in dry-run summary`

### Tâche 4: VS Code extension version bump (15 min)
- Fichier: editors/vscode/package.json → bump version à "0.50.0"
- Commit: `chore(lsp): bump VS Code extension to v0.50.0`
- NOTE: ne PAS essayer de publier — juste bumper la version dans le fichier

### Tâche 5: Documenter agents: + preset: (1h)
- Mettre à jour la règle Nika:
  - /Users/thibaut/dev/supernovae/nika/.windsurf/rules/nika.md (si existe)
  - /Users/thibaut/dev/supernovae/dx/.claude/rules/nika.md
  - Ajouter section "## Agent Presets" avec exemples YAML
- Mettre à jour llms-syntax.txt si nécessaire
- Créer examples/agents-preset.nika.yaml avec provider: mock pour testabilité
- Commit: `docs: document agents: block and preset: field with examples`

### Tâche 6: Vérification finale
Exécuter dans l'ordre:
```bash
cd /Users/thibaut/dev/supernovae/nika/tools
cargo check --workspace
cargo clippy --workspace -- -D warnings
cargo test --workspace --lib
```
Les 3 doivent passer avec zero erreurs/warnings.

Lister les commits créés et le nombre de tests ajoutés.

## CE QU'IL NE FAUT PAS FAIRE
- Ne PAS bumper le schema (@0.12 reste @0.12)
- Ne PAS modifier le DAG (il reste immutable)
- Ne PAS ajouter de features non listées ci-dessus
- Ne PAS utiliser `cargo test` sans `--lib` (déclenche des popups keychain macOS)
- Ne PAS push sans demander (on push à la fin de la session)
- Ne PAS créer de fichiers .md de documentation sauf ceux listés
- Ne PAS refactorer du code qui n'est pas dans le scope
```

---

## Notes pour Thibaut

### Avant de lancer

1. Vérifie que tu es sur `main` et que le working tree est clean:
   ```bash
   cd /Users/thibaut/dev/supernovae/nika && git status
   ```

2. Crée une branche:
   ```bash
   git checkout -b feat/phase0-stabilize
   ```

3. Lance l'agent avec le prompt ci-dessus

### Après la session

1. Review les commits:
   ```bash
   git log --oneline main..HEAD
   ```

2. Vérifie les tests:
   ```bash
   cd tools && cargo test --workspace --lib 2>&1 | tail -5
   ```

3. Si tout est bon, push:
   ```bash
   git push -u origin feat/phase0-stabilize
   ```

### Durée estimée

- Tâche 1: 5 min
- Tâche 2: 2-3h (la plus grosse)
- Tâche 3: 30 min
- Tâche 4: 15 min
- Tâche 5: 1h
- Tâche 6: 15 min
- **Total: ~4-5h**

### Critères de succès

- [ ] `cargo check --workspace` = zero errors
- [ ] `cargo clippy --workspace -- -D warnings` = zero warnings
- [ ] `cargo test --workspace --lib` = 8470+ tests (au moins 10 nouveaux)
- [ ] `preset: think` fonctionne sur un task `infer:`
- [ ] Error code table corrigée
- [ ] Examples YAML créé et validable avec `nika check`
- [ ] 4-6 commits propres avec co-authors
