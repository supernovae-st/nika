# MEGA HANDOFF — v0.54 → v0.55 Sprint

> **Copie ce fichier ENTIER comme premier message dans une nouvelle session Claude Code.**
> Budget: $20-30 API | Duration: autonomous | Mode: Fix, test, commit, push

---

## IDENTITY

Tu es un ingenieur senior Rust travaillant sur **Nika**, un workflow engine YAML semantique pour l'IA.
Tu travailles en FULL AUTONOMIE. Tu fixes des bugs, crees des tests, executes des workflows avec de vrais providers.
Tu ne demandes JAMAIS l'aide humaine sauf si tu es BLOQUE depuis 30min.

---

## CODEBASE SNAPSHOT — 2026-03-31

```
Repo:       /Users/thibaut/dev/supernovae/nika
Version:    v0.54.0
Tests:      9,057 passing, 0 failing, 2 ignored
LOC:        ~500K Rust (12 crates, excluding target/)
Binary:     ./tools/target/debug/nika
Schema:     nika/workflow@0.12
Clippy:     CLEAN (0 warnings)
Git:        main, fully pushed, 0 uncommitted
```

### Test Counts Per Crate
```
nika-init:     0    nika-daemon:  164    nika-lsp-core: 230
nika-core:   156    nika-engine: 4,322   nika-mcp:      388
nika-event:  877    nika-media:   329    nika-tui:    2,153
nika-cli:    146    nika-lsp:       0    nika:            0
                                         TOTAL:      9,057
```

### Crate Architecture
```
nika-core     AST parser, transforms, binding resolution
nika-engine   Runtime: runner, executor, 5 verbs, security, DAG
nika-event    EventKind enum (63 variants), EventLog, broadcast
nika-media    CAS store, 25 builtin tools, image/video processing
nika-mcp      MCP client, tool proxy, NikaMcpTool
nika-daemon   Unix socket daemon, secrets resolution
nika-tui      3-view TUI (Studio/Command/Control), ratatui
nika-cli      23 CLI commands, cliclack prompts
nika-lsp      Language Server (completions, hover, diagnostics)
nika-lsp-core LSP core logic, tree-sitter YAML
nika-init     Project scaffolding, course system
```

### Providers disponibles
- **OpenAI** : gpt-4o-mini, gpt-4o (OPENAI_API_KEY) — structured output natif ✓
- **xAI** : grok-3-fast (XAI_API_KEY) — structured output natif ✓
- **Gemini** : gemini-2.0-flash (GEMINI_API_KEY) — **FREE TIER EXHAUSTED** — 429 sur toutes les requetes
- **Anthropic** : sk-ant-a... present mais 0 credits — SKIP
- **Native** : pas de modeles telecharges — `nika model pull llama3.2:1b` pour tester

---

## COMMANDS

```bash
# Build (default features — fast, pas de native/media)
cd /Users/thibaut/dev/supernovae/nika/tools && cargo build -p nika

# Build (full features — native + media + fetch + charts)
cargo build -p nika -F nika/native-inference -F nika/media-thumbnail -F nika/media-optimize -F nika/media-chart -F nika/fetch-html -F nika/fetch-markdown -F nika/fetch-article -F nika/fetch-feed

# Tests (TOUJOURS --lib — jamais sans, sinon keychain popup macOS)
cargo test --workspace --lib

# Test un crate specifique
cargo test -p nika-engine --lib -- nom_du_test

# Run workflow
./tools/target/debug/nika run tests/e2e-overnight/A01-basic-structured.nika.yaml --no-live

# Validate workflow
./tools/target/debug/nika check tests/e2e-overnight/A01-basic-structured.nika.yaml

# Clippy
cargo clippy --workspace -- -D warnings

# Smoke test suite (tous les workflows gratuits)
bash tests/e2e-overnight/run-smoke.sh ./tools/target/debug/nika

# Download native models
./tools/target/debug/nika model pull llama3.2:1b    # ~1GB
./tools/target/debug/nika model pull mistral:7b     # ~4GB
```

---

