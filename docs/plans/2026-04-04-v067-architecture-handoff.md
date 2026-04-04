# v0.67 Architecture Sprint — Pre-Launch Handoff

> Date: 2026-04-04 | Post v0.66 stabilization | Before May 5 launch
> Based on: 15 specialized agent audits across the day

---

## Sprint 1: engine→init inversion (30 min, LOW risk)

Coupling is SHALLOW — only 2 code blocks:
- `lib.rs:44` — `pub use nika_init as init;` (convenience re-export)
- `error.rs:972-984` — `From<NikaInitError>` (error conversion)

Zero usage of nika-init in runtime/DAG/binding/provider code.

**Fix**:
1. Remove `nika-init` from `nika-engine/Cargo.toml:48`
2. Remove re-export from `nika-engine/src/lib.rs:44`
3. Remove `From<NikaInitError>` from `nika-engine/src/error.rs:972-984`
4. Add `nika-init = { workspace = true }` to `nika-cli/Cargo.toml`
5. Find-replace `nika_engine::init::` → `nika_init::` in 3 CLI files:
   - `nika-cli/src/showcase.rs` (lines 13-17, 93, 210)
   - `nika-cli/src/init.rs` (line 304)
   - `nika-cli/src/course.rs` (lines 13, 579, 583, 741, 877-879)

TUI not affected. Zero behavioral change.

---

## Sprint 2: vault.rs extraction (1h, LOW risk)

vault.rs (~500 lines) in nika-core pulls: orion, whoami, fs2, secrecy.
Breaks the "zero I/O" contract.

**Move to**: `nika-daemon/src/vault.rs` (daemon is the authoritative write path).

**Consumers** (35 refs across 5 crates):
- nika-engine: thin re-export `pub use nika_core::vault::*` → redirect to daemon
- nika-daemon: primary write consumer (already there)
- nika-cli: `provider.rs`, `vault.rs`, `onboarding.rs`
- nika-tui: `provider_modal/tabs/keys.rs`, `views/chat/keys.rs`

**Steps**:
1. Copy `nika-core/src/vault.rs` → `nika-daemon/src/vault.rs`
2. Move deps (orion, whoami, fs2, secrecy) from nika-core to nika-daemon Cargo.toml
3. Remove `pub mod vault;` from `nika-core/src/lib.rs:27`
4. Update `nika-engine/src/secrets/vault.rs` re-export: `nika_core::vault::*` → `nika_daemon::vault::*`
5. Update 9 source files with import path changes (mechanical find-replace)
6. Verify nika-core compiles WITHOUT orion/whoami/fs2/secrecy

---

## Sprint 3: jaq 3.x migration (2h, MEDIUM risk)

### Why
- jaq-core 1.5.x `test()` panics on null (our catch_unwind is duct tape)
- jaq 3.0.0 returns `Exn(Err(...))` instead of panicking
- Better error types, faster compiled filter graph, extensible `DataT` trait

### Dependency changes
```toml
# REMOVE:
jaq-interpret = "1.5"
jaq-parse = "1.0"
jaq-core = "1.5"
jaq-std = "1.6"

# ADD:
jaq-core = "3.0"
jaq-std = "3.0"
jaq-json = { version = "2.0", features = ["serde"] }
```

### Code changes (~90 LOC in transform.rs)

**compile_jq** — complete rewrite:
- `ParseCtx::new()` → `Compiler::default()`
- `insert_natives(jaq_core::core())` → `.with_funs(funs)`
- `insert_defs(jaq_std::std())` → `Loader::new(defs)`
- `jaq_parse::parse(expr)` → `Arena` + `Loader` + `File` pattern
- LRU cache: `Filter` might need `Arc` wrapper (check if Clone impl exists)

**eval_jq** — execution path rewrite:
- `jaq_interpret::Val::from(data)` → `jaq_json::Val::from(data)`
- `filter.run((ctx, val))` → `filter.id.run((ctx, val))`
- `Ctx::new([], &inputs)` → `Ctx::new(&filter.lut, Vars::default())`
- `Value::from(val)` → `serde_json::to_value(&val)`
- `catch_unwind` → KEEP until verified by test (one agent says fixed, one says keep)

### Testing strategy
1. All 17 existing jq tests must pass unchanged
2. Add regression test: `eval_jq("test(\"foo\")", &Value::Null)` → error, not panic
3. Verify LRU cache still works with new Filter type
4. Run full engine test suite

---

## Sprint 4: TUI hardening (2h, LOW risk)

TUI is MATURE (88K LOC, 138 test blocks, 3 polished views).
Risk areas: 300 unwrap() + 243 clone() calls.

**Focus**: audit unwrap() calls in render loops (can cause panic on edge case data).
Replace with `.unwrap_or_default()` or proper error handling where user data is involved.

NOT a rewrite — targeted fixes in hot render paths only.

---

## BONUS: Quick Win Features for Launch

### nika run URL (2h, HIGH impact)
```bash
nika run https://nika.dev/examples/site-audit.nika.yaml -i "url=https://htmx.org"
```
Download + execute workflow from URL. Like `npx` for AI workflows. Viral for demos.

### nika explain (1h, HIGH impact)
```bash
nika explain site-audit.nika.yaml
# → "This workflow crawls a website (44 tasks, ~$0.12/run).
#    It extracts pages, detects locales, validates hreflang,
#    and generates an interactive HTML dashboard."
```
Parse YAML → human-readable summary. Uses the existing AST analyzer.

### GitHub Action (3h, HIGH impact)
```yaml
- uses: supernovae-st/nika-action@v1
  with:
    workflow: site-audit.nika.yaml
    input-url: ${{ github.event.inputs.url }}
```
Nika in CI/CD natively. Pre-built Docker image.

---

## Launch Content Ideas (from killer features agent)

### Viral one-liners
- "7 providers, 1 prompt, who wins?" — fan-out comparison workflow
- "Replace your Python script" — 47 lines Python vs 12 lines YAML, same result
- "Blog post from any URL in 8 seconds" — fetch + infer pipe demo

### Blog post titles
- "I replaced 2,000 lines of Python with 40 lines of YAML"
- "Structured output is a solved problem (and your framework doesn't know it)"
- "The 5 verbs that replace every AI SDK"

### Moat features to highlight
1. **Structured output 5-layer defense** — works identically on all 7 providers
2. **Binary media pipeline** — images, audio, PDFs, QR, C2PA in workflow YAML
3. **`provider: mock`** — deterministic testing without API keys in CI

---

## Execution Timeline

```
v0.66 (stabilization)      ━━━━━━━━━━━━━ ~10h  ← NEXT SESSION
v0.67 (architecture)        ━━━━━━━━ ~6h
  Sprint 1: engine→init     ━━ 30min
  Sprint 2: vault extraction ━━━ 1h
  Sprint 3: jaq 3.x          ━━━━ 2h
  Sprint 4: TUI hardening    ━━━━ 2h
v0.68 (launch prep)          ━━━━ ~4h
  Quick wins (run URL, explain, GH Action)
  Content (demo GIF, blog draft, landing page)
  Feature freeze
─────────────────────────────────────────
MAY 5 → LAUNCH 🚀
```
