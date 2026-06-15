# Changelog

All notable changes to Nika are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Nika follows [forever-v0.x](ROADMAP.md) — incremental quality, no v1.0 target.

Nika Diamond is a ground-up rewrite on an orphan branch (`main` ·
renamed 2026-05-06 from `nika-diamond`). Legacy v0.79.3 lives on
`brouillon` (renamed 2026-05-06 from `main`). Diamond starts at v0.80.0.

**Version history.** Nika has shipped since **v0.1.0**. The early engine
(**v0.1.0 → v0.28.0**, 96 tags) is on this repository as historical record;
the v0.29 → v0.79.3 legacy era is kept as a private reference; this changelog
tracks the Diamond rebuild from **v0.80.0-alpha** onward.

---

## [Unreleased]

### 🧩 Announce ladder s19.6 · nika-lsp L4 admission — the `nika lsp` language server (ADMITTED · 12-gate closed · 2026-06-15)

- **`nika-lsp` crate** · the Nika language server (`nika lsp`, stdio) — the
  v0.1 editor brain for `.nika.yaml`. ONE crate (nika-lsp-core collapsed in as
  internal `analysis::*` modules · per `nika-invariants` + collapse-vs-publish ·
  reconciles `D-2026-06-10-N6` steps 19.6/19.7). Stack: `lsp-server` 0.7 sync
  stdio loop + `lsp-types` 0.97 · pure analysis over `nika-schema`.
- **Diagnostics** reuse the SAME ADR-092 `nika check` ladder (one source of
  truth · task-anchored ranges) · **hover** on the 4 verbs + keywords AND on a
  task reference (`depends_on` item / `${{ tasks.X }}` → the target task's id +
  verb) · **completion** (keys · verbs · `model:` providers · the workflow's own
  task ids · auto-trigger on `.` `/` `[`) · **document symbols** ·
  **go-to-definition** for task refs.
- Feeds the `nika-vscode` extension, auto-detected via `caps.lsp` once
  `nika --help` lists `lsp` — zero extension change. 124 lib tests · mutation
  96.9% · the `nika lsp` subcommand wired into `nika-cli` (owns stdout · LSP
  exit-code convention).

### 🤖 Announce ladder s12 · nika-verb-agent L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-agent` crate** · the `agent` verb executor — the multi-turn
  ReAct loop (model → whitelisted tool dispatch → results fed back → repeat)
  per `nika-spec spec/02-verbs.md §agent`. The **4th and last verb**
  (`D-2026-05-22-N18` · the verb count is 4, absolute). Generic over three
  injected kernel seams: `ProviderInferDyn` (inference) · `ToolExecuteDyn`
  via `InvokeVerb` (dispatch) · `ToolDefinitionProviderDyn` (the tool-def
  source). Zero runtime tokio dep — every turn rides the injected providers.
- **The ToolDefinitionProvider seam** (`nika-kernel-ai`) · resolves the s12
  §8 blocker found 2026-06-11: the agent hands the model its whitelisted
  tools as `ToolDef`s, but only tool NAMES were in hand — nothing enumerated
  definitions. A new kernel trait (the `ToolExecute` pattern · `Dyn` twin ·
  `ToolDefsError` → NIKA-234) + `MockToolDefinitionProvider`. The wiring
  layer implements it over the builtin catalog + (later) live MCP
  `tools/list`.
- **Loop semantics (normative · spec §2)** · terminal-1 (no tool calls →
  `Completed`) and terminal-2 (`nika:done` → `ExplicitCompletion`, with the
  `result:` arg or the last assistant message) BOTH precede the budget gate
  — a concluded answer is a success even if its turn crossed the budget.
  Budgets FAIL (max_turns → NIKA-460 · max_tokens_total → NIKA-461, `>=`
  exhaustion, checked before spending more) with `partial_output` preserved.
- **Security (spec §3 · default-deny)** · the whole tool batch is whitelist-
  validated BEFORE any dispatch (a denied sibling fails the turn with zero
  side effects · NIKA-462 immediate, not fed back). `nika:done` is loop-owned
  (never dispatched · wins over batch-mates). Model-emitted names are length-
  capped + control-char-rejected, and the violation error carries a REDACTED
  name (NIKA-450 log-injection parity). Source-supplied tool defs are
  sanitized before reaching the model.
- **The glob whitelist** · gitignore semantics canonically (a spec
  portability invariant): `*` bounded by `/` and `:`, `**` crosses them,
  `!` negation, last-match-wins. Matched by an O(n·m) DP (correct under
  interleaved `*`/`**`) + a totality proptest on the model-controlled input.
- **Structured output** · the final message validates against the task
  `schema:` (NIKA-464) with `infer.schema:` parity — bare-parse then a
  string-aware balanced-span extraction (tolerates fences + prose).
- **3-lens review swarm** (spn-nika + rust-pro + feature-dev) · all findings
  folded same session: the budget-before-completion bug, the batch-validate
  security ordering, the `**`/`*` glob backtrack gap, log-injection
  redaction, saturating token math, INV-019 `AgentOutput::new()`, the
  max_turns ceiling. NIKA_460..466 registered · hub 460-469 row · API-locked.

### 📡 Telemetry vocabulary closes over the display contract (nika-event · additive)

- **6 new `EventKind`s** · `task_retrying` · `task_cancelled` ·
  `workflow_cancelled` · `cost_incurred` · `infer_chunk` ·
  `permit_checked` — every state the run UI can show (contract §3.1
  state machine) and every live-meter refold driver the contract names
  (§3.3) is now expressible by a canonical engine event. Cancellation is
  terminal-not-failure (a decision, not a defect). `permit_checked`
  makes the declared `permits:` boundary observable at runtime (the
  ADR-092 audit moat).
- **`EventClass`** · the coarse 7-class classifier (`EventKind::class()`)
  — renderers/routers branch on stable classes, not 17 variants.
- **Reference fold** · the `nika-schema` `verbs` example consumes the
  full vocabulary: `--events` renders the whole tape digestibly; `verbs
  workflow` folds the SAME tape into the animated DAG (retry arc ↻ ·
  live stream · ticking cost meter · permits counter). The state-machine
  coverage test pins « every UI row status is event-reachable ».

### 🔌 Announce ladder s11 · nika-verb-invoke L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-invoke` crate** · the `invoke` verb executor per
  `nika-spec spec/02-verbs.md §invoke` (third of the 4 verbs). Rides the
  kernel `ToolExecuteDyn` seam with the engine's builtin+MCP dispatcher
  injected — zero tool implementation of its own, zero Cargo dep on
  `nika-builtin`/`nika-mcp`.
- **The closed-namespace contract** · the tool-ref namespace set is CLOSED
  at v1 (`nika:` · `mcp:` only · `mcp:` requires the `server/tool` slash);
  the verb does the lightweight semantic check before dispatch (grammar
  SHAPE stays the upstream `nika-schema` `NIKA-PARSE` concern). Result
  mapping: `is_error: true` → NIKA-451, dispatcher `NotFound` →
  `UnresolvableTool`, other dispatch failures → NIKA-452.
