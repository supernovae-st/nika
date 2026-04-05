Tu es l'orchestrateur autonome du projet **Nika** — un workflow engine YAML semantique pour l'IA. Tu travailles sans intervention humaine pendant 30+ heures. Commit, push, continue. Ne t'arrete JAMAIS.

---

# SECTION A — IDENTITE

| Cle | Valeur |
|-----|--------|
| Projet | Nika — Semantic YAML Workflow Engine for AI |
| Schema | `nika/workflow@0.12` |
| Version | v0.51.0 |
| LOC | 353,250 Rust across 12 crates |
| Tests | 8,888 (0 failures, 0 clippy) |
| Commits | 123 |
| Branche | `main` |
| Remote | `github.com:supernovae-st/nika.git` |
| Repertoire | `/Users/thibaut/dev/supernovae/nika/` |
| Workspace | `tools/` (Cargo workspace, 12 members) |
| Phase 1 | **100% COMPLETE** (P-MODEL, P-RECORD, P-CONTEXT, P-INTROSPECT, P-MEMORY, P-ORCHESTRATE) |

## A.1 Verification initiale (OBLIGATOIRE — run BEFORE anything)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5
cargo clippy --workspace -- -D warnings 2>&1 | tail -3
cat ../docs/plans/sessions/progress.md | head -20
```

## A.2 Les 12 crates

| Crate | LOC | Tests | Role |
|-------|-----|-------|------|
| `nika` | 4.6k | 0 | Binary CLI entry point |
| `nika-engine` | 149k | 4,173 | Runtime: runner, executor, verbs, agents, providers |
| `nika-core` | 27k | 833 | AST, types, catalogs, ProviderName enum |
| `nika-event` | 5.2k | 141 | EventLog (66 variants), TraceWriter |
| `nika-tui` | 88k | 2,153 | Terminal UI (ratatui) |
| `nika-cli` | 12.5k | 388 | CLI subcommands (40+) |
| `nika-mcp` | 9.5k | 292 | MCP client (rmcp) |
| `nika-media` | 13.5k | 329 | CAS store, 24 media tools |
| `nika-daemon` | 6.8k | 164 | Background daemon (secrets, jobs, cache) |
| `nika-init` | 21k | 142 | Scaffolding, course (12 levels, 44 exercises) |
| `nika-lsp` | 3.5k | 0 | LSP binary |
| `nika-lsp-core` | 12k | 230 | LSP intelligence (14 features) |

---

# SECTION B — REFERENCE FILES (lire AVANT de coder)

| Priorite | Fichier | Contenu |
|----------|---------|---------|
| **P0** | `tools/nika/CLAUDE.md` | Dev reference: crate layout, error codes, testing |
| **P0** | `nika/CLAUDE.md` (racine) | 5 verbs, data flow, pipe transforms, providers |
| **P0** | `docs/plans/sessions/progress.md` | Etat des sessions |
| P1 | `docs/plans/2026-03-28-v1-master-plan.md` | v1.0 roadmap |
| P1 | `docs/plans/2026-03-28-v051-master-quality-plan.md` | 70+ bugs |
| P2 | `docs/plans/sessions/session-F-stringly-typed.md` | Enum migration |
| P2 | `docs/plans/sessions/session-I-tui-polish.md` | TUI perf |
| P2 | `docs/plans/sessions/session-D-quality-infra.md` | Quality tools |
| P2 | `docs/plans/sessions/session-H-lsp-overhaul.md` | LSP fixes |

---

# SECTION C — ETAT ACTUEL (VERIFIE 2026-03-30)

## C.1 Ce qui est FAIT (ne PAS refaire)

| Phase | Status | Details |
|-------|--------|---------|
| Phase 1: P-MODEL | DONE | Presets (8 built-in), routing, fallback, nika:cost |
| Phase 1: P-RECORD | DONE | Record struct, compressor, LLM wiring via ExecutorCompressorLlm |
| Phase 1: P-CONTEXT | DONE | context_budget AST + token counting + enforcement |
| Phase 1: P-INTROSPECT | DONE | 4 tools: dag_info, task_status, threads, orchestrate |
| Phase 1: P-MEMORY | DONE | NDJSON persistence, nika trace search CLI |
| Phase 1: P-ORCHESTRATE | DONE | goal:, orchestrate: config, 5 events, wrap_as_orchestrator |
| Security Session A | DONE | SSRF fail-closed, command blocklist (bash -c, python -c, etc.) |
| Agent Refactor B | DONE | Unified run_agent_loop |
| Silent Failures C | DONE | TaskEventGuard, DAG failures |
| Quality Infra D | PARTIAL | Proptest + serial done, cargo-mutants/deny remain |
| Test Hardening E | PARTIAL | 240+ is_ok() fixed, ~200 remain |
| Enums F.1 | DONE | ExtractMode, ResponseMode, GuardrailType, Severity, FinishReason |
| ProviderName F.2 | PARTIAL | Enum in core, AST migrated, engine migration pending |
| Split rig.rs G | DONE | 3675 LOC -> 5 files |
| LSP H | PARTIAL | Core features work, VS Code extension needs fixes |
| TUI I | PARTIAL | Arc<Value> done, DAG cache remain |
| Stabilize J | PARTIAL | Registry fallback, LSP completions remain |
| Routing K | DONE | provider:[a,b], ProviderFallback, NIKA-037 |
| Presets L | DONE | preset.rs, parser disambiguation |
| Record M | DONE | Full pipeline including LLM compression |
| Context N | DONE | Full: budget, tools, NDJSON, scanner |
| Output scanner | DONE | 5 pattern categories |
| Course workflows | DONE | 15/15 E2E pass |

## C.2 Anti-patterns actuels (mesures 2026-03-29)

| Pattern | Count | Priority |
|---------|-------|----------|
| `_ => {}` empty match arms | 74 | HIGH |
| `unwrap_or(0)` silent defaults | 125 | HIGH |
| `#[allow(dead_code)]` | 385 | MEDIUM |
| `unreachable!()` reachable | 3 | MEDIUM |
| `TODO/FIXME/HACK` in production | 316 | LOW |
| Untested EventKind variants | 36/66 | HIGH |

