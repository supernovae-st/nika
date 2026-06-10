# scripts/hygiene/ — drift detection dashboard

31 drift vectors that keep Nika's ecosystem in sync over 11-12 months of
building in public. Zero maintenance required. Runs locally on every
commit (via Claude Code PostToolUse hook) and nightly via GitHub Action.

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

## The 30 vectors

Each vector is a single `check-*.sh` script. Single responsibility.
Exits `0`/`1`/`2` to signal green/yellow/red.

| # | Script | Detects |
|---|---|---|
| 1 | `check-memory-head.sh` | MEMORY.md HEAD SHA ≠ actual `git rev-parse HEAD` |
| 2 | `check-crate-count.sh` | Workspace `members = [...]` count = `[workspace.metadata.diamond.layers.*]` count (every crate must be layer-classified) |
| 3 | `check-loc.sh` | src LOC drift (> 2% = yellow, > 5% = red) |
| 4 | `check-changelog-dates.sh` | CHANGELOG top entry date reasonable (not future, not > 14 days old without commit) |
| 5 | `check-roadmap-status.sh` | ROADMAP checkboxes align with admitted crates |
| 6 | `check-crate-specs.sh` | Every admitted crate has `docs/crate-specs/nika-X.md` · live-anchored `~NNN LOC src (live · …)` numbers stay within ±15% of `scripts/crate-metrics.sh` (no hardcoded drift) |
| 7 | *(killed 2026-04-17)* | was `check-linear.sh` — no-op stub without `LINEAR_API_KEY`, misleading green. Linear integration lives in its own MCP, not hygiene |
| 8 | `check-milestones.sh` | GitHub milestone progress sanity |
| 9 | `check-org-readme.sh` | Org profile README mentions all 6 canonical repos |
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
| 30 | `check-cancel-safety.sh` | Every `async fn` in `crates/nika-kernel/src/**` has `// CANCEL SAFETY:` or `/// CANCEL SAFETY:` marker in preceding doc block (Batch I.b — kernel effect surface must document drop-safety per method) |
| 31 | `check-owned-strings.sh` | nika-catalog public API uses `&'static str` (ADR-008 codegen pragma) or owned `String` — bans non-static `&str`/`&'a str` in `pub` fields and `pub fn` return types. Allow `&str` in parameters. Per-item exempt via `// OWNED-STRINGS-EXEMPT: <reason>` |
| 32 | `check-unsafe-count.sh` | `unsafe` token count in `crates/*/src/**/*.rs` ≤ baseline (see `baselines/unsafe-count.txt`). Substitutes cargo-geiger-workspace which is hostile to virtual manifests; dep-tree security still covered by `cargo audit` + `cargo deny`. Baseline currently 0 |
| 33 | `check-layer-deps.sh` | Per-layer banned third-party deps — L0 rejects tokio/futures/reqwest/hyper/rayon/async-std/smol/axum/actix-web (17 deps); L0.5 rejects the same minus `futures*` (traits use `trait_variant` + `std::future`, 11 deps). Bans table lives in `[workspace.metadata.diamond] layer-bans.<layer>` in Cargo.toml. Per-line exempt via `# LAYER-BAN-EXEMPT: <reason>` |

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

## Olympus dashboard auto-refresh (post-commit)

After every engine commit, lefthook fires
`scripts/hooks/post-commit-olympus-xtask.sh` in the background (nohup +
`pnpm tsx olympus/scripts/xtask.ts`). This regenerates
`olympus/data/workspace.json` + `data/snapshots/<timestamp>.json`
+ `data/hygiene-status.json`, which the Olympus file-watcher picks up
via `WorkspacePatchKind` so `/timeline`, `/graph/diff`, `/graph/fitness`,
and `/hygiene` all refresh live without manual reload.

The hook is non-blocking: commits always succeed. Missing pnpm or a
missing olympus sibling directory causes a silent skip logged to
`.nika/post-commit-xtask.log`. The log is gitignored via the root
`/.nika/` entry.

## Integration with Claude Code hooks

`.claude/settings.json` PostToolUse hook auto-runs this dashboard after any
commit matching `admit to workspace`. Silent on green; reports yellow/red
inline.

## Performance

Each script has a 30s timeout wrapper (via `timeout` or `gtimeout` on macOS).
Full dashboard completes in ~5s on warm cache. Slowest: `check-cargo-audit.sh`
(network) and `check-linear.sh` (API).

🦋