- **Security guards (swarm)** · whitespace padding and ASCII control chars
  in the tool id are rejected before it reaches a `ToolCall`/log field
  (log-injection class); the derived fallback `call_id` appends a
  process-monotonic counter so repeated same-tool invokes don't collide on
  the kernel's unique-call-id contract.
- **Error one-voice** · NIKA-450..452 registered in the Verb range; the
  verb-range help moved into a `verb_help` helper (keeps `code_help` under
  the 100-line cap).
- 16 lib tests (1 totality proptest cross-checked against an independent
  predicate) · mutation all viable killed bar one documented equivalent ·
  clippy 0 · doc 0 · layering + deny green · tag `v0.80.0-alpha.7`.

### ⚙️ Announce ladder s10 · nika-verb-exec L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-exec` crate** · the `exec` verb executor per
  `nika-spec spec/02-verbs.md §exec` (second of the 4 verbs). Rides the
  kernel `ShellRunDyn` seam with the effect injected (`TokioShell` in prod ·
  `MockShell` in tests) — zero subprocess code of its own, zero Cargo dep on
  `nika-exec-runner` (the L2→L1 inversion through the kernel trait).
  `pre_validated` is NEVER set, so the s7 runner blocklist stays the floor
  (structurally pinned by test).
- **The capture one-obvious-way split** · default modes (`stdout` · `stderr`
  · `combined`) fail the task on a non-zero exit (NIKA-440 / spec
  NIKA-EXEC-001 · with a capped stderr tail); `capture: structured` returns
  `{ stdout, stderr, exit_code }` as DATA — the workflow branches on it, the
  task succeeds.
- **Verb-boundary input guards (NIKA-442)** · a NUL byte in command/stdin
  (silent shell truncation) and a malformed env key (`=` · NUL · empty ·
  child-env corruption) are refused before the runner call — the security
  swarm's two findings.
- **Error one-voice** · NIKA-440..442 registered in the Verb range ·
  `MockShell` aligned to the Send-variant traits + gained `enqueue_result`.
- 19 lib tests (3 proptests · Gate 10 parity vs brouillon) · mutation all
  viable killed bar one documented equivalent · clippy 0 · doc 0 · layering
  + deny green · tag `v0.80.0-alpha.6`.

### 🗣️ Announce ladder s9 · nika-verb-infer L2 admission (ADMITTED · 12-gate closed · 2026-06-11)

- **`nika-verb-infer` crate** · FIRST L2 verb crate — the `infer` verb executor
  per `nika-spec spec/02-verbs.md §infer` (one of the 4 verbs locked forever ·
  D-2026-05-22-N18). Resolves `model: provider/name` through the s8.5
  `nika-providers` registry (D-N17: providers live BELOW the verbs · no
  verb→verb sideways dep), shapes the kernel `InferRequest`, returns the full
  `InferResponse` for the future L3 engine's event/cost seam.
- **Structured-output floor in-crate** · `schema:` tasks get native
  `ResponseFormat::JsonSchema` when the profile supports it (instruction
  fallback otherwise), lenient JSON extraction (bare → fenced → first balanced
  string-aware span), `jsonschema` 0.33 validation (compiled ONCE per run —
  an uncompilable schema is NIKA-432 with zero paid round-trips), and a
  bounded validation retry (default 2 · spec-sanctioned before NIKA-INFER-002).
  Schema text re-injected into prompts is capped at 4096 chars.
- **Error one-voice** · `VerbInferError` speaks `NikaErrorCode` via the new
  registry-owned NIKA-430..433 (Verb range 430-479 opened · same pattern as
  the M2 computer-use ranges) · transience inherited from `ProviderError`,
  never overridden.
- **Gate 11 swarm (3 lenses · 0 P0)** folded same-session: compile-once
  validator · u8→u32 attempts counter (closes the u8::MAX budget saturation
  loop) · schema render cap · both transience branches pinned.
- 33 lib tests (3 proptests · Gate 10 parity vs brouillon shaping pinned) ·
  mutation 95.8% overall + 8/8 on the cap helpers · clippy 0 · doc 0 ·
  layering + deny bans green. New workspace dep `jsonschema` (default-features
  off · no network resolver).

### ♿ Phase 2 M2.3 · nika-a11y L1 admission (ADMITTED · 12-gate closed · 2026-05-25)

- **`nika-a11y` crate** · third computer-use L1 effect crate · implements the
  L0.5 `io::a11y::AccessibilityTree` trait (`snapshot` + `find` + `resolve_ref`)
  exposing the active window's accessibility tree as `AxNode` records. **macOS-first**
  (decision §4 of `docs/crate-specs/nika-a11y.md`): backend via the safe
  **`accessibility` 0.2** crate (`AXUIElement` · `TreeWalker` · the unsafe
  `ApplicationServices` FFI is encapsulated → crate stays `unsafe_code = forbid`);
  Linux `atspi` / Windows `uiautomation` deferred to a consumer signal (LOCK-031).
  B.1 spec (backend research: 3 vetted permissive crates verified on crates.io)
  → B.2 skeleton (`A11yError` NIKA-1200..1206 · `AxBackend` · `snapshot`/`find`/
  `resolve_ref` route through a `walk_tree` placeholder returning `BackendNotWired`).
- **ADR-081 Guard 3 (AX-secure-field redaction · MANDATORY-at-admission) is
  headless-complete at B.2** · a pure recursive tree-transform (`redact_secure_fields`
  / `is_secure_field`) strips `value` from any secure-text node (macOS
  `AXSecureTextField` subrole · AT-SPI `STATE_SENSITIVE`) to `None` (zero leak),
  applied before any node leaves the crate. The pure `find` filter
  (`matches_query` + depth-bounded `collect_matches`) ships too. 12 lib tests
  (incl. a proptest pinning the redaction invariant) · clippy 0 · doc 0 ·
  `cargo-machete` clean · `cargo deny` ok. `nika-a11y` added to `deny.toml`
  tokio wrapper allowlist. API primary-source verified via context7
  (`/eiz/accessibility`) before recommending the backend.
- **B.3 macOS `AXUIElement` walk wired** · `system_wide().focused_window()`
  rooted recursive `build_node` (role/label/value/subrole → `AxNode`) inside
  `spawn_blocking` (the `!Send` handle stays worker-local · CANCEL SAFETY) ·
  macOS-gated deps `accessibility` 0.2 + `core-foundation` 0.10 (CFString/CFType
  reads · all upstream symbols — `focused_window` · `value().downcast::<CFString>()`
  · `children().iter()` · `subrole()` — verified against the crate source before
  use). Non-macOS compiles to `BackendUnavailable` (NIKA-1205). `resolve_ref`
  backed by a `Mutex<Option<AxNode>>` cache of the last redacted snapshot + pure
  `find_by_id`. Pure `ax_role_from_str`. Closed the `BackendNotWired` placeholder
  (NIKA-1200 retired · slot reserved). `bbox` deferred (`None` · frame→`Rect`
  refinement).
