# scripts/hygiene/ — drift detection dashboard

Drift vectors that keep Nika's ecosystem in sync over 11-12 months of
building in public. Zero maintenance required. Runs locally on every
commit (via Claude Code PostToolUse hook) and nightly via GitHub Action.

The count is not written here. It said 37 while `check-all.sh` said 38
and the file carried 46 `run_check` calls — three surfaces, three
numbers. It derives:

```bash
grep -c '^run_check ' check-all.sh
```

and `./check-all.sh` prints the tally it actually ran.

## Quick start

```bash
# Full dashboard with colors
./check-all.sh

# Silent unless something is RED
./check-all.sh --quiet

# JSON for programmatic consumption (e.g., nika.sh/stats)
./check-all.sh --format=json
```

Exit codes: `0` = all green, `1` = at least one yellow, `2` = at least one red.

## The vectors

Each vector is a single `check-*.sh` script. Single responsibility.
Exits `0`/`1`/`2` to signal green/yellow/red. Rows marked *(killed …)*
keep their number: renumbering is churn for no value.

| # | Script | Detects |
|---|---|---|
| 1 | `check-memory-head.sh` | MEMORY.md HEAD SHA ≠ actual `git rev-parse HEAD` |
| 2 | `check-crate-count.sh` | Workspace `members = [...]` count = `[workspace.metadata.diamond.layers.*]` count (every crate must be layer-classified) |
| 3 | `check-loc.sh` | src LOC drift (> 2% = yellow, > 5% = red) |
| 4 | `check-changelog-dates.sh` | CHANGELOG top entry date reasonable (not future, not > 14 days old without commit) |
| 5 | *(killed 2026-08-14)* | was `check-roadmap-status.sh` — grepped ROADMAP.md for `- [ ] <crate>`, a syntax that has **never** existed in that file (`git log -S` over 171 commits: zero). No repo state could make it fire, so it reported OK on every pre-push and nightly run: one of the greens was unearned. The parity it might have been re-aimed at — the ROADMAP census vs `Cargo.toml`'s `wip` list — is already enforced by vector 23, proven by mutation (drop a crate from `wip = [...]`: 23 goes RED, 5 still said "OK (roadmap consistent)"). Kept gap in numbering |
| 6 | `check-crate-specs.sh` | Every admitted crate has `docs/crate-specs/nika-X.md` · live-anchored `~NNN LOC src (live · …)` numbers stay within ±15% of `scripts/crate-metrics.sh` (no hardcoded drift) |
| 7 | *(killed 2026-04-17)* | was `check-linear.sh` — no-op stub without `LINEAR_API_KEY`, misleading green. Linear integration lives in its own MCP, not hygiene |
| 8 | `check-milestones.sh` | GitHub milestone progress sanity |
| 9 | `check-org-readme.sh` | Org profile README mentions every canonical public repo (whole-word, so a longer sibling's name cannot cover a missing one) · and the counts it quotes match `canon.yaml`. YELLOW when canon.yaml is unreachable — the parity is then unmeasured, and the verdict says so rather than asserting it. The count derives from the list in the script; it was written here as "6" while the script carried 13 |
| 10 | `check-license.sh` | LICENSE file present + AGPL-3.0-or-later (renamed from `check-citation.sh` 2026-04-16; name was misleading — never checked CITATION.cff which doesn't exist) |
| 11 | `check-unwraps.sh` | Zero `.unwrap()` / `.expect(` outside tests |
| 12 | `check-file-loc.sh` | Three-tier file-LOC discipline (ADR-023): 800 YELLOW / 1500 RED / 3000 CRITICAL with `// LOC-EXEMPT: <reason>` marker (codegen, lookup-table, enum-mega) |
| 13 | `check-claude-coauthor.sh` | No `Co-Authored-By: Claude` on diamond branch |
| 14 | `check-private-leaks.sh` | No `/.claude/projects/…` in tracked code |
| 15 | `check-cargo-audit.sh` | `cargo audit` shows no RustSec advisories |
| 16 | `check-adr-schema.sh` | ADR frontmatter missing required fields or invalid format |
| 17 | *(killed 2026-05-30)* | was `check-adr-cycles.sh` — subsumed by vector 16 `check-adr-schema.sh` → `validate.sh` Pass 3 (DAG supersession-cycle detection, bash-3.2-safe worklist). The dedicated vector used `declare -A` (bash 4+); validate.sh Pass 3 is portable + self-contained. Kept gap in numbering |
| 18 | *(killed 2026-04-17)* | was `check-adr-dangling.sh` — duplicated by vector 16 `check-adr-schema.sh` → `validate.sh` Pass 2 which already runs dangling-ref check. Kept gap in numbering (renumbering 33 vectors is churn for no value) |
| 19 | `check-adr-orphan-proposed.sh` | ADR stuck in proposed/draft >30 days |
| 20 | `check-adr-evidence.sh` | File path in Evidence section no longer exists |
| 21 | `check-layering.sh` | Diamond layer discipline — wrapper around `scripts/ci/check-layering.sh` (cross-layer upward deps) |
| 22 | `check-no-async-in-l0.sh` | L0 crates with `async fn`, `.await`, tokio/futures/async-trait imports (Q1 lock 2026-04-16) |
| 23 | `check-status-claims-sync.sh` | Whitelisted status docs (ROADMAP, CLAUDE) embed canonical `refresh-status.sh` block verbatim (structural fields) |
| 24 | `check-crate-size.sh` | Per-crate LOC ratchet — every workspace member ≤ 15,000 LOC (Diamond invariant; was CI-only, now in hygiene dashboard) |
| 25 | `check-l0-dep-fanout.sh` | Each L0 crate ≤ 3 sibling `nika-*` deps (ADR-027 §"Hard rule"); per-crate exempt via `# L0-DEP-FANOUT-EXEMPT: <reason>` marker |
| 30 | `check-cancel-safety.sh` | Every `async fn` in the **L0.5 layer** has a `// CANCEL SAFETY:` / `/// CANCEL SAFETY:` marker, in the preceding doc block **or on the declaring trait** (Batch I.b — kernel effect surface must document drop-safety per method). Scope DERIVES from `[workspace.metadata.diamond] layers.* = "L0.5"`; it read `crates/nika-kernel/src/**` until 2026-08-14, which the kernel split had emptied, so the vector guarded 0 of 94 async fn while reporting OK. `*-mock` crates are excluded and the exclusion is printed |
| 31 | `check-owned-strings.sh` | nika-catalog public API uses `&'static str` (ADR-008 codegen pragma) or owned `String` — bans non-static `&str`/`&'a str` in `pub` fields and `pub fn` return types. Allow `&str` in parameters. Per-item exempt via `// OWNED-STRINGS-EXEMPT: <reason>` |
| 32 | `check-unsafe-count.sh` | `unsafe` token count in `crates/*/src/**/*.rs` ≤ baseline (see `baselines/unsafe-count.txt`). Substitutes cargo-geiger-workspace which is hostile to virtual manifests; dep-tree security still covered by `cargo audit` + `cargo deny`. Baseline currently 0 |
| 33 | `check-layer-deps.sh` | Per-layer banned third-party deps — L0 rejects tokio/futures/reqwest/hyper/rayon/async-std/smol/axum/actix-web (17 deps); L0.5 rejects the same minus `futures*` (traits use `trait_variant` + `std::future`, 11 deps). Bans table lives in `[workspace.metadata.diamond] layer-bans.<layer>` in Cargo.toml. Per-line exempt via `# LAYER-BAN-EXEMPT: <reason>` |
| 34 | `check-cargo-deny.sh` | Supply-chain POLICY via `cargo deny check` — superset of vector 15: advisories + bans (banned/duplicate crates) + licenses (SPDX/AGPL-compat allowlist) + sources (trusted registries). Config: `deny.toml`. Sovereignty Rule 1 / SLSA posture (added 2026-06-10) |
| 35 | `check-adr-081-guards.sh` | ADR-081 computer-use guard-presence admission gate — for every MANDATORY guard in the ADR-081 ownership matrix whose owner-crate is a workspace member, an impl-binding (`scripts/ci/adr-081-guard-manifest.tsv`) + its impl/test markers MUST exist. Declarative/evolutive: a guard-owner admitted without its guard ⇒ RED (the security forcing-function for nika-input M2.4 et al.). yellow = guards owed at future admission (added 2026-06-10) |
| 36 | `check-unused-deps.sh` | Unused `[dependencies]` via `cargo machete`, workspace-member-scoped (excluded legacy crates ignored). Dep-rot inflates the supply-chain audit surface (added 2026-06-10) |
| 37 | `check-error-one-voice.sh` | "Error one-voice" doctrine enforcement — every thiserror error enum (one with `#[error(...)]` variant attrs) in an admitted crate's non-test src MUST impl `NikaErrorCode` (central registry codes) OR be in the documented exemption allowlist (`scripts/ci/error-one-voice-allowlist.tsv`, projected from `docs/architecture/error-trait-completeness-2026-06-10.md`). A new error enum that skips the canonical trait ⇒ RED. Orphan-allowlist check keeps the SSOT honest (added 2026-06-10) |
| 38 | `check-public-api-coverage.sh` | public-API surface coverage ratchet — every admitted crate with a lib target SHOULD carry a `crates/<c>/public-api.txt` lock (diffed by public-api.yml, classified by semver-checks.yml). Both CI workflows run on a HARDCODED 5-crate list (ADR-090 anti-pattern); this derives the should-be-covered set from workspace members. Floor = `scripts/ci/public-api-coverage-baseline.txt` (monotonic): a floor crate losing its lock = RED; uncovered lib crates = YELLOW ratchet target (5/17 today). (added 2026-06-10) |
| 39 | `check-gate5-attestation.sh` | Gate-5 (mutation) attestation — the FAST structural complement to the slow `scripts/ci/check-mutation-floor.sh` (admission/nightly tier). An admitted crate has passed all 12 gates, so its Gate-5 spec row must read a measured `✅` score OR carry a `GATE5-EXEMPT: N` marker — never `⏳ deferred/pending`. Parses each admitted lib crate's Gate-5 table row; YELLOW lists any that still DEFER mutation (closes the ADR-090 gap where Gate 5 was enforced socially — check-mutation-floor was wired into no run). (added 2026-06-10) |
| 40 | `check-kernel-io-typed-errors.sh` | Kernel I/O effect-traits must return a TYPED error (`Result<T, XError>` · #[non_exhaustive] + NikaErrorCode), NEVER `std::io::Result` — a typed error carries the NIKA taxonomy across the trait boundary; io::Result erases it (FCI-023bis · the canonical "type it, always" rule). Scans `nika-kernel-core/src/io/*.rs`: a NEW un-typed io trait ⇒ RED; the computer-use laggards mid-migration (`scripts/ci/kernel-io-typed-error-baseline.txt`) ⇒ YELLOW ratchet; all typed ⇒ GREEN. blob/fs/http/process comply; screen/ocr/a11y/input/browser migrating. (added 2026-06-10) |

## Adding a new vector

1. Create `check-<slug>.sh` — single-purpose bash script
2. Exit `0` on green, `1` on yellow (< 2% drift or < 48h lag), `2` on red (hard divergence)
3. Output ≤ 1 line describing the finding
4. Add `run_check` call to `check-all.sh`
5. Test: `./check-all.sh` should show your new line

## Integration with CI

`.github/workflows/hygiene-nightly.yml` runs this at 3 AM UTC daily:

- All green → close any existing drift issue
- Any red → open (or update) 1 idempotent issue with label `hygiene-drift`
  containing the full output

## Olympus dashboard auto-refresh (post-commit) — RETIRED 2026-08-14

There was a `post-commit-olympus-xtask` hook here that regenerated an
Olympus dashboard after every engine commit. It is gone.

It resolved its target as `<engine>/../../olympus`, which the tree moved
out from under: that path names `…/repos/olympus`, and Olympus lives at
`ventures/olympus`. The hook opened with `[ -d "$OLYMPUS" ] || exit 0`
placed ABOVE its own log write, so once the directory was gone it left
no trace at all — it did not even create `.nika/`.

Its own log is the record: 190 fires ever, the last at
`2026-06-05T15:05:13Z`, ending in `ERR_MODULE_NOT_FOUND`. **1722
commits since, in silence.**

Repair was not available. The receiving `scripts/xtask.ts` still exists
in the Olympus OS repo, but that repo has no `package.json`, so the
`pnpm tsx` invocation cannot resolve; its output `data/workspace.json`
is not tracked anywhere; and `olympus studio health` has since taken
over the job. Re-pointing the path would only have reached a script
that cannot run.

## Integration with Claude Code hooks

`.claude/settings.json` PostToolUse hook auto-runs this dashboard after any
commit matching `admit to workspace`. Silent on green; reports yellow/red
inline.

## Performance

Each script has a 30s timeout wrapper (via `timeout` or `gtimeout` on macOS).
Full dashboard completes in ~5s on warm cache. Slowest: `check-cargo-audit.sh`
(network) and `check-linear.sh` (API).

🦋
