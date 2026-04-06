# SESSION-FINAL-HANDOFF — Nika Launch Readiness

> Date: 2026-04-06 | Version: v0.74.0 | Launch: May 5, 2026 (~29 days)
> Tests: ~10,339 GREEN | Crates: 17 workspace | Storage: V5

---

## Section 1: What's DONE

- Schema `nika/workflow@0.12` — 63 transforms, 62 builtin tools, 5 verbs
- Provider layer: 7 cloud + 1 local + 1 mock, fallback chain, auto-infer provider from model
- Structured output 5-layer defense (tool injection → extract → validate → retry → repair)
- `on_error:` fallback routing — ignore, retry_with_provider, fallback task
- Scheduling full stack (v0.72-v0.74): cron, overlap (skip/queue/replace), timeline, history dots, trigger, wizard, serve reconciliation, AST validation
- `nika keys` (5 commands) — set, remove, check, sync, list
- `nika eval`, `nika lint` (10 rules), `nika test --golden`, `nika env`, `nika graph`
- `nika serve` — OpenAPI 3.1, SSE streaming, job tags, batch run, artifact API, single-tenant auth
- LSP decoupled from nika-engine — depends only on nika-core (compile 90s → 15s)
- IDE extension v0.74.0 — all Phase 0 bugs fixed, DAG panel, sidebar tree, Cursor MCP auto-config, `nika/workflowGraph`, binary bundling code (`findBundledBinary`), semantic tokens, inlay hints, code lens, keybindings
- DaemonProvider trait with IPC + Embedded impls
- MCP server: 7 tools + 3 prompts + `enable_prompts()`
- 44-exercise course, 115 showcase workflows
- Security hardening — SEC-1 through SEC-9, NIKA-053 shell escaping
- CHANGELOG documented through v0.74.0
- Version sync — Cargo, npm, VS Code all at 0.74.0

---

## Section 2: What's Remaining

### MUST DO before launch

#### M1 — S10 Multi-Tenant Auth (~850 LOC, ~4h)
Replace single `NIKA_SERVE_TOKEN` with named BLAKE3-hashed API keys.
Blueprint at `docs/plans/2026-04-06-s10-multi-tenant-auth-blueprint.md`.
Files: `nika-storage/src/lib.rs` (V6 migration), `nika-serve/src/lib.rs` (AuthMode), `nika-cli/src/serve.rs` (token commands).

#### M2 — VSIX Binary Bundling CI (~1d)
`findBundledBinary()` exists at `extension.ts:454` but CI never puts the binary there.
Fix: `.github/workflows/release.yml` — add platform VSIX build jobs that extract binary from .tar.gz into `server/`.
BUG: `download-artifact@v4` vs `@v8` mismatch + binary never extracted from archive.

#### M3 — Docs: Remove `enable_extractor` (~15min)
`docs/content/user-guide/infer-verb-guide.md:208` documents removed feature. Delete section, renumber layers.

### SHOULD DO before launch

#### S1 — Fix CI release.yml issues (~2h)
- `download-artifact@v4` → `@v8`
- Add tar.gz extraction step before VSIX packaging
- Add `chmod +x` after extraction

#### S2 — Fix `deactivate()` interval leak (~15min)
`extension.ts` `deactivate()` doesn't clear `statusPollInterval`. Add `clearInterval()`.

#### S3 — Remove hidden vault-reset command (~15min)
`tools/nika-cli/src/provider.rs:53` has dead `vault-reset` command.

#### S4 — Fix Dockerfile version (~5min)
`tools/nika/Dockerfile:56` has `ARG VERSION=0.54.0`. Update to `0.74.0`.

#### S5 — Clean stale TODO comment (~5min)
`tools/nika-engine/src/runtime/for_each.rs:28` — function is wired but TODO says it isn't.

### NICE TO HAVE

- Schedule cost projection ($/month per cron trigger) in `nika schedule list`
- `nika help cron` cheat sheet topic
- Schedule-aware lint rules (L100+)
- `nikaProviders` tree view (declared in package.json, no TreeDataProvider wired)
- MCP `nika_dag_visualization` block-style `depends_on:` parsing

### POST-LAUNCH

- PostgreSQL backend (sqlx, feature-gated)
- Observability UI (htmx + uPlot embedded dashboard)
- `on_error` depth > 1
- `runner.rs` decomposition (8,252 lines → split)
- NikaError 103-variant flattening
- YAML anchor bomb protection (skill/agent file size limit)

---

## Section 3: Known Bugs

| ID | Severity | Description | Location |
|----|----------|-------------|----------|
| CI-1 | HIGH | `download-artifact@v4` mismatch + binary not extracted | `.github/workflows/release.yml:887` |
| EXT-1 | MEDIUM | `deactivate()` doesn't clear statusPollInterval | `extension.ts:967` |
| DOCS-1 | MEDIUM | `enable_extractor: true` documented but removed | `infer-verb-guide.md:208` |
| DEAD-1 | LOW | Hidden `vault-reset` command references deleted feature | `provider.rs:53` |
| MCP-1 | LOW | `nika_dag_visualization` doesn't parse block-style `depends_on:` | `server.rs:282-295` |
| TODO-1 | LOW | Stale TODO comment (code is actually wired) | `for_each.rs:28` |

---

## Section 4: Test Suite Status

- **10,339 passed**, 0 failed, 1 ignored (intentional — needs API key)
- **Clippy**: clean (0 warnings)
- **Formatting**: 6 files with minor drift — `cargo fmt --all`
- **0 blockers** in tech debt scan
- No `todo!()` or `unimplemented!()` in production code

---

## Section 5: Launch Checklist

```
PRE-LAUNCH IMPLEMENTATION
[ ] M1: S10 multi-tenant auth (~4h)
[ ] M2: VSIX binary bundling CI (~1d)
[ ] M3: Remove enable_extractor from docs (15min)
[ ] S1: Fix CI release.yml (2h)
[ ] S2: Fix deactivate() interval leak (15min)
[ ] S3: Remove vault-reset dead code (15min)
[ ] S4: Update Dockerfile version (5min)
[ ] S5: Clean stale TODO comment (5min)

VERIFICATION
[ ] cargo test --workspace --lib (all pass)
[ ] cargo clippy --workspace -- -D warnings (clean)
[ ] cargo fmt --all --check (clean)
[ ] cd editors/vscode && npm run compile (builds)

VERSION & RELEASE
[ ] Tag: git tag v0.75.0 && git push --tags
[ ] CHANGELOG: update for v0.75.0

DISTRIBUTION (automated via .github/workflows/release.yml)
[ ] GitHub Releases — auto on tag
[ ] Homebrew tap — auto on tag
[ ] crates.io — auto on tag
[ ] Docker Hub + GHCR — auto on tag
[ ] VS Code Marketplace — npx vsce publish (needs VSCE_PAT)
[ ] Open VSX — npx ovsx publish (needs OVSX_PAT)
[ ] npm — auto on tag
```