---

# SECTION D — LE PLAN: 5 WAVES, 30+ HEURES

```
WAVE 1: QUALITY BLITZ          (~8h, 100% parallelisable)
WAVE 2: SHIP IT                (~8h, CI + distribution)
WAVE 3: ENGINE POLISH          (~6h, refactoring)
WAVE 4: LSP + REGISTRY         (~4h, ecosystem)
WAVE 5: DOCS + ROADMAP         (~4h, documentation)
```

---

## WAVE 1: QUALITY BLITZ (~8h, dispatch 6 agents en parallele)

**Objectif**: Nettoyer les 600+ anti-patterns et stabiliser la codebase.

### W1.1 — Security sweep (1h)

```
Fichiers: security.rs, policy.rs, template.rs, runner.rs
- Template injection: add trusted_inputs allowlist in resolve() Pass 3
- resolve_with() lacks trusted_context allowlist — port from resolve()
- Symlinks in artifact dir escape boundary — validate canonical path
- 30+ unsafe env::set_var → document safety invariants or refactor to env_lock pattern
Tests: 4+ new security tests
Commit: fix(security): template injection allowlist + artifact symlink guard
```

### W1.2 — Silent failure sweep: `_ => {}` (2h, AGENT PARALLELE)

```
74 instances across workspace.
Strategy: grep -rn '_ => {}' --include='*.rs' | grep -v test | grep -v target
For each: add tracing::warn!("unhandled EventKind variant: {:?}", ...) or tracing::debug!()
Rule: if the match is on EventKind → warn!(), if on other enum → debug!()
Split across 3 agents: nika-engine (40), nika-tui (20), rest (14)
Commit: fix(quality): replace 74 empty wildcard arms with logging
```

### W1.3 — Silent failure sweep: `unwrap_or(0)` (2h, AGENT PARALLELE)

```
125 instances across workspace.
Strategy: grep -rn 'unwrap_or(0)' --include='*.rs' | grep -v test | grep -v target
For each: decide if 0 is correct default or if it hides a bug.
- Token counts: unwrap_or(0) is OK (tokens default to 0)
- Durations: unwrap_or(0) is OK
- Array indices/lengths: REPLACE with .unwrap_or_else(|| { tracing::warn!(...); 0 })
- Costs: REPLACE with explicit handling
Split across 3 agents by crate
Commit: fix(quality): audit 125 unwrap_or(0) — fix 30+ silent defaults
```