- **B.4 12-gate close · ADMITTED** · extracted the pure `assemble_node` (role
  map + empty-title/subrole filter + `AxNode::new`) out of the FFI `build_node`
  to maximize headless coverage; added a `MAX_WALK_DEPTH` (512) recursion cap so
  an untrusted/deep/cyclic focused-app tree can't overflow the stack (caught by
  the Foreman-direct review). **Gate 5 mutation 34/41 viable caught (82.9 %)** ·
  100 % of the headless surface · 7 `AXUIElement`-walk mutants documented-exempt
  per ADR-003 Rule 2 (`docs/crate-specs/nika-a11y.md` §7.1). **Gate 11** ·
  sub-agents hit the 1M-context credit wall → Foreman-direct 3-lens review
  (PE-5.1 · rust-pro + Diamond + bug-hunt · all ADMIT). 14 lib tests + 1
  `#[ignore]` smoke · clippy 0 · doc 0 · machete clean · deny ok · workspace
  `--lib` 1170. Workspace 13/42 admitted · WIP nika-schema only.

### 🔤 Phase 2 M2.2 · nika-ocr L1 admission (ADMITTED · 12-gate closed · 2026-05-25)

- **`nika-ocr` crate** · second computer-use L1 effect crate · implements the
  L0.5 `io::ocr::OcrEngine` trait (`read` + `read_region`) via the pure-Rust
  **`ocrs` 0.12** engine (**`rten` 0.24** runtime · no C system dep · keeps
  `unsafe_code = forbid`). B.1 spec → B.2 skeleton (`OcrError` NIKA-1100..1109
  · pure frame/region validation · `BackendNotWired` placeholder) → B.3 real
  inference: `OcrBackend::with_models(detection, recognition)` eager-loads two
  `.rten` weight files from **explicit local paths** (sovereignty Rule 1 ·
  reads local files only · NEVER auto-downloads · models are operator/daemon-
  provisioned), `read`/`read_region` validate the RGBA8 `Frame` purely then run
  `prepare_input → detect_words → find_text_lines → recognize_text` inside
  `tokio::task::spawn_blocking` (the sync CPU-bound engine runs off the async
  runtime · kernel CANCEL SAFETY: a dropped future abandons the read with no
  side effects). The B.2 `BackendNotWired` placeholder is CLOSED (NIKA-1100
  retired · slot reserved) per `skeleton-option-a-pattern.md` §5.
- **`nika-ocr` 12-gate close (B.4)** · admitted — all 12 gates green
  (registry L1 · ADR-081 inherits 7-guard contract, owns none mandatory ·
  `#[non_exhaustive]` · zero-unwrap src · ~290 LOC · NIKA-1101..1109 ·
  cancel-safety · `test --workspace --lib` 1156 · clippy 0 · `cargo doc` 0 ·
  `cargo-machete` clean · `cargo deny` ok). **Gate 5 mutation 81/87 viable
  caught (93.1 %)** · 100 % of headless-reachable logic · 6 model-inference
  mutants documented-exempt per ADR-003 Rule 2 (need real `.rten` weights ·
  `docs/crate-specs/nika-ocr.md` §6.1). Pure helpers (`rgba_to_rgb` ·
  `crop_rgba` · `words_bbox_union` · `validate_frame` · `validate_region`)
  proptested + 100 % mutation-killed. **Gate 11 review** · sub-agents hit the
  1M-context credit wall → Foreman-direct 3-lens review per
  `orchestrator-autonomous-v6.md` PE-5.1 (rust-pro + Diamond-discipline +
  bug-hunt · all ADMIT · 1 P1 stale-module-doc fixed). Deps: `+ocrs +rten`
  (workspace) `+tokio` rt + `tempfile` dev · `nika-ocr` added to `deny.toml`
  tokio wrapper allowlist. API primary-source verified via context7
  (`/robertknight/ocrs`) before wiring · no phantom symbols.

### 🖥️ Phase 2 M2.1 · nika-screen L1 admission (ADMITTED · 12-gate closed · 2026-05-23)

- **`nika-kernel` `io::screen`** · NEW `capture_stream` additive trait method +
  `FrameStream` type alias (`Pin<Box<dyn Stream<Item = io::Result<Frame>> + Send>>`),
  the canonical kernel streaming idiom (cohérent `ai::provider::InferEventStream`).
  Zero breaking change · uses `futures-core` (NOT `tokio-stream`, which is
  L0.5 layer-banned per `Cargo.toml`). Begins the M2.1 6-batch dispatch (B.1).
- **`crate-layer-registry`** · `nika-screen` registered L1 — first computer-use
  effect crate (Gate 1). ADR-081 7-guard contract already shipped (`3e40c18b3`).
- **`nika-screen` crate** · B.2 skeleton (`ScreenError` NIKA-1000..1009 · 10 codes
  · `ScreenBackend` + consent/LED guard skeletons) → B.3 single-shot capture WIRED
  via `xcap` 0.9.5 (`list_displays` / `capture_full` / `capture_region` · sync OS
  calls wrapped in `spawn_blocking` so the `!Send` `Monitor` stays worker-local and
  dropped futures surrender promptly · zero-copy RGBA8 `Frame`) → B.4 wires
  `capture_stream` (bounded `tokio::mpsc` + dedicated capture thread · ~30fps
  cadence · drop-stop cancellation via channel-close · `futures_core::Stream`
  adapter over `poll_recv`). All 4 `ScreenCapture` methods now real — the B.2
  `BackendNotWired` skeleton is fully CLOSED. B.5 makes the ADR-081 guards real
  + ENFORCED · a fail-closed `ConsentGate` (guard 7 · in-memory · session-scoped
  · revocable · per-frame re-check inside the stream worker) gates every pixel
  capture, and a RAII `LedIndicator` (guard 6 · engaged-count) stays lit for the
  whole capture. xcap encapsulates the OS FFI
  (objc2 / x11 / windows) so the crate is `unsafe_code = forbid`-clean. Plan-dep
  correction · the
  plan's `nokhwa` is a WEBCAM lib (docs.rs verbatim); `xcap` is the screen-capture
  crate (per `cross-source-validation.md` §2.7).
- **`nika-screen` 12-gate close (B.6)** · admitted as the first L1 effect crate —
  all 12 gates green (registry · ADR-081 · `#[non_exhaustive]` · zero-unwrap ·
  LOC 943 · NIKA-1000..1009 · cancel-safety · `test --workspace --lib` 1125 ·
  clippy 0 · `cargo deny` ok · forward-compat). GAP-3 `From<ScreenPoint>` shim
  CARRIED FORWARD to M2.4 `nika-input` · `ScreenPoint` is a `cockpit_overlay`
  (Olympus) type, so a `From` impl in `nika-screen` would violate cross-flow
  D-2026-05-08-N1 (Nika→Olympus) and is an `io::input` (cursor) concern, not
  `io::screen`; the conversion lives on the Olympus consumer side (where
  `cockpit-input-injection` already mirrors it).

