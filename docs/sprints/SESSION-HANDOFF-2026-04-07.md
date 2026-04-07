# Mega Handoff — Nika v0.75.0 → Launch (May 5, 2026)

> **Date**: 2026-04-07 | **Version**: v0.75.0 | **Tests**: 10,411 GREEN
> **HEAD**: `8a6db6d7f` | **LOC**: 547K | **Crates**: 18 | **Launch**: May 5 (28 days)

---

## SITUATION REPORT

### What EXISTS (100% complete, no action needed)

| System | Status | Key Evidence |
|--------|--------|-------------|
| **Engine** | SHIPPED | 5 verbs, 64 transforms, 63 builtins, DAG, for_each, retry, on_error |
| **Providers** | SHIPPED | 9 cloud + 7 OpenAI-compat + native + mock = 18 providers |
| **Zero Ambiguity Design** | SHIPPED | Slash syntax, endpoints in nika.toml, no base_url, native bundled |
| **Scheduling** | SHIPPED | nika every, nika schedule, cron loop, serve reconciliation, 24h timeline |
| **S10 Auth** | SHIPPED | V6 schema, BLAKE3 TokenStore, moka cache, AuthMode, CLI (353 LOC), middleware |
| **LSP** | SHIPPED | Decoupled from engine, ls-types, 15s compile, diagnostics, completions |
| **IDE Extension** | SHIPPED | 8/8 phases, DAG webview, sidebar, MCP auto-config, 5-platform VSIX |
| **Media Pipeline** | SHIPPED | 24 tools, CAS store, provenance bundled, pipeline chaining |
| **Structured Output** | SHIPPED | 5-layer defense, all providers, repair, L0-L4 |
| **TUI** | SHIPPED | 3 views, live renderer, provider modal, routing dashboard |
| **Distribution Infra** | READY | npm, VSIX, Homebrew, Docker, crates.io, GitHub Releases, SLSA |
| **Stabilization** | DONE | 15/15 fixes, 4/4 tests, zero clippy warnings |

### What REMAINS (to tag v0.75.0 release)

| Item | Effort | Blocking? |
|------|--------|-----------|
| S11: PostgreSQL store | 3h | No (SQLite works for launch) |
| S12: Final polish | 1h | No |
| Tag v0.75.0 release | 5min | YES (triggers all CI publishing) |
| Verify CI passes | 30min | YES |

### Decision Point

**Option A: Tag v0.75.0 NOW** — SQLite is fine for single-server, PostgreSQL is multi-server (post-launch).
**Option B: Implement S11 first** — 3h, then tag.

**Recommendation**: Option A. PostgreSQL is a scaling concern, not a launch blocker.

---

## SYSTEM PROMPT FOR NEXT SESSION

Copy this entire block into the next Claude Code session:

```
On est sur nika, main branch. v0.75.0, 10411 tests GREEN, 547K LOC, 18 crates.
HEAD: 8a6db6d7f. Launch: May 5, 2026 (28 jours).

## Contexte

Nika est un workflow engine YAML pour l'IA. Schema nika/workflow@0.12.
5 verbes (infer, exec, fetch, invoke, agent), 64 transforms, 63 builtins.
AGPL-3.0. Repo PUBLIC sur GitHub (supernovae-st/nika).

Tout est prêt pour le launch SAUF la release CI:
- S10 auth: DONE (V6 schema, BLAKE3, TokenStore, CLI)
- LSP: DONE (decoupled, 15s compile)
- IDE: DONE (8/8 phases, DAG webview, VSIX 5 platforms)
- Model/provider: DONE (slash syntax, endpoints, no base_url)
- Scheduling: DONE (cron, reconciliation, timeline)
- Stabilization: DONE (15 fixes, 4 tests)

## Ce qui reste

1. **S11 PostgreSQL store** (~3h) — remplacer SQLite par PostgreSQL pour nika serve multi-server
   - Blueprint dans docs/plans/2026-04-06-s9-s10-handoff.md section S11
   - Blocked by: rien (V6 schema déjà en place)

2. **S12 Final polish** (~1h) — dernières touches avant release
   - Vérifier CHANGELOG complet
   - Vérifier que `nika doctor` passe
   - Vérifier les showcase workflows
   - Run final: cargo test --workspace --lib (10411+ tests)

3. **Tag v0.75.0 release** — déclenche CI qui publie partout:
   - GitHub Releases (binaires signés macOS)
   - npm (@supernovae-st/nika + 5 platform packages)
   - VS Code Marketplace + Open VSX
   - Homebrew tap
   - Docker (ghcr.io + Docker Hub)
   - crates.io

## Règles

- `cargo test --workspace --lib` green après CHAQUE commit (always --lib, jamais keychain)
- 1 fix = 1 commit
- Co-author: ONLY `Nika 🦋 <nika@supernovae.studio>` (JAMAIS Claude/Anthropic)
- AGPL-3.0-or-later
- Zero backward compat (v0.x)
- Push après chaque commit

## Fichiers clés

- Engine: tools/nika-engine/src/ (135K LOC)
- Core AST: tools/nika-core/src/ (23K LOC)
- CLI: tools/nika-cli/src/ (8K LOC)
- Serve: tools/nika-serve/src/ (4K LOC)
- Storage: tools/nika-storage/src/ (2K LOC)
- IDE: editors/vscode/ (VSCode extension)
- Config: tools/nika-engine/src/config.rs + runtime/boot.rs

## Memory

Consulte ta mémoire pour le contexte complet:
- project_v075_stabilization.md — état actuel
- project_v074_mega_handoff.md — model/provider refactor
- project_model_provider_refactor_2026_04_06.md — Zero Ambiguity Design
```

