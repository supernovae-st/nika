# Contributing to Nika

Welcome. Nika is a Rust workflow engine for AI, built as a **diamond
rewrite toward a 1.0 launch** (amended D-2026-06-20-N1): layered crates
(the count is projected, never a gate — ADR-037 horizon 50-90 · cap 100 ·
ruled D-2026-07-21-N1), each passing a 12-gate admission checklist before joining the workspace.
This document explains how to contribute.

See also: [`README.md`](README.md), [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md),
[`SECURITY.md`](SECURITY.md), [`AGENTS.md`](AGENTS.md),
[`DIAMOND.md`](DIAMOND.md) (strategy overview),
[`ROADMAP.md`](ROADMAP.md) (real-semver plan toward 1.0),
[`docs/architecture/BLUEPRINT_2036.md`](docs/architecture/BLUEPRINT_2036.md)
(10-year horizon canon · v1.3 · ADR-037 count horizon 50-90 · cap 100 · 11/10 amplifiers).

---

## Current phase

Nika is being crafted in public on the `main` orphan branch (production
Diamond · renamed 2026-05-06 from `nika-diamond`). Legacy v0.79.3 lives
on `brouillon` (read-only · accessed via `git show brouillon:path`).
External code contributions are formally welcomed once the first public
launch (**1.0.0**) ships and all 7 pre-launch shadow zones are green
(amended D-2026-06-20-N1). Until then:

- **Issues, bug reports, discussion** — always welcome, now.
- **Documentation fixes, typos, small DX improvements** — welcome via PR.
- **New crate admissions, new features, new ADRs** — author-only for now.

If you are unsure whether something is in scope, open an issue first.

---

## Philosophy

- **Real semver toward 1.0** (amended D-2026-06-20-N1). The engine is at 0.91.0 (latest release · main on 0.92.0-dev · release-candidate grade); the first public launch ships as 1.0.0, then 1.x minors add the remaining crates additively toward the ADR-037 count horizon (50-90 · cap 100 · projected, never a gate · ruled D-2026-07-21-N1). Each version is diamond-grade for its declared scope.
- **Quality over speed.** No deadline pressure. A PR lands when it is ready.
- **Perfect diamond.** Zero band-aid, zero residue, zero ghost reference. Every leftover gets fixed or flagged — never ignored.
- **Craft, not extraction.** Legacy code at `main` (v0.79.3) is a read-only reference. Diamond rewrites each crate from scratch, guided by the legacy.

---

## Local development setup

### Prerequisites

- Rust stable (`rust-toolchain.toml` pins the edition).
- macOS, Linux, or Windows (WSL). Primary dev on macOS.

### Core commands

```bash
# Run all lib tests (always --lib to avoid macOS Keychain popups)
cargo test --workspace --lib

# Clippy, zero warnings required
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --check

# Engine-internal hygiene vectors
bash scripts/hygiene/check-all.sh

# Regenerate the canonical status block
bash scripts/refresh-status.sh
```

### Useful extras

```bash
# Per-crate gate check
bash scripts/ci/check-crate-gates.sh <crate-name>

# Mutation testing (slow, per crate)
cargo mutants -p <crate-name>

# Doc build, zero warnings required
cargo doc --workspace --no-deps
```

---

## The 12-gate crate admission model

No crate enters `Cargo.toml` `members = [...]` without all 12 gates passing
in the same PR. No "we'll fix gate X later."

| # | Gate           | What it checks                                                       |
|---|----------------|----------------------------------------------------------------------|
| 1 | SPEC           | `docs/crate-specs/nika-X.md` exists — purpose, layer, LOC, public API |
| 2 | TDD            | Tests written **before** implementation, red before green             |
| 3 | IMPL           | Minimal, compiles, tests pass, no `# TEMP` without removal plan       |
| 4 | CLIPPY         | `cargo clippy --workspace --all-targets -- -D warnings` (0)           |
| 5 | MUTATION       | `cargo mutants -p nika-X` ≥ 90% killed                                |
| 6 | PROPERTY       | Proptest for parsers, encoders, security-sensitive paths              |
| 7 | BENCHMARKS     | `benches/` if hot path (or justified exemption in spec)               |
| 8 | DOCS           | `cargo doc --no-deps` 0 warnings, all pub items documented            |
| 9 | CANARY E2E     | `tests/canary-X.nika.yaml` passes (or justified exemption)            |
| 10| PARITY LEGACY  | Golden test vs `git show brouillon:...` output                             |
| 11| REVIEW SWARM   | 3 parallel reviewers (`spn-nika:code-reviewer`, `spn-rust:rust-pro`, `feature-dev:code-reviewer`), P0/P1 fixed same session |
| 12| ATOMIC COMMIT  | 1 commit, co-authored Nika                                            |