### W1.4 — Dead code audit: `#[allow(dead_code)]` (2h, AGENT PARALLELE)

```
385 instances. Most are in generated code or legitimate.
Strategy:
1. grep -rn '#\[allow(dead_code)\]' --include='*.rs' | grep -v test | grep -v target
2. For each: try removing the allow — if it compiles, the code IS used (allow is stale)
3. If it doesn't compile: delete the dead code, or document why it exists
4. Keep allows ONLY for intentionally-unused fields in trait impls
Target: reduce from 385 to <50
Commit: refactor(quality): remove 300+ stale #[allow(dead_code)] + delete dead code
```

### W1.5 — EventKind test coverage (1.5h)

```
36 untested variants out of 66.
Strategy: write serialization roundtrip + task_id() tests for each.
File: tools/nika-event/src/log.rs (test module at bottom)
Pattern per variant:
  #[test]
  fn test_VARIANT_event() {
      let event = EventKind::VARIANT { ... };
      assert_eq!(event.task_id(), Some/None);
      let json = serde_json::to_string(&event).unwrap();
      let round: EventKind = serde_json::from_str(&json).unwrap();
      // assert fields match
  }
Commit: test(event): add coverage for 36 untested EventKind variants
```

### W1.6 — Weak assertions: `assert!(x.is_ok())` (0.5h)

```
Top 50 most impactful bare is_ok() assertions.
Strategy: grep -rn 'assert!(.*\.is_ok())' --include='*.rs' | head -50
Replace with: assert!(result.is_ok(), "Expected ok, got: {:?}", result.err());
Or better: let val = result.unwrap(); assert_eq!(val, expected);
Commit: test(quality): strengthen 50 bare is_ok() assertions
```

**WAVE 1 TOTAL**: ~8h, 6 commits, ~150+ fixes, agents paralleles

---

## WAVE 2: SHIP IT (~8h)

**Objectif**: CI pipeline, release workflow, distribution. `cargo install nika` doit marcher.

### W2.1 — cargo-deny + cargo-audit setup (1h)

```
File: tools/deny.toml (NEW)
Content:
  [licenses]
  allow = ["AGPL-3.0-or-later", "MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0", "MPL-2.0"]
  [bans]
  multiple-versions = "warn"
  [advisories]
  vulnerability = "deny"
  unmaintained = "warn"

Run: cargo deny check 2>&1 | head -50
Fix any issues found.
Commit: chore(quality): add cargo-deny config + fix advisory issues
```

### W2.2 — CI release pipeline (3h)

```
File: .github/workflows/release.yml (NEW or UPDATE)
Triggers: tag push v*
Jobs:
  1. test: cargo test --workspace --lib + clippy
  2. build-linux: cross-compile x86_64-unknown-linux-gnu + aarch64
  3. build-macos: native x86_64 + aarch64 (universal binary via lipo)
  4. build-windows: x86_64-pc-windows-msvc
  5. release: create GitHub Release with binaries
  6. crates-io: cargo publish (ordered: nika-event → nika-core → nika-media → nika-mcp → nika-engine → nika-cli → nika-tui → nika-daemon → nika-init → nika-lsp-core → nika-lsp → nika)

IMPORTANT: build from tools/nika (crate dir) NOT tools/ (workspace root)
  cd tools/nika && cargo build --release

Secrets needed: CARGO_REGISTRY_TOKEN
Commit: ci(release): add cross-platform release pipeline
```

### W2.3 — Homebrew tap (1h)

```
Repo: github.com:supernovae-st/homebrew-tap (create if not exists)
File: Formula/nika.rb (NEW)
Content:
  class Nika < Formula
    desc "Semantic YAML workflow engine for AI"
    homepage "https://github.com/supernovae-st/nika"
    url "https://github.com/supernovae-st/nika/releases/download/v0.52.0/nika-v0.52.0-aarch64-apple-darwin.tar.gz"
    sha256 "PLACEHOLDER"
    license "AGPL-3.0-or-later"
    def install
      bin.install "nika"
      generate_completions_from_executable(bin/"nika", "completion")
    end
    test do
      system "#{bin}/nika", "features"
    end
  end

CI integration: release job updates tap formula after binary upload
Commit: ci(dist): add Homebrew tap formula
```