## PHILOSOPHY & RULES ABSOLUES

### Core
```
Quality > Speed | Research > Assumption | Question > Code | Test > Implement | Verify > Ship
```

### 10 Commandments
1. **TDD** — test failing d'abord, fix, verify
2. **1 fix = 1 commit** — `type(scope): description` + co-authors
3. **`cargo test --workspace --lib`** DOIT passer apres CHAQUE fix
4. **JAMAIS `cargo test` sans `--lib`** (keychain popup macOS)
5. **Prompts structured output** = langage NATUREL, JAMAIS mentionner JSON
6. **AGPL-3.0-or-later** pour tous les crates
7. **`git add <specific files>`** — jamais `git add .`
8. **Push apres chaque 3-5 commits** (checkpoint)
9. **ZERO backward compat** — only @0.12 matters, 0 users
10. **ZERO dead code** — if unused, delete it

### Commit Format
```
type(scope): concise description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
**Types**: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `style`
**Scopes**: `tui`, `ast`, `runtime`, `mcp`, `provider`, `dag`, `event`, `security`, `e2e`

### Socratic Loop (apres CHAQUE action)
1. Mon test FAIL avant et PASS apres ?
2. Ai-je grep pour des patterns similaires ailleurs ?
3. Ce fix merite une entree CHANGELOG ?
4. Ai-je commit avec les co-authors ?

---

## DOCUMENTS A CHARGER

**Reference pendant execution :**
- `~/.claude/rules/nika.md` — Schema complet, 5 verbs, providers, syntax
- `~/.claude/rules/nika-bugs-and-patterns.md` — Bugs reels, BUG-001 to BUG-013
- `docs/plans/overnight-results.md` — Resultats session precedente
- `docs/plans/overnight-buglog.md` — Bug log detaille

---

## WHAT WORKS PERFECTLY (ne pas toucher)

Verifie par 20+ agents d'audit + 78 workflows E2E:

- **Media pipeline** — CAS O_EXCL atomic, blake3, zstd, 25 tools, ZERO bugs
- **DashMap** — ZERO races, correct clone-and-drop
- **Cancellation** — 3-point check, wired into exec+fetch+agent
- **Error codes** — 158 unique NIKA-XXX, 0 dupes, 43% with FixSuggestion
- **CI** — 8 workflows, 7 targets, cargo-deny + audit + geiger + CodeQL
- **Secrets** — Unix socket 0o600, env→daemon→error, NIKA_NO_DAEMON works
- **LSP** — completions, hover, diagnostics, zero panics
- **Token counting** — saturating_add everywhere
- **DAG** — wave scheduling, per-parent fail_fast, diamond pattern
- **Structured output** — 5-layer defense, 94 tests, all providers
- **Fetch** — 9 extract modes, 3-layer SSRF, binary CAS, DNS pinning
- **Retry** — Permanent error skip (401/403), Retry-After parsing, exponential backoff
- **Presets** — 8 named presets (think, lite, search...) — wired + emitting PresetApplied
- **PolicyEnforcer** — threaded through agent tool calls AND spawn_agent
- **FetchExhausted** — emitted at all 4 retry-exhaustion paths
- **max_tokens** — uses effective_max_tokens() (no more hardcoded 8192)
- **Orchestrator** — YAML syntax examples in system prompt
- **Cost warning** — tracing::warn for unknown model pricing

---

## WHAT'S BEEN FIXED IN THIS SESSION (20 commits)

### Security (5 fixes)
| # | Bug | Commit | Fix |
|---|-----|--------|-----|
| 1 | Shell mode bypass `false` → `is_shell` | deb342e | exec.rs:38 |
| 2 | IPv6 `::` SSRF bypass | 732475d | policy.rs:49, explicit UNSPECIFIED check |
| 3 | SECRET_RE +4 patterns (ASIA, gh[udr]_, SG., JWT) | cef99a7 | util/mod.rs:30, 17 redaction tests |
| 4 | MCP response leaks secrets | 8de44a5 | redact_value() at 4 sites |
| 5 | Child agents bypass PolicyEnforcer | 67c1deb | spawn.rs with_policy(), remove TODO |

### Correctness (4 fixes)
| # | Bug | Commit | Fix |
|---|-----|--------|-----|
| 6 | unwrap_or_default in transforms | f1917e2 | expect() + null handling docs |
| 7 | "null" string → Value::Null coercion | 0f9bd73 | Removed — "null" is valid data |
| 8 | timeout=0 warning → error | a67811f | Reject at analysis time |
| 9 | H04 reads PNG as text | ae84de5 | Remove binary file read task |

### Telemetry (1 commit)
| # | Event | Commit | Where |
|---|-------|--------|-------|
| 10 | ForEachItemStarted/Completed/Failed | 6da352f | runner.rs for_each loop |
| 11 | TaskCancelled (distinct from TaskFailed) | 6da352f | event enum + TUI + display |
| 12 | FallbackChainExhausted | 6da352f | executor routing fallback |

### Edge Cases (1 fix)
| 13 | MAX_FOR_EACH_ITEMS=10,000 | c27fe6a | runner.rs, prevents OOM |

### E2E (3 commits)
| 14 | 91 overnight E2E workflows | ef49d5b | 17 categories |
| 15 | Output assertions E01, D01, D03 | 82fd101 | exec-based test checks |
| 16 | I03 skill path fix | f1b676c | ./skills/ relative to workflow |

---

## REMAINING TODOS — PRIORITIZED

### P0: CRITICAL (fix before v0.55 tag) — ~4h

| # | Bug | File:Line | Effort | Details |
|---|-----|-----------|--------|---------|
| 1 | **Agent scope not wired** | rig_agent_loop/mod.rs:298 | 2h | `full/minimal/debug` parsed but TODO. In `minimal`: only listed tools. In `debug`: add introspection tools. |
| 2 | **LLM guardrails stub** | thinking.rs:58 | 3h | `type: llm` returns hard error. Implement: send output to judge LLM with `judge_prompt`, check `pass_pattern`. |
| 3 | **Circular with: bindings indirect** | validate.rs:184 | 2h | Only direct self-ref detected. Indirect cycles (A→B→A) not caught. Build dependency graph + DFS. |

### P1: HIGH (fix in v0.55) — ~6h

| # | Bug | File:Line | Effort | Details |
|---|-----|-----------|--------|---------|
| 4 | **SSRF redirect DNS re-pinning** | policy.rs | 1h | After DNS pin, HTTP redirects don't re-check SSRF. Use reqwest `redirect::Policy::custom()`. |
| 5 | **Cancellation in binding resolution** | runner.rs:1965 | 1h | Path traversal doesn't check `cancel_token.is_cancelled()`. Long JSON paths block cancellation. |
| 6 | **EventLog O(n) drain** | log.rs:1186 | 2h | Replace `Vec<Event>` with ring buffer (VecDeque). Heavy for_each workflows hit perf wall. |
| 7 | **Template resolution allocations** | template.rs:370 | 1h | Each `{{...}}` allocates new String. Pre-allocate `String::with_capacity(template.len() * 2)`. |
| 8 | **Binding from failed task warning** | resolve.rs | 30m | When binding to `$failed_task`, no warning. Add `tracing::warn`. |

### P2: MEDIUM (nice to have) — ~8h

| # | Bug | File:Line | Effort | Details |
|---|-----|-----------|--------|---------|
| 9 | **TUI ProviderName migration** | lifecycle.rs:66 | 2h | Raw strings → ProviderName enum |
| 10 | **Skills path resolution (engine)** | skill_injector.rs | 1h | CWD-relative vs workflow-relative. Workflow fix done (I03), engine fix needed for general case |
| 11 | **StructuredOutputTimeout event** | structured_output.rs:315 | 1h | Emit event before returning timeout error |
| 12 | **MCP reconnection event** | invoke.rs | 1h | Emit McpReconnected after successful retry |
| 13 | **Unicode blocklist bypass** | security.rs | 1h | Test fullwidth confusables (ｓｕｄｏ), zero-width spaces (s​u​d​o) |
| 14 | **E2E assertions expansion** | tests/e2e-overnight/ | 2h | Add programmatic assertions to 10+ more workflows |

### P3: LOW (defer to v0.56+)

| # | Bug | File:Line | Details |
|---|-----|-----------|---------|
| 15 | MaxTurnsReached dead variant | types.rs | Remove or wire |
| 16 | TOCTOU symlink race in file tools | context.rs:272 | Security edge case |
| 17 | repair_model not validated at config time | infer.rs:758 | Could fail at runtime |
| 18 | Vec::with_capacity() missing in hot paths | resolve.rs:259 | Micro-optimization |
| 19 | Orchestrate max_rounds/max_cost not enforced | runner.rs | Limits parsed but not checked |

---

## NATIVE GGUF TESTS (7 workflows)

**Pas encore executes** — les modeles ne sont pas telecharges.

```bash
# Download (one-time, ~5GB total)
./tools/target/debug/nika model pull llama3.2:1b    # ~1GB, rapide
./tools/target/debug/nika model pull mistral:7b     # ~4GB, lent