### ⚡ Perf profile + craft amendments (2026-05-12)

Pre-W3 perf-craft + architecture polish per 2-agent SOTA audit
(`spn-rust:rust-async-expert` + `spn-rust:rust-perf` parallel) ·

- **`Cargo.toml [profile.release]`** · `lto=fat` + `codegen-units=1` +
  `strip=symbols` + `panic=unwind` + `debug=line-tables-only` +
  `incremental=false` · matches ADR-061 SLSA L3 prep · ~5-10% perf
  delta on BGE-M3 cosine + BM25 + RRF hot paths · 2× build cost
  release only · dev unaffected.
- **`Cargo.toml [profile.bench]`** · inherits release + `debug=true`
  for `cargo flamegraph` + `perf annotate` at W3 admission Gate 7.
- **4 `const fn` promotions in `nika-types`** · `Cost::new` ·
  `Cost::zero` · `Cost::is_zero` · `Trust::new` · `Trust::is_at_least` ·
  unlocks `const SATELLITE_COST: Cost = Cost::from_milli_usd(5)` at
  call-sites = zero runtime eval. `From`-trait + `Option::map` blocked
  (not const-stable yet · 2027+ horizon · per Rust 1.91 limits).
  Forward-compat per ADR-007 · `pub fn → pub const fn` non-breaking.

### 📐 BLUEPRINT_2036 v1.3 amendments (2026-05-12)

Cumulative cascade v1.0 → v1.1 → v1.2 → v1.3 per `docs/architecture/
BLUEPRINT_2036.md` frontmatter · status proposal · annual decennial
review 2027-04+.

- **v1.1 (per-crate detail + best-enemies SOTA)** · 42-crate table
  with LOC + deps + trait + Gate-9 + admission target per row ·
  Restate/LangGraph/Temporal/Mem0/Letta differentiation matrix ·
  collapse-vs-publish principle § 1.5 locked
- **v1.2 (11/10 amplifiers + guardian framing)** · 9→4 amplifier ADR
  fold (saves 5 empty shells · `socratic-research-discipline.md`
  Step 5 Option D) · §4.7 anti-Palantir + AI-2027 trajectory mapping ·
  14 prior Nika-mappings re-validated 2026-Q2
- **v1.3 (perf craft + async depth · this entry)** · §4 RRF fairness ·
  Loom scope (2-thread minimal + Shuttle PCT for full DAG) ·
  `consume_budget` cooperative scheduling · `[profile.release]`
  mirror · §4.5 ADR-066 `#[tracing::instrument]` discipline · NEW
  ADR-070 (`TaskTracker` + child-token fan-out · kernel-pure preserved
  per ADR-016 Alt-A) · ADR-041 `#[track_caller]` builder amendment

### 📚 Pre-launch hygiene shipped (2026-05-12)

- **Per-crate READMEs** · 4 missing of 8 shipped (`nika-error` ·
  `nika-catalog` · `nika-kernel` · `nika-kernel-mock`) following
  tokio/serde/thiserror SOTA pattern (~80-120L each)
- **`CODE_OF_CONDUCT.md`** · Contributor Covenant v2.1 boilerplate ·
  conduct@supernovae.studio · 4-tier enforcement ladder
- **`SECURITY.md`** · vulnerability disclosure policy · 72h ack · 90d
  disclosure · 11-row NIKA-271..389 defense layers table
- **`Cargo.toml [workspace.lints.rustdoc]`** · compile-time doc gate
  (broken_intra_doc_links=deny · private=warn · invalid_codeblock=deny)
- **`.github/workflows/diamond-ci.yml`** · semver-checks baseline ·
  `origin/nika-diamond` (renamed branch · stale since 2026-05-06) →
  `origin/main` · was silently failing

### 📚 Wave 4E — Mintlify rebuild + docs repo split (2026-04-17)

End-user documentation split out to a dedicated public repository and
rebuilt from the current workspace state.

