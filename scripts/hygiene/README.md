# scripts/hygiene/ — drift detection dashboard

20 drift vectors that keep Nika's ecosystem in sync over 11-12 months of
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

## The 20 vectors

Each vector is a single `check-*.sh` script. Single responsibility.
Exits `0`/`1`/`2` to signal green/yellow/red.

| # | Script | Detects |
|---|---|---|
| 1 | `check-memory-head.sh` | MEMORY.md HEAD SHA ≠ actual `git rev-parse HEAD` |
| 2 | `check-crate-count.sh` | `ls crates/*/Cargo.toml` ≠ MEMORY's recorded count |
| 3 | `check-loc.sh` | src LOC drift (> 2% = yellow, > 5% = red) |
| 4 | `check-changelog-dates.sh` | CHANGELOG top entry date reasonable (not future, not > 14 days old without commit) |
| 5 | `check-roadmap-status.sh` | ROADMAP checkboxes align with admitted crates |
| 6 | `check-crate-specs.sh` | Every admitted crate has `docs/crate-specs/nika-X.md` |
| 7 | `check-linear.sh` | Linear issue states match `git log` admissions (needs `LINEAR_API_KEY`) |
| 8 | `check-milestones.sh` | GitHub milestone progress sanity |
| 9 | `check-org-readme.sh` | Org profile README mentions all 6 canonical repos |
| 10 | `check-citation.sh` | CITATION.cff version ↔ workspace consistency |
| 11 | `check-unwraps.sh` | Zero `.unwrap()` / `.expect(` outside tests |
| 12 | `check-file-loc.sh` | No src/*.rs file > 1,500 LOC cap |
| 13 | `check-claude-coauthor.sh` | No `Co-Authored-By: Claude` on diamond branch |
| 14 | `check-private-leaks.sh` | No `/.claude/projects/…` in tracked code |
| 15 | `check-cargo-audit.sh` | `cargo audit` shows no RustSec advisories |
| 16 | `check-adr-schema.sh` | ADR frontmatter missing required fields or invalid format |
| 17 | `check-adr-cycles.sh` | Supersession cycle in ADR graph (A supersedes B supersedes A) |
| 18 | `check-adr-dangling.sh` | Reference to non-existent ADR ID in frontmatter |
| 19 | `check-adr-orphan-proposed.sh` | ADR stuck in proposed/draft >30 days |
| 20 | `check-adr-evidence.sh` | File path in Evidence section no longer exists |
| 21 | `check-layering.sh` | Diamond layer discipline — wrapper around `scripts/ci/check-layering.sh` (cross-layer upward deps) |
| 22 | `check-no-async-in-l0.sh` | L0 crates with `async fn`, `.await`, tokio/futures/async-trait imports (Q1 lock 2026-04-16) |

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

## Integration with Claude Code hooks

`.claude/settings.json` PostToolUse hook auto-runs this dashboard after any
commit matching `admit to workspace`. Silent on green; reports yellow/red
inline.

## Performance

Each script has a 30s timeout wrapper (via `timeout` or `gtimeout` on macOS).
Full dashboard completes in ~5s on warm cache. Slowest: `check-cargo-audit.sh`
(network) and `check-linear.sh` (API).

🦋