# Test (requires native-inference feature)
cargo build -p nika -F nika/native-inference
./tools/target/debug/nika infer "Say hello" --provider native --model llama3.2:1b

# Run workflows
for f in tests/e2e-overnight/N0*.nika.yaml; do
  echo "=== $(basename $f) ===" && ./tools/target/debug/nika run "$f" --no-live 2>&1 | tail -3
done
```

**Workflows N01-N07:**
- N01: basic native infer (llama3.2:1b)
- N02: native + mistral:7b
- N03: native structured output
- N04: native for_each
- N05: native + exec chain
- N06: mixed native + cloud
- N07: native temperature test

---

## E2E WORKFLOW EXECUTION STATUS (78/91 run)

### PASS (70)
```
E01-E07, D01-D03, D05-D07, S01-S05, F01-F12, C01-C09, R01-R03,
I02, A01, A02, A04, A05, A07, B01, B02, B05, B07, H01, H02, H04,
H06, T01, T02, T04, V01, V04, X01, X02, W01, W03
```

### EXPECTED FAIL (5)
```
G01 (SSRF), G02 (path traversal), G03 (cmd injection), G04 (blocklist),
G05 (IPv6), G07 (LD_PRELOAD), E08 (timeout), V02 (error codes), D04 (fail_fast)
```

### GEMINI 429 (8) — not code bugs
```
A03, A06, A10, B03, H03, H05, M01, M02, M03, W02
```

### NOT RUN (13) — need native models or API credits
```
N01-N07 (native GGUF), W01 (vision re-test), remaining H, remaining B
```

---

## KEY FILES (read these before modifying)

```
# Runtime core (the heart of nika)
tools/nika-engine/src/runtime/runner.rs        # DAG scheduler, for_each, task execution
tools/nika-engine/src/runtime/executor/mod.rs  # Task executor, routing, fallback
tools/nika-engine/src/runtime/executor/exec.rs # Exec verb
tools/nika-engine/src/runtime/executor/fetch.rs # Fetch verb + 9 extract modes
tools/nika-engine/src/runtime/executor/infer.rs # Infer verb + structured output
tools/nika-engine/src/runtime/executor/invoke.rs # Invoke verb + MCP
tools/nika-engine/src/runtime/executor/agent.rs # Agent verb setup
tools/nika-engine/src/runtime/rig_agent_loop/mod.rs # Agent loop (rig integration)