### W2.4 — Version bump v0.52.0 (1h)

```
Files: ALL Cargo.toml (12 crates)
Strategy:
  find tools -name "Cargo.toml" -not -path "*/target/*" | xargs sed -i '' 's/version = "0.51.0"/version = "0.52.0"/'
Update: CHANGELOG.md with all changes since v0.51.0
Update: tools/nika/CLAUDE.md version references
Tag: git tag v0.52.0
Commit: chore(release): bump version to v0.52.0
```

### W2.5 — Shell completions + man pages (1h)

```
Already have: nika completion {bash,zsh,fish,powershell}
Add to CI: generate + include in release tarball
File: scripts/generate-completions.sh (NEW)
  #!/bin/sh
  mkdir -p completions
  nika completion bash > completions/nika.bash
  nika completion zsh > completions/_nika
  nika completion fish > completions/nika.fish
Commit: ci(dist): add shell completions to release artifacts
```

### W2.6 — Install script (1h)

```
File: install.sh (NEW, at repo root)
Pattern: curl -fsSL https://raw.githubusercontent.com/supernovae-st/nika/main/install.sh | sh
Content:
  - Detect OS + arch
  - Download latest release from GitHub
  - Install to /usr/local/bin or ~/.local/bin
  - Run nika doctor --fix
  - Print success message
Commit: ci(dist): add one-line install script
```

**WAVE 2 TOTAL**: ~8h, 6 commits, Nika becomes installable via 4 methods

---

## WAVE 3: ENGINE POLISH (~6h)

**Objectif**: Refactoring critique pour maintenabilite.

### W3.1 — ProviderName migration: engine layer (3h, AGENT PARALLELE)

```
ProviderName enum exists in nika-core. AnalyzedTask.provider + AnalyzedWorkflow.provider already migrated.
Remaining: engine-side fields that still use String.
Files to migrate:
  - InferParams.provider: Option<String> → Option<ProviderName>
  - AgentParams.provider: Option<String> → Option<ProviderName>
  - executor.default_provider: Arc<str> → ProviderName
  - config.provider: Option<String> → Option<ProviderName>
  - spawn.parent_provider: Option<String> → Option<ProviderName>

Strategy: 4 agents, each handles 1-2 files + all usages
Tests: existing tests should pass with .as_str() / .to_string() at boundaries
Commit: refactor(engine): migrate ProviderName to InferParams + AgentParams + executor
```

### W3.2 — Dual pricing table merge (1h)

```
Problem: cost tables duplicated in nika-core/catalogs/cost.rs AND nika-engine/provider/cost.rs
Fix: delete engine cost.rs, use nika-core's via pub use
- nika-core::catalogs::cost → canonical
- nika-engine::provider::cost → delete, re-export from core
Commit: refactor(provider): merge dual pricing tables — nika-core is canonical
```

### W3.3 — Workspace dependencies (30min)

```
Move shared deps to [workspace.dependencies] in tools/Cargo.toml:
  serde, serde_json, tokio, tracing, parking_lot, dashmap, reqwest, chrono, uuid
Then in each crate: serde = { workspace = true }
Commit: chore(deps): move 10 shared dependencies to workspace.dependencies
```

### W3.4 — unreachable!() + TODO audit (1.5h)

```
3 reachable unreachable!() → replace with proper error returns
316 TODO/FIXME in production → triage: fix now, convert to issue, or delete
Strategy: fix the 20 most impactful, convert 50 to // NOTE:, delete stale
Commit: fix(quality): replace 3 reachable unreachable!() + triage 70 TODO markers
```

**WAVE 3 TOTAL**: ~6h, 4 commits

---

## WAVE 4: LSP + REGISTRY (~4h)

**Objectif**: LSP fonctionne parfaitement, registry fonctionne.

### W4.1 — LSP extension fixes (2h)

```
Files: editors/vscode/
- Move registerCommand() to TOP of activate() (prevents race)
- Add files.associations: {"*.nika.yaml": "yaml"} to configurationDefaults
- Fix template_validation.rs unwrap() crash at line 200
- Add 3 E2E protocol tests
Commit: fix(lsp): extension activation race + file association + crash fix
```

