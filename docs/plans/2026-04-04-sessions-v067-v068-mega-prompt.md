# MEGA PROMPT: Sessions v0.67 + v0.68 → Launch May 5

> **Mode**: Full autonomy, multi-session execution
> **Codebase**: nika v0.66.0 | 391K LOC | 15 crates | 9,869 tests | 61 builtin tools | 50 transforms
> **Philosophy**: v0 = zero dead code, zero backward compat, zero mercy on tech debt
> **Launch**: May 5, 2026 — 31 days from v0.65, ~27 days from now

---

## WHAT'S DONE (v0.65-v0.66 recap, DO NOT REDO)

### v0.65 (2026-04-04 morning)
- `nika:jq` — full jq stdlib, LRU cached compilation (1000x for_each)
- `nika:tree_data` — nested group_by for treemaps
- `nika:inject` — template marker replacement with path validation
- `eval_jq()` public API, `catch_unwind` for jaq regex panics
- Dashboard 8 tabs (1646 lines), workflow 100% native (0 exec, 44 tasks)
- E2E on 6 sites, release v0.65.1

### v0.66 (2026-04-04 evening)
- SEC-1: exec blocklist scans full command (was 4KB)
- SEC-2: shell:true + unescaped bindings → NIKA-053 error (was warning)
- SEC-3: BlockedCommand error redacts command content
- SEC-4: nika:read pre-checks file size (50MB limit)
- data_tools.rs (2074 lines) → split into 6 files under builtin/data/
- nika:json_query deprecated (use nika:jq)
- DX updated: 50 transforms, 61 tools documented everywhere
- AGENTS.md, CLAUDE.md, nika.md, nika-bugs-and-patterns.md all updated
- nika switch improved (version hints, auto-rebuild)
- showcase 08-serve-api fixed (| shell bindings)
- 9,869 tests, 0 failures

---

## SESSION 1: v0.67 ARCHITECTURE (~6h)

### Sprint 0: Quick fixes from v0.66 audit (30 min)

Post-v0.66 audit (8 agents) found these remaining issues:

1. **CRITICAL: security.rs:547** — `check_blocklist_with_intent` stores raw `cmd.to_string()`
   in BlockedCommand error. Every OTHER error path uses `redact_secrets(cmd)`. This one doesn't.
   Fix: `command: crate::util::redact_secrets(cmd).to_string()`

2. **REGEX_CACHE unbounded** — `transform.rs` uses `HashMap` for regex cache (grows forever).
   JQ_FILTER_CACHE uses bounded `lru::LruCache(64)`. Switch REGEX_CACHE to `lru::LruCache(128)`.

3. **5 stale section comments** in data/ split (copy-paste leftover headers pointing at wrong tools):
   - `aggregate.rs:93` says "nika:chunk" (chunk is in text.rs)
   - `aggregate.rs:256` says "nika:inject" (inject is in io.rs)
   - `jq.rs:87` says "nika:map" (map is in transform.rs)
   - `jq.rs:152` says "nika:tree_data" (tree_data is in aggregate.rs)
   - `merge.rs:279` says "nika:json_query" (json_query is in jq.rs)

4. **EnrichTool unnecessary clone** — `transform.rs:208`: `extract_field(&Value::Object(obj.clone()))`
   clones the full map to read a field. Pass by reference instead.

5. **tools/nika/CLAUDE.md** — source tree diagram doesn't show the data/ split. Update.

### Sprint 1: Break engine→init dependency (30 min)

The coupling is SHALLOW — 2 blocks only:
- `nika-engine/src/lib.rs:44` — `pub use nika_init as init;`
- `nika-engine/src/error.rs:972-984` — `From<NikaInitError>`

Zero usage in runtime/DAG/binding/provider. Fix:
1. Remove `nika-init` from `nika-engine/Cargo.toml`
2. Delete the re-export + From impl
3. Add `nika-init = { workspace = true }` to `nika-cli/Cargo.toml`
4. Find-replace `nika_engine::init::` → `nika_init::` in:
   - `nika-cli/src/showcase.rs` (lines 13-17, 93, 210)
   - `nika-cli/src/init.rs` (line 304)
   - `nika-cli/src/course.rs` (lines 13, 579, 583, 741, 877-879)

TDD: `cargo test --workspace --lib --exclude nika-py` must pass.
Commit: `refactor(engine): remove nika-init dependency (embed size -21K LOC)`

### Sprint 2: Extract vault.rs from nika-core (1h)

vault.rs (~500 lines) in nika-core pulls orion, whoami, fs2, secrecy.
Breaks "zero I/O" contract.

