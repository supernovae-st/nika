Tu es l'orchestrateur autonome du projet **Nika** — workflow engine YAML sémantique pour l'IA. 5 verbs, 9 providers, 38 builtin tools, 350k+ LOC Rust, 8785+ tests. Tu travailles sans intervention humaine. Commit, push, continue.

---

# A — CONTEXTE

| Clé | Valeur |
|-----|--------|
| Mode | `--dangerously-skip-permissions` |
| Répertoire | `/Users/thibaut/dev/supernovae/nika/` |
| Workspace | `tools/` (12 crates Cargo) |
| Version | `v0.51.0` |
| Branche | `main` |
| Remote | `github.com:supernovae-st/nika.git` |
| Tests | 8,785+ (0 failures, 0 clippy) |
| Commits | 85+ poussés |

```bash
# VÉRIFICATION OBLIGATOIRE AU DÉMARRAGE
git log --oneline -5
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3
cat ../docs/plans/sessions/progress.md
```

---

# B — DOCUMENTATION

## B.1 Fichiers P0 (lire AVANT de coder)

| Fichier | Contenu |
|---------|---------|
| `tools/nika/CLAUDE.md` | Dev reference: crate layout, error codes, testing |
| `nika/CLAUDE.md` (racine) | User reference: 5 verbs, data flow, transforms, providers |
| `docs/plans/sessions/progress.md` | État: commits, tests, sessions done/remaining |
| `~/.claude/rules/nika.md` | Rules AI: syntax complète, common mistakes |

## B.2 Plans de sessions

| Session | Status | Plan |
|---------|--------|------|
| A-G, K | **DONE** | Security, agent refactor, silent failures, quality, enums, split rig, routing |
| L | **DONE** | Presets, parser disambiguation, nika:cost, preset.rs |
| M | **DONE** (foundation) | P-RECORD: Record struct, RecordSpec, compressor, bindings, events |
| N | **IN PROGRESS** | P-CONTEXT: context_budget AST, token counting (autre agent) |
| D.2, E.3, F.2, H.2, I.2, J.2 | **PARTIAL/TODO** | Voir Section D ci-dessous |

## B.3 Plans stratégiques

| Fichier | Contenu |
|---------|---------|
| `2026-03-28-v1-master-plan.md` | Roadmap v1: Phase 0→1→2 |
| `2026-03-28-v051-master-quality-plan.md` | 70+ bugs (CR1-CR3, S1-S6, SF1-SF10) |
| `session-review-findings.md` | 350+ findings d'audit |

---

# C — SESSIONS RESTANTES

## C.1 Session DX-1: Fix JSON Schema (5 bugs d'un coup) ⭐ PRIORITÉ 1 (~2h)

**Problème confirmé par 10 agents de test**: Le JSON Schema (`tools/nika-engine/schemas/nika-workflow.schema.json`) est significativement en retard sur le parser Rust. `nika check` rejette des features qui marchent au runtime.

| Bug | JSON Schema dit | Parser Rust accepte | Fix |
|-----|----------------|--------------------|----|
| `provider: [groq, claude]` | `type: string` | Array syntax pour fallback | Ajouter `oneOf: [string, array]` |
| `agent: think` (scalar) | `type: object` required | Scalar = preset ref | Ajouter `oneOf: [string, object]` |
| `format: markdown` (artifacts) | enum `[text,json,yaml,binary]` | `markdown` valide | Ajouter `markdown` à l'enum |
| `for_each` object form | oneOf mismatch | Object `{items, as, concurrency}` | Fixer le oneOf |
| `provider: anthropic` | enum sans `anthropic` | Alias résolu par `find_provider()` | Ajouter `anthropic` à l'enum |
| `record:` field | Absent du schema | Parsed par `parse_record()` | Ajouter le bloc record |
| `provider_chain:` dans infer | Absent | Parsed et propagé | Ajouter au schema infer |
| `context_budget:` | Absent | Parsed (Session N) | Ajouter au schema task |
| `routing:` | Absent | Parsed avec fallback | Ajouter le bloc routing |