---

## ARCHITECTURE SNAPSHOT

```
tools/
├── nika/                CLI binary (2K) — main.rs, cli/
├── nika-engine/         Execution engine (135K) — embeddable runtime
│   ├── src/provider/    18 providers (rig-core + OpenAI-compat + native + mock)
│   ├── src/runtime/     Runner, executor, agent loop, builtins, security
│   ├── src/ast/         Lower (Analyzed → Runtime types)
│   ├── src/binding/     Templates, 64 transforms, JSONPath
│   ├── src/display/     CLI renderers (live + classic)
│   └── src/tools/       File tools (read, write, edit, glob, grep)
├── nika-core/           AST + types + catalogs (23K) — pure, zero I/O
├── nika-vault/          Encrypted secrets (1.2K) — XChaCha20 + Argon2i
├── nika-daemon/         Background daemon (5K) — secrets, jobs, watch, cache
├── nika-init/           Project scaffolding (21K) — init wizard + course
├── nika-event/          EventLog, TraceWriter (4K)
├── nika-mcp/            MCP client + server (9K) — rmcp
├── nika-media/          CAS store + processor (13K)
├── nika-storage/        SQLite abstraction (2K) — V6 schema
├── nika-cli/            CLI subcommands (8K) — verbs, schedule, token, model
├── nika-tui/            Terminal UI (86K) — ratatui
├── nika-serve/          HTTP API server (4K) — axum, SSE, auth
├── nika-sdk/            Embedded SDK (3K) — programmatic API
├── nika-display/        Display crate (4K) — shared renderers
├── nika-lsp-core/       LSP intelligence (9K) — protocol-agnostic
└── nika-lsp/            LSP binary (2.5K) — tower-lsp
```

### Auth System (S10, complete)

```
Token Flow:
  nika serve token add --name "prod"
    → generate nk_<48hex> (raw token, shown ONCE)
    → BLAKE3 hash → store in serve_tokens table (V6)
    → startup: count_tokens() > 0 → AuthMode::MultiKey

  HTTP request:
    Authorization: Bearer nk_abc123...
    → BLAKE3(token) → moka cache (60s TTL, 10K cap)
    → miss? → DB → validate (revoked? expired?) → cache → Principal
    → inject into request extensions
    → rate limiter uses token_id

Files:
  tools/nika-storage/src/lib.rs     — V6 schema, TokenEntry, 7 CRUD methods
  tools/nika-serve/src/token_store.rs — AuthMode, Principal, TokenStore, generate/hash
  tools/nika-serve/src/auth.rs       — require_auth middleware + WWW-Authenticate
  tools/nika-cli/src/token.rs        — nika serve token add/list/revoke (353 LOC)
```

### Zero Ambiguity Design (v0.74, complete)

```
Resolution chain:
  1. provider: explicit → WINS ALWAYS
  2. model: has slash → first segment = endpoint/provider lookup
  3. model: prefix match → auto-infer (claude→anthropic, gpt→openai)
  4. nika.toml [provider] → workflow default
  5. detect_first_configured() → scan API keys

Examples:
  model: claude-sonnet-4-6              # → anthropic (auto-infer)
  model: groq/llama-3.3-70b            # → provider=groq, model=llama-3.3-70b
  model: h100/Qwen/Qwen3-8B            # → named endpoint from nika.toml
  provider: native                      # explicit override
  model: gpt-4o                         # → openai (auto-infer)

Removed:
  base_url: in YAML                     # → use [endpoints.*] in nika.toml
  model/provider mismatch warning       # → false positives with custom endpoints
  --features native-inference           # → bundled by default
```