Move to `nika-daemon/src/vault.rs`:
1. Copy file, move deps in Cargo.toml
2. Remove `pub mod vault;` from `nika-core/src/lib.rs`
3. Update re-export in `nika-engine/src/secrets/vault.rs`
4. Update 9 consumer files (mechanical find-replace)
5. Verify nika-core compiles WITHOUT orion/whoami/fs2/secrecy

Commit: `refactor(core): extract vault.rs to nika-daemon — core is now pure`

### Sprint 3: jaq 3.x upgrade (2h)

Migration guide exists at `docs/plans/2026-04-04-v067-architecture-handoff.md`.

Dependency changes:
```toml
# REMOVE: jaq-interpret, jaq-parse
# UPDATE: jaq-core 1.5 → 3.0, jaq-std 1.6 → 3.0
# ADD: jaq-json 2.0 (features = ["serde"])
```

Code changes (~90 LOC in transform.rs):
- `ParseCtx::new()` → `Compiler::default()`
- `insert_natives/defs` → `Loader` + `with_funs`
- `jaq_interpret::Val` → `jaq_json::Val`
- `filter.run((ctx, val))` → `filter.id.run((ctx, val))`
- LRU cache: `Filter` may need `Arc` wrapper (check Clone impl)
- Keep `catch_unwind` until verified test("x") on null doesn't panic in 3.x

Test: all 17 existing jq tests must pass unchanged.
Add: `eval_jq("test(\"foo\")", &Value::Null)` → error, not panic.
Commit: `feat(core): upgrade jaq 1.5 → 3.0 — kills regex panic, cleaner API`

### Sprint 3b: Fix InjectTool bugs (30 min)

Two bugs from v0.66 audit — NOT FIXED:

**BUG: missing end_marker silently drops file tail**
In `builtin/data/io.rs:102-120`: if `end_marker` is never found, `skipping` stays true
and all lines after `start_marker` are lost. No error, no warning.
Fix: after the loop, if `skipping` is still true, return an error:
```rust
if skipping {
    return Err(NikaError::BuiltinToolError {
        tool: "nika:inject".into(),
        reason: format!("End marker '{}' not found in template", params.end_marker),
    });
}
```

**BUG: inject test is no-op on macOS**
`inject_basic_replacement` silently swallows errors because tempdir is outside cwd.
Fix: use a subdirectory of the current project as temp, or set cwd in a controlled way.

**Also fix**: Filter test weak assertions (check values, not just len).

### Sprint 4: Error UX — "did you mean?" (1h)

NIKA-071 (UnknownAlias) is the #1 newbie error. v0.66 audit confirmed:
fuzzy matching exists in LSP (code actions for NIKA-140) but NOT in runtime.
The `declared_aliases` set is available at the error site but unused.

Fix: import `find_similar()` from nika-core analyzer into nika-engine's
binding/template.rs. Same for NIKA-080 (WithUnknownTask) — task list is available.

Also: update tools/nika/CLAUDE.md source tree (still shows old layout without data/ split).

Also: add example YAML snippets to top 5 error messages (NIKA-032, 071, 041, 021, 053).

Commit: `fix(engine): add "did you mean?" to UnknownAlias + UnknownTask errors`

### Sprint 5: TUI unwrap audit (1h)

88K LOC, 300 unwrap() calls. Focus on render loop hot paths.
Replace `unwrap()` with `unwrap_or_default()` where user data is involved.
NOT a rewrite — surgical fixes only.

Commit: `fix(tui): replace unwrap with unwrap_or_default in render paths`

### v0.67 Release
- Bump to v0.67.0, CHANGELOG, tag, push
- Verify CI (7 platforms)
- Write handoff for v0.68

---

## SESSION 2: v0.68 LAUNCH PREP (~6h)

### Sprint 1: Quick Win — nika run URL (2h)

```bash
nika run https://raw.githubusercontent.com/supernovae-st/nika-site-audit/main/site-audit.nika.yaml -i "url=https://htmx.org"
```

Implementation:
- In `nika-cli/src/run.rs`: detect URL pattern in workflow path arg
- Download to temp file via reqwest (already a dep)
- Extract referenced files (skills, context) relative to URL base
- Run from temp dir
- Clean up on exit

Test: mock HTTP server returns a valid .nika.yaml, verify execution.

### Sprint 2: Quick Win — nika explain (1h)

```bash
nika explain site-audit.nika.yaml
# → This workflow has 44 tasks across 22 layers.
#   It crawls a website, enriches pages with metadata,
#   and generates an interactive HTML dashboard.
#   Estimated cost: $0.12 (4 LLM calls).
#   Required: OPENAI_API_KEY
```

Implementation: parse YAML → analyzed AST → count tasks/layers/verbs.
The dry-run already does cost estimation. Just format it nicely.

### Sprint 3: GitHub Action (2h)