# Security
tools/nika-engine/src/runtime/security.rs      # Command blocklist, shell validation
tools/nika-engine/src/runtime/policy.rs         # SSRF, PolicyEnforcer, DNS pinning
tools/nika-engine/src/util/mod.rs               # redact_secrets(), SECRET_RE

# AST & Parsing
tools/nika-core/src/ast/parser.rs               # YAML → RawWorkflow
tools/nika-core/src/ast/analyzer/analyze.rs     # RawWorkflow → AnalyzedWorkflow
tools/nika-core/src/binding/transform.rs        # 38 pipe transforms
tools/nika-core/src/binding/resolve.rs          # Binding resolution

# Events
tools/nika-event/src/log.rs                     # EventKind (63 variants), EventLog

# Tests
tools/nika-engine/src/ast/tests_200_workflows.rs # 200+ AST parser tests
tools/nika-engine/src/runtime/security.rs        # Security tests (inline)
tests/e2e-overnight/                             # 91 E2E workflow files
```

---

## KNOWN BUGS NOT TO FIX (by design)

| Bug | Why It's OK |
|-----|-------------|
| G06 newline injection passes for benign commands | Blocklist covers dangerous patterns, not all injections |
| `nika:assert` only accepts literal booleans | Use exec-based `test` assertions instead |
| Gemini free tier exhausted | Provider quota, not code bug. Works with paid tier |
| Anthropic 0 credits | Skip anthropic workflows, use openai/xai |
| Mock provider structured output is synthetic | By design — `generate_mock_json(&schema)` |

---

## RECOVERY PROCEDURES

- **Compilation fails after fix:** Read error. Import manquant? Scope? Fix in same commit.
- **Tests fail after fix:** `cargo test -- --nocapture test_name`. Understand WHY.
- **Provider 429:** Wait 60s. If persistent, switch provider.
- **50+ tests break:** `git stash`. Rethink approach. Fix might be wrong.
- **Context window full:** Commit, push, write handoff in `docs/plans/`, new session.

---

## BUG TRACKING

**Tiens a jour `docs/plans/overnight-buglog.md` pendant TOUTE la session.**

**REGLES :**
1. CHAQUE bug trouve = une ligne
2. CHAQUE workflow execute = une ligne
3. CHAQUE artifact ecrit = verifie physiquement
4. Si output vide/faux = BUG, pas un succes
5. Si feature parsed but not wired = BUG
6. JAMAIS ignorer un echec

---

## EXECUTION ORDER (suggested)

### Wave 1: Quick Wins (~2h)
1. Fix P0-1: Agent scope wiring (mod.rs:298)
2. Fix P1-5: Cancellation in binding resolution
3. Fix P1-8: Binding from failed task warning
4. Download native models + run N01-N07
5. Push checkpoint

### Wave 2: Security + Correctness (~3h)
6. Fix P0-3: Circular binding detection
7. Fix P1-4: SSRF redirect re-pinning
8. Fix P2-13: Unicode blocklist bypass
9. Run security workflows G01-G07 (all must fail)
10. Push checkpoint

### Wave 3: Performance + Telemetry (~3h)
11. Fix P1-6: EventLog ring buffer
12. Fix P1-7: Template allocation optimization
13. Fix P2-11: StructuredOutputTimeout event
14. Fix P2-12: MCP reconnection event
15. Push checkpoint

### Wave 4: Polish + Tests (~2h)
16. Fix P2-14: Add assertions to 10+ more workflows
17. Fix P2-9: TUI ProviderName migration
18. Update CHANGELOG with all fixes
19. Final: cargo test + clippy + smoke test
20. Push final

### Wave 5: Handoff
21. Update `overnight-results.md` with new results
22. Create `docs/plans/v055-handoff.md` for next session
23. `git push` everything

---

## SUCCESS CRITERIA

A la fin de cette session :

- [ ] P0 items fixed (scope, LLM guardrails, circular bindings)
- [ ] P1 items fixed (SSRF redirect, cancellation, EventLog, template alloc, binding warning)
- [ ] Native GGUF workflows executed (N01-N07)
- [ ] Security workflows G01-G07 all fail correctly
- [ ] `cargo test --workspace --lib` passes (9,100+ tests expected)
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] Tout commite et pousse sur main
- [ ] CHANGELOG mis a jour
- [ ] Handoff ecrit pour session suivante

---

**Push souvent. Commit souvent. Fix tout. Sois pas superficiel.**
**TOUT echec = bug. TOUT output vide = bug. TOUT warning = investiguer.**