- **`supernovae-st/nika-docs`** — new public repo, serves
  [`docs.nika.sh`](https://docs.nika.sh) via Mintlify. Replaces the
  in-engine `docs/mintlify/` directory, which is removed from this
  repo. Engine-internal docs (`docs/adr/`, `docs/architecture/`,
  `docs/crate-specs/`) stay here.
- **Mintlify content refreshed** — 2-tab navigation (Guide / Reference),
  honest v0.80 pre-release framing, live snapshot of 32 providers, 49
  capability rules, 35 ADRs (11 thematic groups), L0 architecture
  decisions, admission 12-gate walkthrough.
- **Dead pages purged** — 8 Mintlify pages that no longer mapped to the
  Diamond workspace state removed pre-split.
- Cross-links from this repo's README + ROADMAP point to
  `docs.nika.sh` for end-user content.

### ⚡ Swarm-3 Batches I.b + II ε.2/ε.3 + Wave 3A + Wave 4A + 4B seeds + Wave 4C (2026-04-17)

**Hygiene — Batch I.b vectors 30-33 (+4 new):**

- **Vector 30 `check-cancel-safety.sh`** — every `async fn` in
  `crates/nika-kernel/src/**` now carries a `// CANCEL SAFETY:` or
  `/// CANCEL SAFETY:` marker. 43 kernel methods annotated
  (cancel-safe contract: drop semantics, atomic vs non-atomic writes,
  `kill_on_drop` requirement, billing/telemetry exposure).
- **Vector 31 `check-owned-strings.sh`** — preventive ratchet: bans
  non-static `&str` in nika-catalog `pub` fields / `pub fn` return
  types. Catalog stays 100% `&'static str` per ADR-008 codegen pragma.
- **Vector 32 `check-unsafe-count.sh`** — `unsafe` token counter
  vs `scripts/hygiene/baselines/unsafe-count.txt` (currently 0).
  Substitutes cargo-geiger which is hostile to virtual manifests.
- **Vector 33 `check-layer-deps.sh`** — per-layer banned third-party
  deps (`[workspace.metadata.diamond] layer-bans`). L0 rejects 17
  deps (tokio family, rayon, async-std, smol, futures family,
  reqwest, hyper, axum, actix-web); L0.5 rejects 11.
- **Killed vector 7** (linear-issue-states stub) **and vector 18**
  (adr-dangling duplicate of vector 16).

**Wave 3A — engine post-commit hook for Olympus snapshots:**

- `scripts/hooks/post-commit-olympus-xtask.sh` wired via lefthook.
  Background `pnpm tsx olympus/scripts/xtask.ts` regenerates
  workspace.json + snapshots + hygiene-status.json on every engine
  commit; Olympus live-refreshes `/timeline`, `/graph/diff`,
  `/graph/fitness`, `/hygiene`.

**Wave 4A — v0.95 Cortex + v0.100 WASM reservations (R1-R5):**

- **R1 `EmbeddingSpec`** (`nika-types::embedding`) — Dtype,
  DistanceMetric, EmbeddingSpec; `#[non_exhaustive]` + snake_case wire.
- **R2 `MemoryFrameRef.trust: TrustLevel`** — sticky ingest taint;
  `#[serde(default)]` → UNTRUSTED fail-safe.
- **R3 `RecallQuery.tenant: TenantId`** — mandatory multi-tenant
  keyspace scope. `TenantId::default_tenant()` → `"default"`.
- **R4 `WasmPluginError::OutOfFuel` + `Trap { kind: TrapKind }` +
  `PluginCallContext`** — fuel metering, W3C-style trap taxonomy,
  per-call context with trust + cancel + budget.
- **R5 `MemoryLifecycle` trait** with default-impl consolidate/prune
  returning empty reports. Standalone; Cortex opts in at v0.95.

**Wave 4B seeds (telemetry foundations):**

- **#1 `SpanGuard.parent_span_id` + `links: Vec<SpanRef>`** — W3C
  Trace Context parent linkage unblocks Olympus `/trace`. Default
  `TracerProvider::start_child_span` backfills parent.
- **#3 `Timestamp(i64 unix_ns)` + `WallDuration(i64 nanos)`** in
  `nika-types::timestamp`. RFC 3339 Display via inlined Hinnant
  civil-from-days algorithm. Serde-transparent wire. Field retrofit
  (`_ms: u64` → `timestamp`) deferred.

**Batch II — test depth:**

- **ε.2 Loom** — `#[cfg(loom)]` interleaving tests for `CancelCtx`
  (INV-029). Conditional `[target.'cfg(loom)'.dependencies]`.
  Run explicitly via `RUSTFLAGS="--cfg loom" cargo test`.
- **ε.3 proptest audit** — 14 new properties: TrustLevel lattice
  invariants (meet/join bounds, idempotence, commutativity,
  associativity, absorption); ID serde roundtrip (TenantId,
  ProviderId, ModelId, TaskId, TraceId full 2^128 surface, SpanId
  full 2^64 surface).
- **ε.1 mutation baseline** — `cargo mutants -p nika-error` run:
  60 mutants, 31 caught, 13 missed (mostly miette::Diagnostic
  accessor returns — no observable behaviour), 16 unviable.
  Viable kill rate 70.5%. Pushing to ≥90% requires dedicated
  miette diagnostic-method assertion tests; deferred to a focused
  follow-up session.

**Batch V.2** — `docs/architecture/axes.md`: 12-axis × crate ISP
matrix with shipped/reserved/not-yet markers. Source of truth for
Olympus `/graph/architecture` edge rendering + Gate 12 audits.

**Observability locks (parallel work already landed):**

- Q12 — `ObservabilitySink` dropped (5→4 effect channels);
  `AuditSink` added as compliance-grade 5th channel.
- Q13 — `GenAiAttrs` OTel semconv bridge on Infer{Request,Response}.

**CI ratchets:**

- `cargo-public-api` snapshot workflow (Gate 12 mechanical).
- `cargo-semver-checks` workflow.
- Public-api baseline files regenerated on every reservation commit
  (`--all-features --omit auto-trait-impls` to match CI invocation).

**Forward-compat seams:**

- nika-types `no_std`/`alloc` seam at module level (F1 complete;
  shipped 2026-04-17 morning).
- F2 (full per-module cfg-gating) deferred — requires uuid dep
  re-architecture (currently in `serde` feature but used in
  non-serde struct fields in RunId/EventId/CorrelationId/MemoryId).
  Re-open trigger: uuid becomes unconditional OR UUID-backed IDs
  move to a dedicated feature separate from serde.

**Numbers at close:**

| field              | value                                      |
|--------------------|--------------------------------------------|
| HEAD               | (updated at commit time)                   |
| lib tests          | 905 (+58 this session)                     |
| integration tests  | 10                                         |
| loom tests         | 2 (cfg-gated)                              |
| clippy             | 0 warnings                                 |
| hygiene vectors    | 31 deployed (27 green / 4 yellow)          |
| crates admitted    | 6 + 1 WIP (unchanged)                      |
| ADRs               | 25+ (seeds ADR-029-032 + 035 authored)     |

### ⚡ Phase D Session 4B — Data enrichment (2026-04-16)

Pure data expansion on the structural foundation laid by Session 4A.
Zero trait/struct changes — only enum variants, TOML data, and tests.

- **6 new `ParamFlag` variants** — `BatchApi`, `ContextCaching`,
  `PredictedOutputs`, `ComputerUse`, `Citations`, `IncludeReasoning`.
  Aligned with `OpenRouter` 25-value `supported_parameters` vocabulary.
  Enum: 7→13 variants.
- **3 new `Modality` variants** — `Embedding` (vector output), `Speech`
  (TTS/ASR), `ImageGen` (text-to-image). Covers non-LLM provider
  capabilities. Enum: 5→8 variants.
- **4 new `TokenizerFamily` variants** — `LlamaV4` (~200k vocab, distinct
  from LlamaV3), `Granite` (IBM `StarCoder` BPE), `Glm` (Zhipu
  `SentencePiece`), `Grok` (xAI custom). Enum: 8→12 variants.
- **7 new providers** — nvidia-nim (FIX: inventory discrepancy),
  deepinfra, replicate, hyperbolic, writer, databricks, cloudflare.
  All `openai-chat` dialect. Count: 25→32.
- **7 new capability rules** — one `Matcher::Any` fallback per new
  provider (text-only, `json_schema` where applicable). Count: 42→49.
- `mock-full` rule updated with all 13 `ParamFlag` variants.
- Cross-catalog overlap allowlist: replicate + cloudflare (dual-role).

### ⚡ Phase D Session 4A — Catalog structural enrichment (2026-04-16)

Context-window + output-limit + JSON mode enrichment. First structural
expansion of capabilities beyond the Session 2a/2b foundation.

- **3 new CapPatch fields** — `context_window_tokens: Option<u32>`,
  `max_output_tokens: Option<u32>`, `json_mode: Option<JsonMode>`.
  Per-model context windows and output limits are now expressible in the
  TOML-driven capability resolver.
- **`JsonMode` enum** — `Schema` (tool_use enforcement) / `Object`
  (unstructured json_object mode). Per-provider granularity.
- **`ContainsAny` matcher** — word-boundary-anchored substring matching
  with left/right boundary chars (`-`, `_`, `/`, `.`, `@`). Prevents
  "sonnet-4" from matching "sonnet-4-60" (the `6` after "sonnet-4" is
  not a boundary character).
- **`#[non_exhaustive]` on 20 mock structs** — all `nika-kernel-mock`
  types now enforce invariant #19 (attribute + `pub fn new()`).
- **`HttpStreamResponse::new()`** — invariant #19 compliance for the
  only `#[non_exhaustive]` struct that was missing a constructor.
- **12-field merge_with regression guard** — all CapPatch fields covered
  by a single test with confirmed RED on removal.
- **estimate_cost edge cases** — zero tokens → $0.00, nonexistent model → None.
- **MemoryId deserialize error paths** — missing `mem-` prefix and invalid
  UUID now have dedicated tests.
- Token count: 625 → **630 lib tests** (+5).

### 🛡️ Phase C Wave 3 — Stabilization + review-swarm defense (2026-04-16)

Hardening pass after the foundational-types expansion. Mutation testing,
proptest campaigns, and a 3-agent review swarm closed all P0/P1 findings.

- **Seal `SecretResolver`** — `cargo-expand` verified private supertrait;
  community can't implement, allowing future method additions (P1-1).
- **`CancelCtx` Acquire/Release** — correctness fix for v0.95 DAG cancel
  semantics (P1-6). Drop guard prevents leaked tokens.
- **Reserve NIKA-700..819** + `Category::Memory` / `WasmPlugin` / `Sandbox`
  / `Observability` — error-code real estate for v0.95+ subsystems.
- **Cost stdlib arithmetic** — `Add`/`Sub`/`AddAssign`/`SubAssign` with
  panic-in-debug, wrap-in-release semantics. `checked_add` / `checked_sub`
  for fallible callers.
- **Remove `TrustLevel::Default`** — safe-by-default inversion (P1-2).
  All trust must be explicitly stated.
- **`InferResponse.cost: Option<Cost>`** — structured cost replaces the
  deprecated `cost_usd` float. Provider-side cost tracking now type-safe.
- **Structured `DenialKind`** — replaces `CapabilityDenied { reason: String }`
  with enum variants (`FsReadNotGranted`, `FsWriteNotGranted`, `NetEgressBlocked`,
  `ExecBlocked`, `EnvReadBlocked`, `Custom`).
- **20 proptest lattice/identity laws** — cost commutativity, associativity,
  identity; trust lattice meet/join; baggage merge idempotence (integration tests).
- **MemoryId UUIDv7** — `MemoryId(u128)` → `MemoryId { uuid: Uuid }`.
  Time-sortable, standard format, `Display`/`FromStr` roundtrip.
- **`#[deprecated]` cost_usd** on `InferResponse`, `AgentOutcome`,
  `AgentCheckpoint` + `Cost::to_usd_f64()` bridge for deprecation window.
- **Pin zeroize=1.8** — workspace-wide version lock for `SecretString`.
- **cargo-mutants 88.5% kill rate** on nika-error L0 (cost/trust/baggage).
- Token count: 572 → **585 lib / 621 total** (+13 lib, +49 total).

### ⚡ Phase C Wave 2 — L0 foundational types + L0.5 traits (2026-04-16)

23 pure-data types landed in L0 crates, 6 kernel traits in L0.5, plus
forward-compat seams for v0.95 Cortex and v0.100 WASM.

- **23 L0 value types** across nika-error and nika-kernel — cost, budget,
  trust, retry, schema versioning, baggage, resource URI, content hash,
  memory frame, deny kind, cancel context, plugin DTOs, sandbox policy,
  observability event.
- **6 L0.5 kernel traits** — `IdGenerator`, `SecretResolver`, `MetricsExporter`,
  `TracerProvider`, `EventSink`, `BillingSink`. Sealed: `SecretResolver`,
  `EventSink`, `BillingSink`. Open: `IdGenerator`, `MetricsExporter`,
  `TracerProvider`. All have mock implementations in nika-kernel-mock.
- **Sealing pattern** — `Provider`, `EventSink`, `BillingSink`,
  `SecretResolver` now sealed via `mod sealed { pub trait Sealed {} }`.
  Open traits (`MemoryStore`, `EmbeddingProvider`, `ToolExecutor`) remain
  community-implementable.
- **Forward-compat seams** — `cancel.rs`, `plugin.rs`, `sandbox.rs`,
  `observability.rs` in nika-kernel. `MemoryFrame` gains reserved
  `Option<_>` fields (`cipher`, `provenance`, `retention`, `redactions`).
- **ADRs 016-020** — cancellation, streaming, runtime, retry, WASM
  (Batch F part 1). **ADRs 033-034** — L0/L0.5 expansion plans.
- Token count: 416 → **572** (+156 tests).

### ⚡ Phase D Session 2a — TOML-driven model capabilities (2026-04-14)

Zero-allocation capability resolver migrated from hardcoded Rust to a
TOML-driven rule table. Zero-alloc, proptest-verified, forward-compatible.

- **`data/model-capabilities.toml`** — 9 ordered rules covering OpenAI o-series,
  GPT-5, Claude family, Anthropic catch-all, DeepSeek reasoner, DeepSeek any,
  and xAI Grok-4. Schema `nika/model-capabilities@1.0`. First-match-wins
  semantics with build-time FK checks (providers must exist in
  `llm-providers.toml`, api_dialect must be in the closed dialect set).
- **`src/types/capabilities.rs`** — `CapPatch` (5 `Option<T>` fields,
  `const fn merge_with`, `fn materialize`), `Matcher` (Any/Exact/ExactAny/PrefixAny,
  zero-alloc `eq_ignore_ascii_case`), `Rule` (providers + api_dialect scope + matcher + caps).
- **`build/capabilities.rs`** — extracted from `build.rs` (380 LOC) to stay under
  the 1500-LOC-per-file budget. Validates TOML schema, FK checks, closed-set
  enum validation, all-None rule prevention, emits static Rust arrays at compile time.
- **`api_dialect`** — `Option<&'static str>` added to all 21 providers in
  `llm-providers.toml`. Closed set: anthropic / openai-chat / openai-responses /
  gemini / cohere / ai21 / bedrock / voyage / mock. Reserved for Session 2b+
  dialect-scoped rule authoring.
- **`supports_thinking` → `reasoning` rename** — aligns with 2026 industry
  convention (LiteLLM `supports_reasoning`, models.dev `reasoning`, OpenRouter
  `reasoning`). No compat shim (forever-v0.x nuke-and-rebuild).
- **`TokenLimitParam::MaxOutputTokens`** — variant added (OpenAI Responses API
  future-proofing). No rule maps to it yet; the `#[non_exhaustive]` enum can
  grow without a schema bump.
- **Proptest parity harness** — 10,000 random (provider, model) pairs compared
  against frozen legacy body in `mod parity_tests`. Regex widened to cover slash
  syntax, uppercase, underscore (HF-style), long names.
- **Insta snapshot** — 31 golden (provider, model) pairs reviewable under
  `src/data/snapshots/`.
- **Invariant #19 FULL** — 15 `new()` constructors across the crate (every
  `#[non_exhaustive]` public struct). Includes: `ProviderModel`, `Provider`,
  `ProviderModel`, `McpServer`, `Embedding`, `TransformDef`, `Builtin`,
  `EnvVarSpec`, `McpPackage`, `McpRemote`, `ModelCapabilities`, `ModelPricing`,
  `CostEstimate`, `ParseTagError`, `ParseCategoryError`, `Suggestion`.