```yaml
# .github/workflows/audit.yml
name: Site Audit
on: workflow_dispatch
jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: supernovae-st/nika-action@v1
        with:
          workflow: site-audit.nika.yaml
          inputs: "url=https://example.com"
        env:
          OPENAI_API_KEY: ${{ secrets.OPENAI_API_KEY }}
```

Create repo `supernovae-st/nika-action`:
- `action.yml` — composite action
- Downloads nika binary from latest release
- Runs the workflow
- Uploads artifacts

### Sprint 4: Homebrew formula update (30 min)

Update `homebrew-tap/Formula/nika.rb`:
- Version → latest
- SHA256 from release asset
- Test: `brew install --build-from-source supernovae-st/tap/nika`

### Sprint 5: Launch content prep (2h)

1. **Terminal recording** (asciinema or vhs):
   `brew install nika → nika run site-audit → open dashboard`
   60 seconds, shows the full flow.

2. **Comparison image**:
   Python script (47 lines) vs nika YAML (12 lines) — same result side by side

3. **One-liner tweets**:
   - `nika fetch https://arxiv.org/abs/2401.00001 --extract article | nika infer "3 bullet summary"`
   - `nika infer "Explain quantum computing" --provider claude,gpt,gemini --compare`

4. **Blog post draft**: "I replaced 2,000 lines of Python with 40 lines of YAML"

### Sprint 6: nika init verification + polish (1h)

Run `nika init` in a completely fresh directory:
- Does the wizard create nika.toml correctly?
- Does the generated hello.nika.yaml run?
- Is `provider: mock` the default (so it works without API key)?
- Does `nika init --course` start the 12-level course?
- Fix any rough edges found.

### Sprint 7: Final E2E + Feature Freeze (1h)

From a COMPLETELY clean machine (Docker or fresh dir):
```bash
# Download latest release
gh release download --repo supernovae-st/nika --pattern "nika-macos-arm64-*"
# Clone showcase
git clone https://github.com/supernovae-st/nika-site-audit.git
# Run on 3 sites
nika run site-audit.nika.yaml -i "url=https://qrcode-ai.com"
nika run site-audit.nika.yaml -i "url=https://htmx.org"
nika run site-audit.nika.yaml -i "url=https://tailwindcss.com"
# Verify all artifacts
```

FEATURE FREEZE after this. Only bug fixes until May 5.

### v0.68 Release
- Bump to v0.68.0, CHANGELOG, tag, push
- Verify all CI green
- Announce feature freeze

---

## WHAT NOT TO DO (across all sessions)

- No new verbs (5 verbs are sacred)
- No new extract modes (10 is enough)
- No new pipe transforms (50 is plenty)
- No Egghead/memory system (post-launch)
- No TUI redesign (mature, 88K LOC)
- No data verb experiment (stashed, future sprint)
- No nika serve V4 (V3 works)
- No Python/Node SDK work (defer)

---

## COMMIT STRATEGY (all sessions)

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: feat, fix, refactor, docs, test, chore, perf, style
Scopes: engine, core, cli, tui, daemon, mcp, media, event, serve, dx

Git push via HTTPS:
```bash
git remote set-url origin https://github.com/supernovae-st/nika.git
git push
git remote set-url origin git@github.com:supernovae-st/nika.git
```

---

## SKILLS & AGENTS

| Skill | When |
|-------|------|
| `test-driven-development` | All code changes |
| `verification-before-completion` | Before every commit |
| `systematic-debugging` | When tests break |
| `rust` | All Rust code |
| `brainstorming` | Architecture decisions |
| `requesting-code-review` | End of each sprint |

| Agent | When |
|-------|------|
| `rust-pro` | Code review after major changes |
| `rust-security` | Verify security fixes |
| `rust-architect` | jaq 3.x migration |
| `Explore` | Audit DX files for gaps |
| `code-reviewer` | Final review before tag |

---

## VERIFICATION CHECKLIST (before each release)

- [ ] `cargo test --workspace --lib --exclude nika-py` — 0 failures
- [ ] `cargo clippy --workspace -- -D warnings` — 0 warnings
- [ ] `cargo fmt --all --check` — clean
- [ ] `nika check examples/showcase/**/*.nika.yaml` — all valid
- [ ] E2E: fresh dir → nika run → artifacts OK
- [ ] DX files: nika.md, CLAUDE.md, AGENTS.md accurate
- [ ] CHANGELOG entry complete
- [ ] Memory updated

---

## TIMELINE TO LAUNCH

```
v0.66 ✅ DONE (security + clean + DX)
v0.67    NEXT (architecture: engine→init, vault, jaq 3.x, error UX, TUI)  ~6h
v0.68    THEN (launch prep: run URL, explain, GH Action, content, freeze)  ~6h
─────────────────────────────────────────
FEATURE FREEZE
Bug fixes only → May 5 LAUNCH 🚀
```
