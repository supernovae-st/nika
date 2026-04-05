# NIKA — Mega Prompt v12 Handoff

Tu es l'orchestrateur autonome du projet **Nika** -- un workflow engine YAML semantique pour l'IA (5 verbs, 9 providers, 30+ builtin tools, 353k+ LOC Rust). Tu travailles sans intervention humaine. Commit, push, continue.

---

# SECTION A -- CONTEXTE OPERATIONNEL

## A.1 Acces

| Cle | Valeur |
|-----|--------|
| Mode | `--dangerously-skip-permissions` |
| Repertoire | `/Users/thibaut/dev/supernovae/nika/` |
| Workspace Cargo | `tools/` (12 crates) |
| Binaire | `nika` |
| Version | `v0.51.0` |
| Branche | `main` |
| Remote | `github.com:supernovae-st/nika.git` |

## A.2 Verification initiale (OBLIGATOIRE)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3
cat ../docs/plans/sessions/progress.md | head -20
```

## A.3 Snapshot (2026-03-29)

```
Commits       : 112
Tests         : 8,854 (0 failures, 0 clippy warnings)
  nika-engine : 4,177
  nika-tui    : 2,153
  nika-core   : 838
  nika-cli    : 388
  nika-media  : 329
  nika-mcp    : 292
  nika-lsp-core: 230
  nika-daemon : 164
  nika-init   : 142
  nika-event  : 141
Builtin tools : 30 nika:*
EventKind     : 64 variants
LOC total     : 353,250+
```

---

# SECTION B -- CE QUI EST FAIT (NE PAS REFAIRE)

## B.1 Phase 1 Intelligence (80% complete)

| Feature | Status | Commits |
|---------|--------|---------|
| P-MODEL (presets, routing, nika:cost) | DONE | Sessions K+L (11 commits) |
| P-RECORD (compression, bindings, LLM wiring) | DONE | Session M (12 commits) |
| P-CONTEXT (context_budget, token counting) | DONE | Session N (4 commits) |
| P-INTROSPECT (4 introspection tools) | DONE | Session N (1 commit) |
| P-MEMORY-LOCAL (NDJSON, CLI search, scanner) | DONE | Session N (2 commits) |
| P-ORCHESTRATE | **NOT STARTED** | -- |

## B.2 Quality Sprint (this session)

| Commit | Description |
|--------|-------------|
| `b32b68d` | refactor(ast): ProviderName enum migration (12 files) |
| `4a7fbab` | fix(provider): canonicalize defaults to "anthropic" |
| `43c916e` | docs: quality sprint plan + CHANGELOG |
| `12ba270` | fix(security): allow multi-line shell commands from YAML blocks |
| `c556fba` | docs: progress + workflow testing results |

## B.3 Bug Verification Results

| Bug ID | Reported Issue | Actual Status |
|--------|---------------|---------------|
| NIKA-BUG-002 | provider: anthropic rejected | **NOT A BUG** -- schema accepts any string |
| NIKA-BUG-003 | for_each rejects templates | **NOT A BUG** -- parser tests prove it works |
| NIKA-BUG-006 | nika.md overwritten | **NOT A BUG** -- hash protection preserves customizations |
| NIKA-BUG-007 | for_each artifact overwrites | **NOT A BUG** -- path templates fully supported |
| NIKA-BUG-008 | French apostrophes | **NOT A BUG** -- escape_for_shell handles '\'' correctly |
| NIKA-BUG-009 | nika:write blocked | **BY DESIGN** -- security feature, not a bug |

---

# SECTION C -- TRAVAIL IMMEDIAT (cette semaine)

## C.1 Fix 9 course workflow design issues

**Probleme**: 9/15 course workflows fail at `nika run` because exec tasks output raw text but transforms (sort, keys, first, flatten) expect JSON.

**Fix**: Add `output: { format: json }` on source tasks that produce JSON output.

**Fichiers**:
```
tools/nika/examples/gates/course/03-array-operations.nika.yaml
tools/nika/examples/gates/course/04-object-operations.nika.yaml
tools/nika/examples/gates/course/05-array-slicing.nika.yaml
tools/nika/examples/gates/course/06-flatten-compact.nika.yaml
tools/nika/examples/gates/course/07-reverse-unique.nika.yaml
tools/nika/examples/gates/course/09-json-roundtrip.nika.yaml
tools/nika/examples/gates/course/11-default-values.nika.yaml
tools/nika/examples/gates/course/12-type-inspection.nika.yaml
```

**Pattern**:
```yaml
# AVANT (fail):
- id: source
  exec: 'echo ''["a","b","c"]'''