- **Gate 8 GREEN** — `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` clean.
  8+ broken intra-doc links fixed across the crate.
- **5-agent review** — rust-architect + rust-pro + rust-perf + spn-nika +
  feature-dev:code-reviewer. All P0/P1 findings addressed in same session
  across 2 hardening commits.

### 🏷️ Phase D Session 1 — Tag vocabulary + Cargo features (2026-04-14)

Typed tag system for catalog entries, Cargo feature gating, and Shield
safety invariant enforcement.

- **42-variant `Tag` enum** (`#[non_exhaustive]`) — model I/O modalities,
  reasoning/generation behaviour, economics, deployment/sovereignty,
  specialisation, domain, and MCP-server permissioning. Kebab-case wire
  format (`Tag::as_str()` + `FromStr`). Locked as enum (not `&str`) so
  pck authors get compile errors on typos.
- **`tags` + `extra_tags` fields** on `Provider`, `McpServer`, `Embedding` —
  `&'static [Tag]` (validated at build time) + `&'static [&'static str]`
  (passthrough escape hatch for community-specific vocabulary).
- **All 139 catalog entries tagged** (21 providers + 13 embeddings + 105 MCP
  servers). build.rs enforces: known tags only, sorted, deduplicated, and
  MCP entries MUST carry exactly one of `read-only` / `destructive` (Shield
  security-filter invariant, compile-time enforced).