### IDE Extension (editors/vscode/, complete)

```
Phases:
  0: Bug fixes (8 commits)              — activation, disposables, error handling
  1: Platform VSIX (2 commits)          — 5-platform bundled binaries
  2: DaemonProvider trait (2 commits)   — unified LSP/daemon interface
  3: MCP auto-config (1 commit)         — Windsurf, Cursor, VS Code
  4: DAG webview (1 commit)             — ELK.js layout, D3.js rendering
  5: Sidebar tree (1 commit)            — workflow navigator
  6: Crate decoupling (1 commit)        — nika-lsp drops nika-engine
  7: MCP expansion (1 commit)           — 3 new tools
  S2: Decomposition (4 commits)         — extension.ts → 5 modules
```

---

## BRAINSTORM: Post-Launch Features

### Priority 1 — User-facing (first 2 weeks post-launch)

1. **nika serve webhooks** — HTTP trigger for workflows (Telegram bot → nika serve → workflow)
2. **nika memory (codename Egghead)** — workflow learning, 22 mechanisms, 5 levels
   - Design bible: `docs/plans/2026-03-31-egghead-design-bible.md` (1400 lines)
3. **nika bench** — provider benchmarking with cost/latency/quality comparison
4. **OpenRouter first-class** — meta-aggregator, 200+ models through single key

### Priority 2 — Platform (first month)

5. **PostgreSQL store** (S11) — multi-server nika serve
6. **L2 scope enforcement** — glob matching on Principal.scope
7. **L3 RBAC** — admin/viewer roles in auth
8. **nika serve batch API** — POST /v1/batch/run for bulk workflow execution
9. **Remote MCP** — nika serve as remote MCP endpoint for IDE integrations

### Priority 3 — Ecosystem (first quarter)

10. **nika pkg publish** — package registry for sharing workflows
11. **nika studio web** — browser-based workflow editor (Mintlify-style)
12. **Smart routing** — cost/latency-based provider selection, A/B testing
13. **Fleet management** — endpoint health checks, load balancing

### Never (by design)

- New verbs (5 is sacred — use invoke for everything else)
- Python runtime (Rust only, embed via SDK)
- Direct database access (MCP protocol only)
- OS keychain access (NikaVault XChaCha20 only)

---

## KNOWN BUGS (P3, not blocking launch)

1. `nika check` warns on slash syntax models (analyzer doesn't parse slash)
2. Double `find_project_root_from` in main.rs (redundant filesystem walk)
3. `nika-napi` deprecated but not removed (was nika-sdk predecessor)
4. `runner.rs` still 5000+ LOC after decomposition (task_dispatch + structured_retry extracted)
5. EventLog drain is O(n) (could be O(1) with ring buffer)

---

## CI/CD Pipeline

```
On tag push (v0.75.0):
  ├── Build matrix: 9 platforms (macOS arm64/x64, Linux arm64/x64/musl, Windows x64)
  ├── macOS: Developer ID signing + notarization
  ├── VSIX: 5 platform-specific packages (bundled nika binary)
  ├── npm: @supernovae-st/nika + 5 platform packages
  ├── Homebrew: supernovae-st/homebrew-tap auto-update
  ├── Docker: ghcr.io/supernovae-st/nika + Docker Hub
  ├── crates.io: nika crate
  ├── SLSA: Build provenance + SBOM
  └── GitHub Release: Binaries + checksums + VSIX
```

Secrets needed: `CARGO_REGISTRY_TOKEN`, `NPM_TOKEN`, `VSCE_PAT`, `OVSX_PAT`,
`APPLE_DEVELOPER_ID_*`, `DOCKER_*`. All configured in GitHub repo settings.

---

## FINAL CHECKLIST BEFORE TAG

```bash
# 1. Verify tests
cd tools && cargo test --workspace --lib     # 10,411+ tests, 0 failures

# 2. Verify clippy
cargo clippy --workspace -- -D warnings      # 0 warnings

# 3. Verify formatting
cargo fmt --all --check                      # Clean

# 4. Verify VSCode extension
cd ../editors/vscode && npm run compile      # Builds

# 5. Verify CHANGELOG is complete
cat ../CHANGELOG.md | head -20               # v0.75.0 header present

# 6. Tag and push
cd ../.. && git tag v0.75.0 && git push --tags

# 7. Monitor CI
gh run list --limit 5                        # Watch release workflow
```