- id: sorted
  with: { items: $source | sort }  # NIKA-152: sort expects array, got string

# APRES (pass):
- id: source
  exec: 'echo ''["a","b","c"]'''
  output: { format: json }         # Parse stdout as JSON
- id: sorted
  with: { items: $source | sort }  # OK: source is now a JSON array
```

**Verification**: `nika run <file> --no-live` pour chaque fichier, 0 errors.

**Commit**: `fix(course): add output format json to 8 course workflows`

## C.2 Fix course 02 shell blocklist false positive

**Probleme**: `$(date +%Y-%m-%d)` in resolved template content triggers NIKA-053 `$(` check.

**Fichier**: `nika-engine/src/runtime/security.rs:143-149`

**Approach**: The `$(` check runs on the RESOLVED command (after template substitution). Literal `$(` in data (from task output) is NOT command substitution. Two options:
- **Option A**: Check raw command template BEFORE resolution (pre-resolution validation)
- **Option B**: Context-aware `$(` detection (skip if inside quotes)

**Option A is simpler** -- validate the user's YAML template, not the resolved data.

**TDD**:
1. Write test: shell command with `$(date)` in resolved binding should pass
2. Write test: shell command with literal `$(rm -rf /)` in user template should fail
3. Fix: validate pre-resolution template, not post-resolution string

## C.3 Fix $env SECRET blocking (NIKA-BUG-001)

**Fichier**: `nika-engine/src/binding/resolve.rs:695-714`

**Probleme**: $env.ELEVENLABS_API_KEY silently returns null. Any var with KEY/SECRET/TOKEN in name is blocked.

**Fix**: Add `env_allow:` workflow header field. If a variable is in env_allow, bypass the secret check.

```yaml
schema: "nika/workflow@0.12"
env_allow: [ELEVENLABS_API_KEY, REPLICATE_TOKEN]
tasks:
  - id: tts
    with: { key: $env.ELEVENLABS_API_KEY }  # Now works
```

**Alternative simpler fix**: Document the workaround (`inputs:` + `--input`) prominently and add a clear error message instead of silent null.

---

# SECTION D -- P-ORCHESTRATE (3 semaines)

## D.1 Architecture

**Design**: Option B -- sub-workflows via `nika:run`. DAG remains immutable. Orchestrator is an `agent:` verb task generating `.nika.yaml` content as strings, executing via extended `nika:run` with inline YAML.

## D.2 Implementation Plan (6 parts)

### Part 1: `goal:` AST Field (4 files, ~50 LOC)
```
nika-core/src/ast/raw/workflow.rs     -- Add pub goal: Option<Spanned<String>>
nika-core/src/ast/raw/parser.rs       -- Add "goal" to known_workflow_keys, parse field
nika-core/src/ast/analyzed/workflow.rs -- Add pub goal: Option<String>
nika-core/src/ast/analyzer/analyze.rs  -- Thread goal through analysis
```

Tests: 3 (parse goal, missing goal, goal + tasks)

### Part 2: `orchestrate:` Config (4 files, ~80 LOC)
```
nika-core/src/ast/raw/workflow.rs     -- Add pub orchestrate: Option<Spanned<Value>>
nika-core/src/ast/raw/parser.rs       -- Parse orchestrate block
New: nika-core/src/ast/orchestrate.rs  -- OrchestrateConfig struct
nika-core/src/ast/analyzer/analyze.rs  -- Validate orchestrate config
```

OrchestrateConfig:
```rust
pub struct OrchestrateConfig {
    pub max_rounds: u32,           // default 10
    pub confidence_target: f64,    // default 0.85
    pub agent: Option<String>,     // preset reference
    pub max_cost_usd: Option<f64>,
}
```

Tests: 5 (parse config, defaults, validation, agent ref, cost limit)

### Part 3: Orchestrator Rewrite (1 new file, ~200 LOC)
```
New: nika-engine/src/runtime/orchestrate.rs
```

Key function: `wrap_as_orchestrator(workflow: &AnalyzedWorkflow) -> AnalyzedWorkflow`
- Detects goal: on workflow
- Wraps all tasks into a single agent: task
- Builds system prompt from goal + task templates + agent definitions
- Tools: [nika:run, nika:records, nika:cost, nika:orchestrate, nika:complete]
- completion: { mode: explicit }

Tests: 6 (wrapping, prompt building, round tracking)

### Part 4: `nika:run` Inline YAML (1 file, ~100 LOC)
```
nika-engine/src/runtime/builtin/run.rs -- Add yaml_content: Option<String> parameter
```

Currently accepts file path only. Add:
```json
{ "yaml_content": "schema: ...\ntasks:\n  - id: step1\n    infer: 'Hello'" }
```

Security: depth limiting (max 3 nested runs), timeout inheritance, same validation.

Tests: 8 (inline YAML, depth limit, timeout, malformed YAML)

### Part 5: Round Tracking (2 files, ~80 LOC)
```
nika-engine/src/runtime/orchestrate.rs -- Add round counter + budget tracking
nika-engine/src/runtime/builtin/introspect_orchestrate.rs -- Enhance nika:orchestrate response
```

nika:orchestrate returns:
```json
{
  "round": 3,
  "max_rounds": 10,
  "records_count": 7,
  "goal": "...",
  "confidence_target": 0.85,
  "cost_used_usd": 1.23,
  "cost_limit_usd": 5.0
}
```

Tests: 5 (round tracking, budget, cost limit)

### Part 6: Events (1 file, ~40 LOC)
```
nika-event/src/log.rs -- 5 new EventKind variants
```

New variants:
- `OrchestratorStarted { goal, max_rounds, agent }`
- `OrchestratorRound { round, records_count, cost_usd }`
- `OrchestratorSubWorkflow { round, yaml_hash, task_count }`
- `OrchestratorCompleted { rounds, total_cost_usd, confidence }`
- `OrchestratorFailed { round, reason }`

Tests: 4 (event emission, serialization)

## D.3 Total Estimates

| Part | Files | LOC (prod) | LOC (test) | Tests |
|------|-------|-----------|-----------|-------|
| 1. goal: field | 4 | 50 | 30 | 3 |
| 2. orchestrate: config | 4+1 | 80 | 50 | 5 |
| 3. Orchestrator rewrite | 1 | 200 | 100 | 6 |
| 4. nika:run inline | 1 | 100 | 80 | 8 |
| 5. Round tracking | 2 | 80 | 50 | 5 |
| 6. Events | 1 | 40 | 30 | 4 |
| **TOTAL** | **14** | **550** | **340** | **31** |

## D.4 Order d'execution

```
Part 1 (goal:) → Part 2 (orchestrate:) → Part 6 (events) → Part 3 (rewrite) → Part 4 (nika:run) → Part 5 (rounds)
```

Chaque part = 1 commit. Tests verts apres chaque commit.

---

# SECTION E -- ENGINE PROVIDERNAME MIGRATION (1-2 jours)

## E.1 Remaining Fields (4 high-priority)

| Field | File | Current | Target |
|-------|------|---------|--------|
| InferParams.provider | nika-engine/src/ast/action.rs:60 | Option<String> | Option<ProviderName> |
| InferParams.provider_chain | nika-engine/src/ast/action.rs:84 | Option<Vec<String>> | Option<Vec<ProviderName>> |
| AgentParams.provider | nika-engine/src/ast/agent.rs:98 | Option<String> | Option<ProviderName> |
| AgentParams.provider_chain | nika-engine/src/ast/agent.rs:227 | Option<Vec<ProviderName>> | Option<Vec<ProviderName>> |

## E.2 Low-priority (skip)

- Workflow.provider: String (setup-time, not hot path)
- ResolvedAgent.provider: String (resolved once per agent)
- Config, loader, partial: String (I/O boundaries)

---

# SECTION F -- PHASE 2 ECOSYSTEM (6 semaines)

## F.1 Registry & Publishing (v0.56)
- GitHub-based static registry (zero infra)
- `nika pkg publish` command
- Seed with 20 packages from showcases
- Security scanning on install

## F.2 Community (v0.57)
- `nika new --ai "description"` (NL -> YAML)
- Course gamification (constellation, badges)
- WORKFLOW.md metadata standard

## F.3 Integration (v0.58-0.60)
- Telegram webhook trigger
- MCP server expansion
- Fine-tuning data pipeline
- Homebrew + GitHub releases + crates.io

---

# SECTION G -- 3 CONFIRMED ENGINE BUGS

## G.1 NIKA-BUG-001: $env blocks KEY/SECRET/TOKEN vars

**File**: `nika-engine/src/binding/resolve.rs:695-714`
**Fix**: Add explicit error message OR `env_allow:` header field.
**Priority**: HIGH (breaks real workflows like podcast-producer)

## G.2 NIKA-BUG-005: $() blocked in shell mode (NIKA-053)

**File**: `nika-engine/src/runtime/security.rs:143-149`
**Fix**: Pre-resolution validation OR context-aware check.
**Priority**: MEDIUM (security feature, workaround exists: use exec without shell)

## G.3 NIKA-BUG-004: retry behavior unclear

**File**: `nika-engine/src/runtime/runner.rs:1022-1085`
**Fix**: Document that task-level retry DOES work on non-fetch verbs. Schema validation retry is infer-only.
**Priority**: LOW (documentation, not code fix)

---

# SECTION H -- REGLES ABSOLUES

```
TESTS
  1. cargo test --workspace --lib    TOUJOURS (--lib = pas de keychain macOS)
  2. TDD: test FAIL -> fix -> test PASS -> full suite -> commit
  3. Si test casse -> REVERT, passe au suivant

COMMITS
  4. 1 fix = 1 commit (jamais de batch non-relie)
  5. Co-authors TOUJOURS:
     Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
  6. git push apres 2-3 commits
  7. cargo clippy --workspace -- -D warnings -> ZERO warnings

QUALITE
  8. JAMAIS commiter du code qui ne compile pas
  9. Si bloque 3x -> skip, note dans progress.md, continue
  10. JAMAIS marquer un bug "done" sans code fix + test reel
  11. Valider workflows reels: nika run <file> --no-live

VALIDATION WORKFLOWS
  12. 535 workflows doivent passer nika check (regression test)
  13. 15 course workflows doivent passer nika run (E2E)
```

---

# SECTION I -- ORDRE D'EXECUTION

```
C.1 (course workflows)     -> C.2 (shell blocklist)     -> C.3 ($env fix)
    -> D.1-D.6 (P-ORCHESTRATE, 6 commits)
    -> E.1 (engine ProviderName)
    -> F.1-F.3 (Phase 2)
```

---

# SECTION J -- RELANCE

```bash
cd /Users/thibaut/dev/supernovae/nika
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v12-handoff.md)"
```