- **Cargo features for subset compilation** — `full` (default), `minimal`,
  `mcp`, `providers`, `embeddings`, `pricing`, `capabilities`,
  `builtins-transforms`, `extension-author`. Community crates depend on
  `features = ["extension-author"]` for types-only (no bundled data).
- **7 runtime tag invariant tests** — XOR, Budget/Frontier mutex,
  Embedding/Reranker presence, sort/dedup codegen integrity, spot-checks
  (anthropic tags, stripe MCP tags).
- **COMMUNITY_EXTENSIONS.md** — pck-author pattern documentation for
  `nika-catalog-cn`, `nika-catalog-eu`, etc.
- **3-agent review** (spn-nika + feature-dev + rust-pro) — all P0/P1
  findings addressed: `f64::INFINITY` validation gap, `#[allow(dead_code)]`
  scoping, `tag_variant` drift guard, `Tag::Sandbox` doc clarification,
  `extra_tags` Gate 1 SAFETY note, version pin fix.

### ⚙️ Hygiene + automation (2026-04-14 PM)

Autonomous ecosystem hygiene stack added to prevent drift over the 11-12 month build:

- **15-vector hygiene dashboard** (`scripts/hygiene/check-all.sh`) — MEMORY HEAD,
  crate count, LOC, CHANGELOG, ROADMAP, crate specs, Linear, GitHub milestones,
  org profile, CITATION, unwraps, file LOC cap, Claude coauthor leak, private
  path leak, cargo audit. Green/yellow/red table, exit codes 0/1/2.
- **Claude Code hooks** — PreToolUse blocks 5 dangerous ops (force push,
  `git add -A`, `cargo test --test`, checkout main, `--no-verify`); PostToolUse
  inspects HEAD commit for Claude coauthor + auto-runs hygiene on admissions;
  SessionStart injects grep-verified HEAD + crate count + hygiene state.
- **Skills** — `/gate-check` and `/crate-admit` for 12-gate discipline;
  `review-swarm.md` subagent for parallel 3-agent review (Gate 11).
- **CI workflows** — `hygiene-nightly.yml` (cron 3h UTC, idempotent drift issue),
  `forward-compat.yml` (cargo-public-api + cargo-semver-checks on PR),
  `changelog-cliff.yml` (auto-PR prepend CHANGELOG on tag push).
- **git-cliff config** (`cliff.toml`) — groups match content pipeline.

## [0.80.0-alpha.4] - 2026-04-14

### 🆕 Crate admitted: nika-catalog-verify

The immune system.

Where `nika-catalog` answers "what do we know?" in O(1) from compile-time data,
`nika-catalog-verify` answers "is what we know still true?" It probes real
package registries (npm, PyPI, Docker) and remote MCP endpoints in parallel,
producing a JSON drift report. Binary, not library — runs nightly from CI or
on-demand via `cargo run -p nika-catalog-verify`.

This is the second catalog crate and the first L4 binary admitted. It exists
because static catalogs decay: a package gets deprecated, an API endpoint goes
away, a provider renames a model. Without verify, the catalog silently rots.

Exempted from Gate 5 (mutation ≥90%) because binary I/O code produces
tautological mutations. Gate 10 (legacy parity) is N/A — this is new tooling.

| Metric | Value |
|--------|-------|
| LOC | ~600 |
| Tests | partial (logic only, I/O excluded) |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

Commit `a977e35b1`. 🦋

---

## [Previously Unreleased] — moved to 0.80.0-alpha.4

### 🔨 Refactors

- **nika-catalog Phase C migration** — migrating catalog data from hardcoded
  Rust arrays to `data/*.toml` source files, compiled at build time via
  `build.rs` + `phf_codegen`. Same zero-runtime-overhead phf maps, but the
  source of truth is now human-readable TOML. This unblocks community
  contributions to the catalog (PR a TOML file, not a Rust array).

### 🐛 Fixes

