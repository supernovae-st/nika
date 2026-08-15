# scripts/

Operational scripts for Nika Diamond, organized by responsibility.
One folder = one concern. Keep it this way.

## Layout

```
scripts/
├── adr/          ADR DX scripts — schema validation, index generation, scaffolding
├── ci/           CI ratchets — LOC caps, unwrap scan, clippy, tests, etc.
├── hooks/        Git hook scripts (lefthook integration)
└── hygiene/      31 drift vectors dashboard — keeps ecosystem in sync
```

## ci/ — CI enforcement

Runs on every push via `.github/workflows/diamond-ci.yml`. Each script is
a single-purpose ratchet. See `ci/` for the full list.

| Script | Rule |
|---|---|
| `check-loc-limits.sh` | file ≤ 1,500 LOC |
| `check-crate-size.sh` | crate ≤ 15,000 LOC |
| `check-fn-length.sh` | fn ≤ 100 lines |
| `check-unwrap.sh` | 0 `.unwrap()` in src/ |
| `check-expect.sh` | 0 `.expect(` in src/ |
| `check-dead-code.sh` | 0 `#[allow(dead_code)]` |
| `check-clippy.sh` | `cargo clippy -D warnings` |
| `check-tests.sh` | `cargo test --workspace --lib` |
| `check-no-default-features.sh` | compiles with no default features |

## adr/ — ADR DX tooling

Schema validation, index generation, and scaffolding for Architecture Decision Records.

```bash
bash scripts/adr/generate-index.sh   # regen index.toml + index.json from frontmatter
bash scripts/adr/validate.sh         # schema + cycles + dangling refs
bash scripts/adr/new.sh "Title"      # scaffold next ADR from template
```

## hygiene/ — drift detection dashboard

The autonomous ecosystem hygiene system. 31 drift vectors. Runs locally + in
CI nightly. Opens an idempotent issue on the repo if anything is RED.

```bash
bash scripts/hygiene/check-all.sh           # full table (colored)
bash scripts/hygiene/check-all.sh --quiet   # silent unless yellow/red
bash scripts/hygiene/check-all.sh --format=json
```

Vectors:

| # | Vector | What it catches |
|---|---|---|
| 1 | memory-head-sha | MEMORY.md HEAD out of sync with `git rev-parse HEAD` |
| 2 | crate-count | `ls crates/*/Cargo.toml` ≠ MEMORY's count |
| 3 | loc-totals | Per-crate LOC drift |
| 4 | changelog-dates | CHANGELOG top entry date sanity |
| 5 | roadmap-crate-status | ROADMAP mentions match admitted crates |
| 6 | crate-spec-metrics | `docs/crate-specs/nika-X.md` frontmatter vs reality |
| 7 | linear-issue-states | Linear issue states match `git log` admissions |
| 8 | gh-milestones | GitHub milestone completion % |
| 9 | org-profile-repos | `supernovae-st/.github/profile/README.md` mentions all canonical repos |
| 10 | citation-version | CITATION.cff version matches workspace |
| 11 | unwraps-in-src | Zero `.unwrap()`/`.expect(` outside tests |
| 12 | file-loc-cap | No src/*.rs file > 1,500 LOC |
| 13 | claude-coauthor-leak | No `Co-Authored-By: Claude` on diamond branch |
| 14 | private-path-leak | No private monorepo path in tracked code · same patterns as the pre-commit hook (`scripts/lib/private-patterns.sh`) · frozen ADRs exempt |
| 15 | cargo-audit-rustsec | RustSec advisories |

## Naming convention

All scripts use lowercase-dashed names, Bash shebang, `set -euo pipefail` at
top, and exit codes `0` (success) / `1` (warn/yellow) / `2` (fail/red).

## See also

- [`.github/workflows/`](../.github/workflows/) — CI pipelines that invoke these scripts
- [`../ROADMAP.md`](../ROADMAP.md) — forever-v0.x plan these scripts guard
- [`cliff.toml`](../cliff.toml) — git-cliff config for auto-CHANGELOG

🦋
