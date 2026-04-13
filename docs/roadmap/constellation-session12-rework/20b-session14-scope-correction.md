# Session 14 — Scope Correction (Post Phase 1 Review)

> **Auteur / date:** Claude Opus 4.6, 2026-04-11
> **Supersède:** `20-session14-mega-prompt.md` (handoff stale — 11 commits landed between draft and execution)
> **Source:** 4 agents parallèles (rust-architect, rust-pro, code-explorer, rust-async-expert) dispatched after Phase 0 drift detection
> **Gitignored** — local only, ne jamais push

---

## 0. Pourquoi ce document existe

Le mega-prompt `20-session14-mega-prompt.md` a été écrit contre le HEAD `a3e8d8ab8`. Entre sa rédaction et l'exécution S14, **11 commits additionnels ont landed**, dont 4 qui réutilisent les labels `W14-A0`, `W14-A1`, `W14-B0`, `W14-E1` pour **des scopes différents** de ce que le mega-prompt leur assignait.

| Label handoff | Scope handoff | Commit landed | Same? |
|---|---|---|---|
| W14-A0 | enrich `InferEvent::Done` streaming | `e0970025c` enriched `InferResponse` non-streaming | ❌ |
| W14-A1 | migrate fetch retry loop ~340 LOC | `58397ed8d` impl `supports_response_format` | ❌ |
| W14-B0 | real `McpPoolAdapter` + cancel | `d4885f715` error variant mapping only | ❌ |
| W14-E1 | ARCHITECTURE.md S14 update | `4a2555be9` **already done** | ✅ |

En plus, **deux bugs flaggés P0/P1 dans le handoff n'existent pas**:
- **BUG-P0** (`fetch.rs` `RwLockReadGuard !Send`): le code à `fetch.rs:147/273/957` utilise déjà le pattern owned-extract. Audit rust-async-expert: **zéro `!Send` guard held across `.await`** dans le retry loop. Pas un bug.
- **BUG-P1** (`nika-verb-exec` pre-spawn cancel): le token est passé dans `ShellCommand.cancel` et `TokioShell` l'honore pendant le run. C'est une micro-optimization (skip un fork si déjà cancel), pas un bug de correctness.

---

## 1. Baseline réel (HEAD au début de S14 execution)

```
HEAD     : eaa7f16c2  fix(engine): restore McpInvoke emission for non-builtin path (S14-BUG2 regression)

COMMITS S14 DÉJÀ LANDÉS (ordre chronologique) :
  0dc079757  S14-BUG1  NonZeroExit includes exit_code
  3cc49f3d1  S14-BUG2  remove duplicate McpInvoke event
  e0970025c  W14-A0    InferResponse += request_id/cost_usd + Provider::supports_response_format (stub)
  58397ed8d  W14-A1    RigProvider impl supports_response_format
  c2d486de4  W14-A2    nika-verb-fetch test P0 fixes
  d4885f715  W14-B0    VerbInvokeError::Mcp mapping (preemptive, path unreachable)
  2ddd28ca1  W14-B1    nika-verb-infer crate + 9 tests
  040bfad4a  W14-B3    verb_infer adapter + infer_caps()
  4a2555be9  W14-E1    ARCHITECTURE.md S14 update
  eaa7f16c2  (regression fix for S14-BUG2)

Crates   : 33
Engine   : 146,473 LOC
Tests    : ~10,840 lib
Clippy   : 0 warnings
```

---

## 2. Phase 1 Review Findings (synthèse 4 agents)

### rust-architect — verdict sur InferEvent::Done + McpPoolAdapter