Full spec: [`.claude/rules/diamond-discipline.md`](.claude/rules/diamond-discipline.md) +
[`docs/architecture/forward-compat-invariants.md`](docs/architecture/forward-compat-invariants.md).

---

## Architecture invariants

Before modifying any crate, know these:

- **L0** crates: zero I/O, zero async, ≤ 15,000 LOC.
- **L0.5** crates: traits only (`nika-kernel`, `nika-kernel-mock`).
- **L1** effect crates: one trait impl each (clock, fs, http, blob, process, ...).
- **L2** domain crates: `verb-*`, service crates, memory stubs.
- **L3** orchestration: runtime + daemon.
- **L4** interfaces: cli, lsp, serve, sdk, init, lints.
- **L5** binary: `nika` (≤ 500 LOC composition root · the `nika` bin target is already born in L4's `nika-cli`; L5 will own it, never rename it · ADR-135).

Strict **downward** dependencies only. No upward imports.
Enforced by `scripts/ci/check-layering.sh` + `cargo-deny`.

Forward-compat is non-negotiable. Public API surface is protected by
`cargo public-api` + `cargo semver-checks` + `cargo deny` +
`#[non_exhaustive]` on every public type. See
[`docs/architecture/forward-compat-invariants.md`](docs/architecture/forward-compat-invariants.md).

---

## Commit convention

Format: `type(scope): lowercase description`.

```
feat(nika-X): admit to workspace — all 12 gates passed
fix(nika-Y): propagate parse error via ? instead of panic
refactor(dx): split oversized hygiene script into modules
docs(roadmap): refresh current state section
chore(deps): bump tokio 1.42 to 1.43
```

Every commit message ends with:

```
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Full rules: [`.claude/rules/commit-granularity.md`](.claude/rules/commit-granularity.md).

### Banned

- `git add -A` or `git add .` (stage by explicit path).
- `--no-verify` on commits (fix the hook instead).
- `git commit --amend` on pushed commits.
- `Co-Authored-By: Claude` (always Nika).
- `cargo test --test` (macOS Keychain popup — use `--lib`).

---

## Developer Certificate of Origin

Every PR commit carries a `Signed-off-by` trailer matching the commit
author ([DCO 1.1](https://developercertificate.org) — the Linux-kernel
convention, the lightweight alternative to a CLA). Signing off certifies
you have the right to submit the change under AGPL-3.0-or-later.

```
git commit -s                       # appends: Signed-off-by: Your Name <your-commit-email>
git rebase --signoff origin/main    # repair an existing branch, then force-push it
```

CI enforces this on every PR (`.github/workflows/dco.yml`). Merge
commits and bot-authored commits (`*[bot]`) are exempt, so the heal
lanes keep flowing.

---

## Pull request expectations

- **Atomic.** One logical change per PR. No "feat + refactor + docs" bundles.
- **Tests pass.** `cargo test --workspace --lib` green.
- **Clippy clean.** Zero warnings.
- **Hygiene clean.** `bash scripts/hygiene/check-all.sh` green.
- **No `.unwrap()` / `.expect(` in `src/`.** Use `?` propagation. Enforced by `workspace.lints.clippy.unwrap_used = "deny"`.
- **No `#[allow(dead_code)]`.** Delete or make `pub(crate)`.
- **`#[non_exhaustive]` on every public error enum + response struct.**
- **Files ≤ 1,500 LOC, crates ≤ 15,000 LOC.** Enforced in CI.
- **One crate admission = one PR.** Never batch two admissions.

### PR review

- Small changes: single reviewer, merge when green.
- Crate admissions: 3-agent review swarm, all P0/P1 fixed in the same session before merge.
- Architecture / ADR changes: explicit escalation, no silent merges.

---

## Reporting issues

- **Bug**: GitHub Issues, include minimal repro and `nika --version`.
- **Feature request** (engine behavior): GitHub Issues, describe the use case before the solution.
- **Language change** (anything normative: syntax, verbs, permits, envelope): the spec's [NEP door](https://github.com/supernovae-st/nika-spec/blob/main/governance/nep-0000-the-nep-process.md) — nobody amends the standard directly, maintainers included. Engine issues cannot change the language.
- **Security vulnerability**: see [`SECURITY.md`](SECURITY.md). Email `nika@supernovae.studio`, **not** a public issue.

---

## Code of Conduct

By participating, you agree to uphold the
[Contributor Covenant v2.1](CODE_OF_CONDUCT.md). Contact for reports:
`nika@supernovae.studio`.

---

## Contact

- General: [`nika@supernovae.studio`](mailto:nika@supernovae.studio)
- Website: [nika.sh](https://nika.sh)
- Docs: [docs.nika.sh](https://docs.nika.sh)
- Studio: [supernovae.studio](https://supernovae.studio)

Merci. Nika is a small project that takes quality seriously.
Every contribution — however small — helps keep the diamond clean.