- **nika-catalog Phase A cleanup** (db0bf8e3f) — a 5-agent deep audit
  discovered 29 of our 131 MCP aliases were broken. Some pointed to
  Anthropic reference servers that were quietly deprecated ("Package no
  longer supported" on npm). Others referenced npm packages that never
  existed — Python-only tools, Go binaries, or names we'd fabricated from
  incomplete documentation. Three were community forks with zero weekly
  downloads.

  We removed all 29 and added a regression test (`removed_broken_aliases_not_present`)
  so they can't sneak back. The catalog went from 131 to 102 aliases.
  Every remaining alias now resolves to a real, installable package.

---

## [0.80.0-alpha.3] - 2026-04-13

### 🆕 Crates admitted: nika-kernel + nika-kernel-mock

The nervous system.

`nika-kernel` defines the **trait contracts for every side effect** in Nika.
It sits at L0.5 — above the pure types (error, catalog) and below the
implementations (fs, http, process, provider). Zero implementations live here.
This crate is the constitution: it says what each organ *must* do, not how.

The design follows Interface Segregation Principle to the max: ~20 fine-grained
atomic traits (`FsRead`, `FsWrite`, `HttpGet`, `ShellRun`...) grouped into ~6
super-traits of convenience (`Fs`, `HttpClient`, `ShellExecutor`, `Provider`...).
Consumers depend on exactly the surface they need — a context loader imports
`FsRead` alone, not the entire filesystem umbrella.

All async traits use `trait_variant` (Rust 1.91 native AFIT) instead of
`async_trait`. Zero boxing on the static dispatch path. The kernel carries no
tokio dependency — pure trait definitions that any async runtime can implement.

We also planted the **Cortex + agent-v2 hooks** now: `MemoryStore`,
`EmbeddingProvider`, `ToolExecutor`, `ContextCompressor`, and agent checkpoint
types. These won't be implemented until v0.95, but defining them in Phase 1
means we won't need breaking changes to `#[non_exhaustive]` structs later.
Forward compatibility bought cheaply.

`nika-kernel-mock` is the companion: deterministic mocks for every kernel trait
(`MockClock`, `InMemoryFs`, `MockHttp`, `MockShell`, `MockProvider`...).
Test hermeticity from day one — no test in Nika will ever touch a real
filesystem, a real network, or a real LLM provider.

| Metric | nika-kernel | nika-kernel-mock |
|--------|-------------|------------------|
| LOC | 3,369 | 1,731 |
| Tests | 99 | 88 |
| Mutation killed | 100% | 95.7% |
| Clippy warnings | 0 | 0 |
| Unwraps in src/ | 0 | 0 |

### Key decisions

- **Clock is SYNC, everything else ASYNC** — YAGNI on network time. Hot paths
  stay simple.
- **`BTreeMap` over `HashMap`** — deterministic iteration order, no hasher
  dependency. Tests are reproducible.
- **Cancel as `fn` param, not in struct** — keeps `ShellCommand` free of
  tokio-util. The kernel stays runtime-agnostic.
- **Provider = Infer + Stream + Meta** — all providers MUST stream (even mock).
  Embed and Vision are opt-in traits.
- **Errors per subsystem** — `ProviderError`, `ShellError`, `ToolExecError`,
  `MemoryError`. No god-enum.

All 12 gates passed. Commit `ef8804371`. 🦋

---

## [0.80.0-alpha.2] - 2026-04-13

### 🆕 Crate admitted: nika-catalog

The memory.

`nika-catalog` is Nika's static knowledge of the world: every LLM provider it
can talk to, every MCP server it knows how to install, every builtin tool it
ships, every pipe transform it supports, and the pricing of every model it's
seen.

The catalog is compiled into the binary at build time. No runtime I/O, no
config files, no network calls. You ask "do you know `anthropic`?" and the
answer comes back in O(1) via a [perfect hash function](https://en.wikipedia.org/wiki/Perfect_hash_function).

Why this matters: when a user writes `provider: claude` in their YAML, the
engine resolves the alias → canonical provider → model → capabilities → pricing
in a chain of zero-allocation lookups. No guessing, no fuzzy matching, no
"did you mean?" The catalog is the ground truth.

The lookup strategy is hybrid by design:
- **phf + unicase** for case-insensitive lookups (providers, MCP aliases) —
  because users write `Claude`, `claude`, `CLAUDE` and they all mean Anthropic.
- **Sorted arrays + binary_search** for case-sensitive lookups (builtins,
  transforms) — because `nika:read` and `nika:Read` are different things
  (actually `nika:Read` doesn't exist, and the catalog should say so clearly).

At admission: 16 providers, 105 MCP aliases, 63 builtins, 65 transforms,
61 model pricing entries. All from a single `cargo build`.

| Metric | Value |
|--------|-------|
| LOC | 2,235 |
| Tests | 85 |
| Mutation killed | 94.7% |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

All 12 gates passed. Commit `55a451695`. 🦋

---

## [0.80.0-alpha.1] - 2026-04-13

### 🆕 Crate admitted: nika-error

The DNA.

Every error in Nika carries a code. `NIKA-001` means schema validation failed.
`NIKA-053` means a blocked command was attempted. `NIKA-382` means a canary
token leaked (prompt injection detected). There are hundreds of these codes,
and every single one must roundtrip through Display, parse back from a string,
serialize to JSON, and match the exact same format across every provider, every
verb, every transport layer.

`nika-error` is the crate that makes this possible. It defines:

- **`NikaErrorCode`** — a trait that every per-crate error enum must implement.
  This is the contract: if you want to be a Nika error, you carry a code, a
  severity, a category, and you format yourself as `"NIKA-XXX: message"`.
- **`NikaError`** — a `Box<dyn NikaErrorCode>` wrapper. The unified error type
  that flows through `?` propagation across the entire codebase.
- **`NikaCode`** — the code itself. Dual format: Display gives you `"NIKA-140"`,
  serde gives you `{"num":140,"category":"ast","severity":"error","slug":"ast-analysis-failure"}`.
- **`CoreError`** — cross-cutting errors that don't belong to any specific crate
  (Validation, NotFound, Unsupported, Internal).

This is the L0 anchor. Zero `nika-*` dependencies. Reachable from every crate
in the workspace. The first cell of the organism.

It also resolves **shadow zone 6** from the pre-launch audit: every admitted
`NIKA-XXX` now ships with a Display parity golden test against the legacy
format. No silent drift.

| Metric | Value |
|--------|-------|
| LOC | 1,013 |
| Tests | 44 |
| Mutation killed | 100% |
| Clippy warnings | 0 |
| Unwraps in src/ | 0 |

All 12 gates passed. Commit `42909b1c7`. 🦋

---

## [0.80.0-alpha.0] - 2026-04-13

### The beginning

Orphan branch `nika-diamond` (renamed `main` on 2026-05-06) created from scratch. No code inherited from legacy.
Clean slate, edition 2024, Rust 1.91.

From the start, the workspace enforces:
- `clippy::unwrap_used = "deny"` — zero unwraps, everywhere, always.
- `clippy::panic = "deny"` — if it can panic, it doesn't compile.
- `clippy::expect_used = "warn"` — we'll get there.

32 legacy crate directories excluded via `.gitignore` — they exist on disk
(the orphan branch inherits the working tree) but cargo ignores them. We read
legacy code via `git show main:path/to/file.rs` when we need guidance, but
nothing is copied verbatim. Every line is rewritten.

The organism's skeleton is in place. Now it grows. 🦋

---

[Unreleased]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.4...HEAD
[0.80.0-alpha.4]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.3...v0.80.0-alpha.4
[0.80.0-alpha.3]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.2...v0.80.0-alpha.3
[0.80.0-alpha.2]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.1...v0.80.0-alpha.2
[0.80.0-alpha.1]: https://github.com/supernovae-st/nika/compare/v0.80.0-alpha.0...v0.80.0-alpha.1
[0.80.0-alpha.0]: https://github.com/supernovae-st/nika/commits/v0.80.0-alpha.0