**InferEvent::Done struct variant**: SAFE — mark enum `#[non_exhaustive]`, convert `Done(StopReason)` to `Done { stop_reason, request_id: Option<String>, finish_reason_raw: Option<String> }`. Pas de `cost_usd` sur `Done` (cost = dérivé caller-side depuis pricing catalog × TokenUsage, l'asymétrie non-streaming/streaming est architecturalement honnête).

**Hidden trap**: struct variant = breaking match exhaustiveness. Grep OBLIGATOIRE avant commit dans `nika-kernel-mock`, rig stream adapter (`nika-engine/src/providers/`), `nika-verb-infer`, tests helpers.

**McpPoolAdapter**: BLOCKER. Le trait `McpPool` est trop mince. Deferred S15 (voir `21-session15-handoff.md`).

### rust-pro — verdict sur fetch retry migration

**340 LOC estimate was wrong.** Actual décomposition:
- `safe_backoff_delay` + `MAX_BACKOFF_MS` (10 LOC)
- `parse_retry_after` (10 LOC)
- `merge_link_hreflang` family (61 LOC)
- Unit tests associés (172 LOC)
- **Total migratable pur: ~253 LOC**
- Retry loop control flow proper: ~160 LOC, **inséparable** de ~500 LOC de response handling couplé à `RunContext`, `fetch_cache`, `MediaRef`, closure `reqwest::Client` sur `allowed_hosts`

**Verdict: SLOWDOWN → rescope.** Ship les ~253 LOC de helpers purs + tests (W14-A2-analog), defer le retry loop orchestration à S15 (même shape que W14-B2).

**Send audit**: clean. Pas de `!Send` guards across await.

### code-explorer — McpPool trait gap analysis

| Method McpClient | McpPool trait covers? | Blocking? |
|---|---|---|
| `call_tool_with_retry_events(tool, params, task_id, event_log)` | N | **YES** — retry + cache + validation + McpRetry events live ici |
| `read_resource → ResourceContent` | Partial — returns `String`, drops blob | **YES** for W14-B4 |
| `list_tools` | N | No (pas appelé depuis invoke.rs) |
| Pool lifecycle (get_or_connect) | N | No (adapter wraps `Arc<McpClientPool>`) |
| Media pipeline (ContentBlock, CasStore, MediaProcessor) | N | **YES** — pas dans kernel, couplage engine |

**`NoopMcpPool` sites**: `invoke.rs:342` (builtin path, jamais appelé par builtins), `dispatch.rs:144` (test helper). Removal attend un vrai `McpPoolAdapter` qui attend trait expansion.

### rust-async-expert — cancel + Send audit + ProviderResponded mapping

**Invoke cancel wrapper**: `biased; select!` inside verb crate est SAFE. Pas de double-cancel race avec le bridge outer `select!` (pas biased) — ils guardent des futures différentes dans des stack frames différentes. Le `biased` est une amélioration de correctness vs l'outer.

**MCP drop semantics**: `nika-mcp::McpClient::call_tool_with_retry_events` drop la rmcp request on future-drop **mais n'envoie pas d'abort au peer**. Le serveur MCP peut avoir déjà dispatché le tool. À documenter dans `VerbInvokeError::Cancelled`.

**Send audit fetch.rs**: CLEAN. Zero guards across await (L147, L273, L957 tous owned-extract pattern).

**ProviderResponded field mapping**: les 8 fields flow déjà correctement dans `nika-verb-infer/src/lib.rs:155-164` pour le non-streaming path. Tests actuels n'assertent que 3 fields (`input_tokens`, `output_tokens`, `finish_reason`). À étendre pour inclure `request_id`, `ttft_ms`, `cost_usd`, `cache_read_tokens` (invariant S12-G2).

**Top traps**:
1. **W14-B2 double-emit**: `infer.rs:621` + `:1136` emit `ProviderResponded` depuis 2 sites. Quand le bridge flip à `nika_verb_infer::run()` (S15), supprimer les deux atomiquement.
2. Ne pas ajouter de timeout dans verb-invoke — deadline = bridge's job.
3. MCP cancel != un-executed — documenter.

---

## 3. Vrai scope S14 — 5 commits

### S14-α: InferEvent::Done struct variant enrichment

**Fichier**: `tools/nika-kernel/src/provider.rs`

**Changes**:
```rust
#[non_exhaustive]  // NEW: mark enum non_exhaustive
pub enum InferEvent {
    // ... existing variants unchanged
    Done {
        stop_reason: StopReason,
        request_id: Option<String>,
        finish_reason_raw: Option<String>,
    },
}
```

**Match sites à mettre à jour** (grep before commit):
- `nika-kernel-mock/src/provider.rs` (MockProvider stream impl)
- `nika-engine/src/provider/rig/*` (rig stream adapter)
- `nika-verb-infer/src/lib.rs` + tests
- Tout autre site qui `match ev { InferEvent::Done(sr) => ... }`

**Tests**: unit test sur les 3 fields present + destructuring.

**LOC delta**: ~40

### S14-β: Fetch pure helpers migration

**Source**: `tools/nika-engine/src/runtime/executor/fetch.rs`
**Target**: `tools/nika-verb-fetch/src/retry.rs` + `hreflang.rs` (nouveaux fichiers)

**À déplacer**:
- `safe_backoff_delay` + `MAX_BACKOFF_MS` → `retry.rs`
- `parse_retry_after` → `retry.rs`
- `merge_link_hreflang` + `merge_link_hreflang_value` + `dedup_hreflang` → `hreflang.rs`
- 18 unit tests (parse_retry_after ×6, backoff ×10, is_html_content_type ×2) → tests alongside

**Pré-check**: vérifier `nika-verb-fetch/Cargo.toml` pour `reqwest` direct dep (pour `HeaderMap`). Probablement déjà transitive via `nika-kernel::http` mais à confirmer.

**Re-export** depuis engine bridge: `pub(crate) use nika_verb_fetch::retry::{safe_backoff_delay, parse_retry_after, MAX_BACKOFF_MS};` dans fetch.rs pour garder le bridge compilant.

**LOC delta**: engine −253, nika-verb-fetch +253 (net ≈ wash en workspace, engine shrinks)

### S14-γ: VerbFetchError new variants

**Fichier**: `tools/nika-verb-fetch/src/error.rs`

**Variants ajoutés** (prep pour S15 retry loop orchestration):
```rust
#[error("fetch retry exhausted after {attempts} attempts: {reason}")]
RetryExhausted {
    attempts: u32,
    last_status: Option<u16>,
    reason: String,
},

#[error("fetch deadline exceeded after {attempts}/{max_attempts} attempts")]
DeadlineExceeded {
    attempts: u32,
    max_attempts: u32,
},
```

**From impl**: `From<VerbFetchError> for NikaError` dans le bridge (pattern S13-B/C/D).

**LOC delta**: ~25

### S14-δ: Golden oracle enrichment for ProviderResponded

**Fichier**: `tools/nika-verb-infer/src/lib.rs` (tests module, ~L332-368)

Étendre l'assertion existante pour couvrir:
- `request_id`: Some(expected) ou None selon mock config
- `ttft_ms`: Some(_) quand provider le fournit
- `cost_usd`: concrete value (pas juste "non-zero")
- `cache_read_tokens`: 0 par default, non-zero si mock le set

**Invariant respecté**: S12-G2 — never lifecycle-only, always assert concrete fields.

**LOC delta**: ~20

### S14-ε: verb-exec pre-spawn cancel check

**Fichier**: `tools/nika-verb-exec/src/lib.rs`

**Addition** (avant `caps.shell.run(cmd).await`):
```rust
if caps.cancel.is_cancelled() {
    return Err(VerbExecError::Cancelled {
        task_id: input.task_id.to_string(),
    });
}
```

**Test**: nouveau test `run_pre_cancelled_returns_cancelled` qui cancel le token BEFORE appelant run(), assert `VerbExecError::Cancelled`.

**Note**: ce n'est pas un bug de correctness (le token est déjà propagé dans `ShellCommand.cancel` et `TokioShell` l'honore), c'est une micro-optimization qui évite un fork superflu quand le task est déjà cancel.