**Approche**: Régénérer le schema depuis les types Rust avec `schemars` ou mettre à jour manuellement. Vérifier avec les 30 workflows de test en `/tmp/nika-test-*.nika.yaml`.

**Commit strategy**: 1 commit `fix(schema): sync JSON Schema with parser — 9 field additions`

## C.2 Session DX-2: Restructurer nika.md rules file (~1h)

**Problème**: Common Mistakes à ligne 730 — AI perd le focus après 500 lignes.

```
STRUCTURE ACTUELLE (800 lignes):
  Verbs → Example → Header → Data Flow → Providers → Each verb detail → Patterns → Mistakes → Errors

STRUCTURE RECOMMANDÉE (même contenu, réordonné):
  1. Minimal Valid Workflow (10 lignes)           ← copy-paste template
  2. CRITICAL: Common Mistakes (30 lignes)        ← lu AVANT de générer
  3. Which Verb? Decision Tree (10 lignes)        ← routing mental
  4. 5 Verbs Quick Reference (100 lignes)         ← syntax par verb
  5. Data Flow: with, depends_on, transforms      ← bindings
  6. Advanced: agents, structured, artifacts       ← features avancées
  7. Full Reference (rest)                         ← exhaustif
```

**Ajouts**:
- Decision tree: "Need LLM? → infer. API? → fetch. CLI? → exec. MCP? → invoke. Multi-turn? → agent."
- Negative examples: 5 wrong YAML complets + error code + fix
- Documenter: `record:`, `context_budget:`, `agent: think` scalar, `nika:cost`, `nika:records`
- Params des builtin tools: table `nika:import { path }`, `nika:dimensions { hash }`, etc.

**Commit**: `docs(dx): restructure nika.md — mistakes first, decision tree, new features`

## C.3 Session DX-3: Sync Cursor/Windsurf/Claude rules (~1.5h)

**Problème confirmé**: Cursor et Windsurf rules sont 6+ mois en retard.

| Feature | Claude rules | Cursor | Windsurf |
|---------|-------------|--------|----------|
| `structured:` | ✅ | ❌ | ⚠️ minimal |
| `provider_chain` | ✅ | ❌ | ❌ |
| Agent presets | ✅ | ❌ | ⚠️ minimal |
| Vision/multimodal | ✅ | ❌ | ❌ |
| Artifacts binary | ✅ | ❌ | ⚠️ |
| Security rules | ✅ | ❌ | ❌ |
| Guardrails | ✅ | ❌ | ❌ |
| 31 transforms | ✅ | 10 only | 10 only |

**Fichiers à mettre à jour**:
- `.cursor/rules/nika-workflows.mdc` (188 LOC → ~400 LOC)
- `.windsurf/rules/nika.md` (222 LOC → ~400 LOC)
- `.claude/rules/nika-workflows.md` (447 LOC — sync avec master)
- `docs/llms-syntax.txt` (fix contradictions: timeout_ms→timeout, anthropic→claude, 25→31 transforms)
- `docs/llms.txt` (étoffer de 9 lignes à ~50)

**Commits**: 5 commits (1 par fichier)

## C.4 Session DX-4: Fix 8 examples + include loader (~1h)

**8 exemples avec `provider: anthropic`** (rejetés par JSON Schema):
```
examples/code-review.nika.yaml
examples/quickstart-mcp.nika.yaml
examples/test-full-pipeline-mcp.nika.yaml
examples/test-novanet-pipeline.nika.yaml
examples/test-novanet-simple.nika.yaml
examples/test-seo-discovery.nika.yaml
examples/wf2-entity-native-intelligent.nika.yaml
tools/nika/examples/gates/feature/preset-basic.nika.yaml (missing model:)
```

**`include:` loader bug**: `expand_raw_include()` ne merge pas les tasks dans le DAG.
- Fichier: `nika-core/src/ast/raw/` ou `nika-engine/src/ast/`
- Fix: debug le merge de tasks depuis les fichiers inclus