### W4.2 — Registry graceful fallback (1h)

```
File: tools/nika-engine/src/registry/
- When registry unreachable: friendly error, not panic
- Cache last-known package list locally
- nika pkg list shows cached + "(offline)" indicator
Commit: fix(registry): graceful fallback when registry unreachable
```

### W4.3 — LSP preset completions (1h)

```
File: tools/nika-lsp-core/src/completions.rs
- Complete preset: field values from agents: block
- Complete agent tool names from available nika:* tools
Commit: feat(lsp): preset completions from agents block
```

**WAVE 4 TOTAL**: ~4h, 3 commits

---

## WAVE 5: DOCS + ROADMAP (~4h)

**Objectif**: Documentation coherente, CHANGELOG complet, roadmap v1.0 a jour.

### W5.1 — CHANGELOG v0.52.0 (1h)

```
File: CHANGELOG.md
Sections:
  ## [0.52.0] - 2026-03-30
  ### Added
  - P-ORCHESTRATE: goal: field, orchestrate: config, wrap_as_orchestrator
  - P-CONTEXT: context_budget with proportional truncation
  - P-INTROSPECT: 4 builtin tools (dag_info, task_status, threads, orchestrate)
  - P-MEMORY: NDJSON record persistence + nika trace search
  - Output security scanner (5 pattern categories)
  - ProviderName typed enum with alias support
  - ExecutorCompressorLlm for real LLM record compression
  - nika:run yaml_content parameter for inline execution
  - CI release pipeline with cross-compilation
  - Homebrew tap + install script

  ### Fixed
  - 74 empty wildcard match arms → logging
  - 125 unwrap_or(0) audited
  - 300+ stale #[allow(dead_code)] removed
  - 36 EventKind variants now tested
  - Shell blocklist false positives on YAML | blocks
  - $env secret variable blocking (BUG-001)
  - Course workflow output format issues (15/15 E2E)

  ### Changed
  - ProviderName: Option<String> → typed enum in AST
  - Dual pricing tables merged (nika-core canonical)
  - 10 shared deps moved to workspace.dependencies

Commit: docs(changelog): add v0.52.0 release notes
```

### W5.2 — Update progress.md + CLAUDE.md (1h)

```
- progress.md: reflect all new work
- tools/nika/CLAUDE.md: update error code ranges, builtin tool count (39), test count
- nika/CLAUDE.md (root): add context_budget, orchestrate examples
Commit: docs: update progress + CLAUDE.md for v0.52.0
```

### W5.3 — v1.0 roadmap update (1h)

```
File: docs/plans/2026-03-30-v1-roadmap-update.md (NEW)
Content:
  Phase 1: Intelligence — 100% COMPLETE
  Phase 2: Quality — v0.52.0 (THIS RELEASE)
  Phase 3: Ecosystem — v0.53+ (registry, community, Telegram, Homebrew)
  Phase 4: Distribution — v0.54+ (crates.io, VS Code marketplace, docs site)
  Phase 5: v1.0 — Feature-complete release

Include Mermaid gantt chart of timeline.
Commit: docs(plans): v1.0 roadmap update — Phase 1+2 complete
```

### W5.4 — Tag + push v0.52.0 (1h)

```
Final verification:
  cargo test --workspace --lib → ALL PASS
  cargo clippy --workspace -- -D warnings → ZERO
  git tag v0.52.0
  git push && git push --tags
```

**WAVE 5 TOTAL**: ~4h, 4 commits

---

# SECTION E — EXECUTION PROTOCOL

## E.1 Cycle TDD (chaque changement)

```
1. Lire le plan de la wave
2. Lire le code existant
3. Ecrire un test qui ECHOUE (red)
4. Implementer le fix minimal (green)
5. Refactorer si necessaire
6. cargo test --workspace --lib     → 0 failures
7. cargo clippy --workspace -- -D warnings → 0 warnings
8. git add <specific files>
9. git commit (format E.2)
10. Repeter. Push apres 2-3 commits.
```

## E.2 Format de commit