**LOC delta**: ~15

---

## 4. Ordre d'exécution + verification ritual

```
S14-α (InferEvent)  → cargo check workspace + --no-default-features + lib tests
S14-β (fetch helpers) → cargo check + lib tests (verify 18 tests migrate cleanly)
S14-γ (VerbFetchError) → cargo check, trivial
S14-δ (golden oracle) → lib tests on nika-verb-infer
S14-ε (pre-spawn cancel) → lib tests on nika-verb-exec

Commit after each wave (1 FIX = 1 COMMIT per git-workflow.md).
Push deferred until all 5 green.
```

**Sacred invariants à respecter pendant l'exécution**:
- #1 no `!Send` guards across await
- #7 cargo check --no-default-features before commit
- #14 LOC estimates conservative (this doc reflects actual counts)
- #17 NO `infer_vision` / `infer_with_tools` on trait (respecté, on ne touche que `Done`)
- #19 imports table before code for any >1000 LOC file migration

---

## 5. Deferred to S15 — voir `21-session15-handoff.md`

Le handoff S15 documente:
- McpPool trait expansion (call_tool → McpToolResult, read_resource → McpResourceContent, cancel surface)
- McpPoolAdapter in `nika-engine::kernel_bridge::mcp_pool_adapter`
- Fetch retry loop orchestration (W14-A1 real scope, same shape as W14-B2)
- W14-B2 infer.rs bridge surgery (delete double-emit sites atomically)
- W14-B4 resource reads
- W14-E0 shim removal (NoopMcpPool × 2 sites)
- Wave C: nika-verb-agent extraction
- Wave D: TaskExecutor dissolution

---

## 6. Post-mortem du handoff obsolete

**Leçon #1**: un mega-prompt écrit en fin-de-session-N peut être obsolète avant le début de session-N+1 si des commits inter-sessions landent. **Toujours vérifier HEAD vs baseline handoff en Phase 0 pré-flight.**

**Leçon #2**: les labels de commits (`W14-A0` etc.) doivent être préfixés par session-dynamique-ID ou scope-hash pour éviter les collisions sémantiques entre drafts de plan et commits réels.

**Leçon #3**: la review Phase 1 a attrapé 2 phantom bugs (BUG-P0/P1) + 1 phantom docs update (W14-E1) + 1 mauvaise estimation LOC (340 → 253). **La socratic review sauve ~4h de code bullshit à chaque session**. Invariant #15 "socratic review before code" confirmé ×N.