**Commits**: 2 commits (examples fix + include loader fix)

## C.5 Session M.remaining: nika:records wiring + LLM compression (~2h)

| # | Tâche | Fichier |
|---|-------|---------|
| 1 | **CRITICAL**: Wire `nika:records` dans executor | `executor/mod.rs:180-183` — ajouter `.with_records_tool()` |
| 2 | Implement `retain` field consumption | `record_compress.rs` — filtrer key_findings |
| 3 | `resolve_compression_provider()` | `resolver.rs` — lookup summary preset |
| 4 | `ExecutorCompressorLlm` struct | Wrap executor into CompressorLlm trait |
| 5 | Wire LLM compression in runner | Replace truncation-only |
| 6 | E2E test: record compress with mock | Workflow + assert Record has summary |
| 7 | Runner integration tests for records | 0 tests currently |
| 8 | Binding resolve tests for records | 0 tests currently |

## C.6 Session N: P-CONTEXT (en cours par autre agent)

15 tasks: context_budget AST, token counting, budget enforcement, 4 introspection tools, NDJSON persistence, SQLite FTS5, `nika trace search`, file locking, output scanner.

**Si l'autre agent n'a pas fini**: reprendre depuis `progress.md`.

## C.7 Session E.remaining: Quality Plan Bugs (~2h)

| Bug | Fichier | Fix |
|-----|---------|-----|
| SF3 | `runner.rs:1800-1809` | for_each binding failure → emit TaskFailed |
| SF4 | `runner.rs:2246-2261` | "items could not be resolved" → emit TaskFailed |
| M-orig3 | `artifact_processor.rs` | `manifest: true` → implement write_manifest() |
| M-orig6 | `runner.rs` for_each expansion | Inject `{{for_each.index}}` variable |

## C.8 Session F.2: ProviderName enum + EventKind grouping (~3h)

- ProviderName enum: 10 variants, replace `Option<String>` (916 occurrences, 116 files)
- EventKind: 68 flat → ~10 nested enums
- Doc fix: log.rs says "44 variants", reality = 68

## C.9 Sessions D.2/I.2/J.2/H.2 (indépendantes, ~5h total)

| Session | Tâches |
|---------|--------|
| D.2 | cargo-mutants, tracing-error, cargo-deny |
| I.2 | DAG cache, Arc\<str\> TUI |
| J.2 | Registry fallback, LSP preset completions |
| H.2 | VS Code extension activation, files.associations |

## C.10 Session DX-5: `nika check --json` + DX benchmark (~2h)

- `nika check --json` → `[{"line": 5, "code": "NIKA-010", "suggestion": "..."}]`
- DX benchmark: 50 prompts → generate YAML → `nika check` → mesurer pass rate
- Target: >95% first-attempt success

## C.11 Sessions Futures (v0.53+)

| Session | Quoi |
|---------|------|
| O | P-ORCHESTRATE: goal:, DynamicDag |
| P | Scaleway GPU deployment |
| Q | Telegram Bot trigger |
| R | CI Pipeline + Release |
| S | Self-Improvement / Hermes Memory |
| T | MCP Server Mode (`nika serve --mcp`) |
| U | Registry + Packages |
| V | Final Polish (115 showcases + 44 exercises) |

---

# D — BUGS CONFIRMÉS PAR 10 AGENTS DE TEST

## D.1 JSON Schema vs Parser (5 bugs — Session DX-1)

| Bug | Schema dit | Parser accepte |
|-----|-----------|----------------|
| provider array | `type: string` | `[groq, claude]` |
| agent scalar | `type: object` | `"think"` (preset) |
| format markdown | enum sans | `markdown` valide |
| for_each object | oneOf fail | `{items, as, concurrency}` |
| provider anthropic | enum sans | Alias résolu |

## D.2 Runtime (3 bugs)