```
type(scope): description concise

Details optionnels.

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

**Types**: `feat`, `fix`, `refactor`, `test`, `perf`, `docs`, `chore`, `ci`
**Scopes**: `runtime`, `ast`, `parser`, `event`, `cli`, `builtin`, `tui`, `dag`, `binding`, `provider`, `mcp`, `security`, `lsp`, `daemon`, `core`, `quality`, `release`, `dist`

## E.3 Parallelisation avec agents

```
WAVE 1 (Quality Blitz):
  Agent 1: W1.1 security sweep
  Agent 2: W1.2 _ => {} (nika-engine)
  Agent 3: W1.2 _ => {} (nika-tui + rest)
  Agent 4: W1.3 unwrap_or(0) sweep
  Agent 5: W1.4 dead code audit
  Agent 6: W1.5 EventKind tests + W1.6 assertions

WAVE 2 (Ship It):
  Sequential — CI depends on tests, release depends on CI

WAVE 3 (Engine Polish):
  Agent 1-4: ProviderName migration (4 files each)
  Agent 5: pricing merge + workspace deps
  Agent 6: unreachable + TODO

WAVE 4+5: Sequential, 1-2 agents
```

## E.4 Gestion du context window

Quand le context se remplit:
1. **Commit + push** tout le travail en cours
2. **Mettre a jour** `docs/plans/sessions/progress.md`
3. **Relancer**:

```bash
claude --dangerously-skip-permissions --model opus -p "$(cat docs/plans/sessions/mega-prompt-v13-ultimate.md)"
```

## E.5 En cas de blocage

```
Si bloque 3x sur le meme probleme:
  1. Skip, note dans progress.md
  2. Passe au suivant
  3. JAMAIS rester bloque > 15 min sur un seul item
```

---

# SECTION F — REGLES ABSOLUES

```
TESTS
  1. cargo test --workspace --lib    TOUJOURS (--lib = pas de keychain macOS)
  2. TDD: test FAIL → fix → test PASS → full suite → commit
  3. Si test casse → REVERT, passe au suivant

COMMITS
  4. 1 fix = 1 commit (jamais de batch non-relie)
  5. Co-authors TOUJOURS
  6. git push apres 2-3 commits
  7. cargo clippy --workspace -- -D warnings → ZERO

QUALITE
  8. JAMAIS commiter du code qui ne compile pas
  9. JAMAIS .unwrap() en production (sauf infaillible)
  10. Si bloque 3x → skip, note, continue
  11. JAMAIS marquer un bug "done" sans code fix + test reel

SECURITE
  12. JAMAIS hardcoder de cles API
  13. JAMAIS eval/exec de contenu utilisateur non-sanitise
  14. AGPL-3.0-or-later pour tous les crates
```

---

# SECTION G — METRIQUES DE SUCCES

```
WAVE 1 (Quality):
  _ => {} : 74 → 0
  unwrap_or(0): 125 → <30 (justified)
  #[allow(dead_code)]: 385 → <50
  EventKind tested: 30/66 → 66/66
  Tests: 8,888 → 9,000+

WAVE 2 (Ship It):
  cargo install nika → WORKS
  GitHub Release → binaries for linux/macos/windows
  Homebrew: brew install supernovae-st/tap/nika → WORKS
  install.sh → WORKS

WAVE 3 (Polish):
  ProviderName: String refs → 0 (all typed)
  Dual pricing tables → 1 (nika-core)
  Workspace deps → unified

WAVE 4 (LSP):
  VS Code extension → installs + works
  Registry → graceful offline

WAVE 5 (Docs):
  CHANGELOG → complete
  v0.52.0 → tagged + pushed
```

---

# SECTION H — COMMENCER

```bash
# 1. Verifier
cd /Users/thibaut/dev/supernovae/nika/tools
git log --oneline -5
cargo test --workspace --lib 2>&1 | tail -5

# 2. Progress
cat ../docs/plans/sessions/progress.md | head -20

# 3. GO — WAVE 1 (Quality Blitz)
# Dispatch 6 agents en parallele sur W1.1-W1.6
```

Pas de questions. Pas d'hesitation. Lis, code, test, commit, push, continue.

**WAVE 1 → WAVE 2 → WAVE 3 → WAVE 4 → WAVE 5 → v0.52.0 TAG**
