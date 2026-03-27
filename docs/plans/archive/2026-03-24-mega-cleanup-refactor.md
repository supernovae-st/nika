# Mega Cleanup & Refactor Plan — 2026-03-24

**Sources**: 4 parallel audit agents (Architecture, Telemetry, Bug Hunter, Rust Pro)

## Phase 0 — Critical Bugs (immediate)

| ID | Finding | File | Fix |
|----|---------|------|-----|
| B1 | Double history insertion in `chat_continue_*` (all 7 providers) | `chat.rs` | Remove manual push; rely on `add_to_history()` |
| B2 | CRLF byte offset `+1` instead of `+2` for `\r\n` | `document_links.rs` | Account for actual newline byte length |
| B3 | Copilot false-positive: detected whenever VS Code is installed | `install.rs:43-45` | Remove `editors.contains(&"vscode")` from OR condition |
| B4 | Byte-slice panic on non-ASCII `generation_id` | `header.rs:75` | Use `.chars().take(8)` instead of byte slice |

## Phase 1 — Provider Deduplication (~1500 LOC saved)

| ID | Finding | Fix |
|----|---------|-----|
| R1 | 7x `chat_continue_*` (110 lines each) in `chat.rs` | Generic method parameterized on `CompletionClient` |
| R2 | 7x `run_*()` in `providers.rs` | Same generic extraction |
| R3 | `Arc::from(task_id.as_str())` x35 allocations | Change `task_id: String` to `Arc<str>` |
| R4 | `.clone().unwrap_or_default()` x7 | `.as_deref().unwrap_or("")` |
| R5 | Stringly-typed provider dispatch | Use `ProviderKind` enum everywhere |

## Phase 2 — Telemetry Correctness

| ID | Finding | Fix |
|----|---------|-----|
| T1 | Cache write tokens not priced (Anthropic 25%) | Add `cache_write_tokens` param to `calculate_cost_with_cache()` |
| T2 | `error_code: None` at `runner.rs:684,977` | Emit NIKA-061 and forward error code |
| T3 | `claude-haiku-4-5` model name mismatch | Verify against Anthropic API, fix pricing key |
| T4 | NIKA-160 collision (Syntax vs StartupError) | Reassign StartupError to NIKA-167 |
| T5 | NIKA-150-155 inline strings only | Promote to `NikaError` variants or `CoreError` enum |
| T6 | `estimate_tokens` uses byte length not char count | Use `.chars().count() / 4` |
| T7 | Pricing comment "March 2025" stale | Update to "March 2026" |

## Phase 3 — Architecture (God Crate Reduction)

| ID | Finding | LOC Impact | Fix |
|----|---------|------------|-----|
| A1 | `nika-engine` = 162K LOC god crate | — | Split per phases below |
| A2 | `init/` + `new/` + `display/` in engine | 27K out | Move to `nika-cli` |
| A3 | LSP handlers duplicated (engine 8K + lsp-core 5.5K) | 8K deleted | Complete migration to lsp-core |
| A4 | `nika-media` depends on `nika-mcp` (wrong direction) | — | Move `ContentBlock` to nika-core |
| A5 | 105 feature flags, 4-level forwarding | 74 eliminated | Runtime capability API |

## Phase 4 — Dependency Hygiene (quick wins)

| ID | Finding | Fix |
|----|---------|-----|
| D1 | 6 unused deps in `nika/Cargo.toml` | Remove `camino`, `xxhash-rust`, `humantime`, `unicode-width`, `terminal_size`, `nika-lsp-core` |
| D2 | 4 deps should be dev-deps | Move `reqwest`, `parking_lot`, `rustc-hash`, `ignore` |
| D3 | `dirs` v5 vs v6 mismatch | Unify via `[workspace.dependencies]` |
| D4 | 7 shared deps not in workspace | Add `infer`, `tree-sitter`, `ropey`, etc. |
| D5 | `thiserror` 1.0 → 2.0 | Upgrade (backward-compatible) |
| D6 | Missing `default-features = false` | Add for `chrono`, `petgraph`, etc. |

## Phase 5 — File Splits & Code Quality

| ID | Finding | Fix |
|----|---------|-----|
| F1 | `verbs.rs` 2816 LOC | Split into `infer.rs`, `exec.rs`, `fetch.rs`, `invoke.rs`, `agent.rs` |
| F2 | `runner.rs` 5755 LOC | Extract tests into `runner_tests.rs` |
| F3 | `rig.rs` 3135 LOC | Split into `rig/{mod,streaming,mcp_tool,verification}.rs` |
| F4 | `NikaError` 90+ variants | Split into domain enums |
| F5 | `run_infer()` 686 lines deep nesting | Extract sub-methods |
| F6 | `coerce_json_types` converts `"null"` string to JSON null | Add template-origin flag |
| F7 | `has_inline_value` returns `true` for no-colon lines | Return `false` for non-key-value |
| F8 | `exec` stdout UTF-8 lossy silently corrupts | Use `from_utf8()` with error |
| F9 | `validator.rs` wrong field extraction for AdditionalProperties | Extract from error message |

## Phase 6 — Observability

| ID | Finding | Fix |
|----|---------|-----|
| O1 | `run_infer()` no `#[instrument]` span | Add tracing span |
| O2 | `PolicyBlocked`/`FetchRetry` silent in TUI | Add TUI notifications |
| O3 | `parking_lot::RwLock` in async context (fragile) | Audit + document or switch to `tokio::sync` |
| O4 | `LimitTracker::clone()` copies stale `Instant` | Manual `Clone` impl with `reset()` |

## Execution Order

```
Phase 0 (bugs)     ████ immediate — parallel agents
Phase 4 (deps)     ████ quick wins — parallel with Phase 0
Phase 1 (dedup)    ████████ highest ROI — after Phase 0
Phase 2 (telemetry)████████ after Phase 1
Phase 5 (splits)   ████████████ after Phase 1 (verbs.rs split enables dedup)
Phase 3 (arch)     ████████████████ biggest, after stabilization
Phase 6 (observ.)  ████ anytime
```

## Metrics

| Metric | Value |
|--------|-------|
| Critical bugs | 4 |
| LOC to delete/move | ~37K |
| Deps to clean | 13 |
| Feature flags eliminable | 74 |
| Files >2K LOC to split | 6 |