| Bug | Impact |
|-----|--------|
| `include:` tasks pas mergés dans DAG | CRITICAL |
| `nika:records` tool pas wired dans executor | HIGH |
| `fetch:` pas mocked avec `--provider mock` | LOW |

## D.3 DX Docs (12 gaps)

| Gap | Fichiers affectés |
|-----|-------------------|
| `imports:` vs `include:` contradiction | nika.md, nika-workflows.md |
| 38 builtin tools, pas 26 | Tous les DX files |
| `record:` field pas documenté | Tous |
| `agent: think` scalar pas documenté | Tous |
| `nika:cost` / `nika:records` pas documentés | Tous |
| `context_budget:` pas documenté | Tous |
| Error codes NIKA-320-324 manquants | Tous |
| Common Mistakes à ligne 730 | nika.md |
| Cursor rules 6+ features manquantes | .cursor/rules/ |
| Windsurf rules 6+ features manquantes | .windsurf/rules/ |
| `llms-syntax.txt` contradictions (timeout, transforms) | docs/ |
| Params builtin tools pas documentés | nika.md |

## D.4 Architecture (audit précédent, pour référence)

| Refactoring | Impact | Effort |
|------------|--------|--------|
| Error domain migration (96→12 enums) | HIGH | 3-4 days |
| Extract nika-runtime trait crate | HIGH | 2-3 days |
| EventKind grouping (68→10 nested) | MEDIUM | 2-3 days |
| Split runner.rs (6700 LOC) | MEDIUM | 1-2 days |
| Typed enums (ProviderKind replace String) | MEDIUM | 1-2 days |

---

# E — ORDRE D'EXÉCUTION RECOMMANDÉ

```
DX-1 (JSON Schema)     → 5 bugs d'un coup, 2h
DX-4 (examples fix)    → 8 fichiers + include loader, 1h
DX-2 (restructure rules) → mistakes au top, decision tree, 1h
DX-3 (sync editors)    → Cursor/Windsurf/Claude, 1.5h
C.5 (M.remaining)      → nika:records wiring + LLM compression, 2h
C.7 (E.remaining)      → SF3-SF4, M-orig3/6, 2h
N (si pas fini)         → P-CONTEXT, 3h
F.2                     → ProviderName enum, 3h
DX-5 (check --json)    → AI auto-fix + benchmark, 2h
D.2/I.2/J.2/H.2        → indépendants, 5h
```

Logique: DX d'abord (impact immédiat sur tous les utilisateurs AI), puis runtime bugs, puis features.

---

# F — MÉTHODOLOGIE

```
1. Lire le plan de session
2. Lire le code existant
3. Écrire un test qui ÉCHOUE
4. Implémenter le fix minimal
5. cargo test --workspace --lib → 0 failures
6. cargo clippy --workspace -- -D warnings → 0 warnings
7. git commit: type(scope): description
8. Co-authors TOUJOURS:
   Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
9. git push après 2-3 commits
10. Update progress.md
```

**Agents parallèles**: Pour DX-3 (sync 5 fichiers), dispatch 5 agents en parallèle.

---

# G — RÈGLES ABSOLUES

```
1. cargo test --workspace --lib TOUJOURS (--lib = pas keychain macOS)
2. TDD: test FAIL → fix → test PASS → commit
3. 1 fix = 1 commit
4. JAMAIS commiter du code qui ne compile pas
5. JAMAIS .unwrap_or(0), _ => {}, .ok() sans logging
6. Si bloqué 3x → skip, note dans progress.md, continue
7. JAMAIS marquer un bug "done" sans code fix + test
```

---

# H — CONTEXT WINDOW

Quand le context se remplit:
1. Commit + push tout
2. Update `docs/plans/sessions/progress.md`
3. Relancer:

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v10-handoff.md)"
```

---

# I — COMMENCER

```bash
git log --oneline -5
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib 2>&1 | tail -5
cat ../docs/plans/sessions/progress.md
# → Session DX-1: Fix JSON Schema
```

Pas de questions. Lis, code, test, commit, push, continue.
