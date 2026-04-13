# Nika — Release Chain Index

This directory holds the detailed release-body drafts ready to paste into
`gh release create --notes-file docs/releases/vX.Y.Z.md`. Each file is a
standalone document — no cross-file state, no include-macros. Drafts can be
edited freely before tagging.

The chain currently covers eight releases between `v0.79.3` (the last
published tag) and `v0.80.0` (the pre-launch scope-sharpening cut). Each
release is narrow and narrative: one theme, one audience, one story.

---

## The chain at a glance

```
v0.79.3 ─── (last published — Constellation Keystone)
   │
   ▼
v0.79.4     Codegen Infrastructure         ─▶ contributors, embedders
   │        nika-macros · 3 proc-macros        (invisible to workflow authors)
   │        ~170 LOC boilerplate removed
   │
v0.79.5     Builtin Tools Crate             ─▶ embedders, security auditors
   │        nika-builtin · 24+ tools            (reliability fixes for users)
   │        BuiltinRouter kernel trait
   │        5-agent review hardening (18 commits)
   │
v0.79.6     Security Crate + Hardening      ─▶ ⚠️  ALL USERS — two CVEs
   │        nika-security L1 crate              (HIGH: \r blocklist bypass)
   │        6 new redact patterns                 (MED: JSON nesting DoS)
   │        \r/\x0B/\x0C blocklist fix
   │        MAX_REDACT_DEPTH=128 DoS fix
   │
v0.79.7     Honest Structured Output        ─▶ docs readers, embedders
   │        Delete phantom "L1 rig Extractor"
   │        4-layer defense (not 5) — truthful now
   │        Unwrap ratchet CI gate (baseline: 89)
   │
v0.79.8     Binding Architecture Migration  ─▶ embedders, contributors
   │        binding/resolve.rs → nika-core
   │        BindingStore kernel trait
   │        137 resolver tests migrated
   │        −2,853 engine LOC
   │
v0.79.9     Phantom Sweep                   ─▶ contributors
   │        Nuke nika-runtime crate (0 callers)
   │        Delete NIKA-383 + NIKA-385 phantoms
   │        Drop dead resolve shim re-exports
   │
v0.79.10    Capability Validation (Track 2) ─▶ ALL USERS — safety feature
   │        NIKA-120 UnsupportedProviderCapability
   │        ProviderCapabilities SSOT
   │        nika eval full JSON Schema
   │
   ▼
v0.80.0     The Great Deletion              ─▶ CLI users + IDE users
            Month A W1 + W2                      (breaking: TUI, course, editors)
            −119,710 LOC workspace
            nika-tui + napi + py + 3 editors     (workflow YAML unchanged)
            docs grep-verified reality
```

---

## Tag plan (decision pending)

| Tag | HEAD SHA (proposed) | Type | Theme | Drafted | Audience |
|---|---|---|---|---|---|
| `v0.79.4` | _pre-macros end_ | patch | Codegen Infrastructure | [✓](./v0.79.4.md) | contributors |
| `v0.79.5` | _post-builtin review_ | patch | Builtin Tools Crate | [✓](./v0.79.5.md) | embedders |
| `v0.79.6` | _post-S19_ | patch | Security + Hardening | [✓](./v0.79.6.md) | ⚠️ all users |
| `v0.79.7` | _post-S20_ | patch | Honest Structured Output | [✓](./v0.79.7.md) | embedders |
| `v0.79.8` | _post-S24_ | patch | Binding Architecture | [✓](./v0.79.8.md) | embedders |
| `v0.79.9` | _post-phantom-sweep_ | patch | Phantom Sweep | [✓](./v0.79.9.md) | contributors |
| `v0.79.10` | `c9b0516c4` | patch | Capability Validation | [✓](./v0.79.10.md) | ⚠️ all users |
| `v0.80.0` | current HEAD | **minor** | The Great Deletion | [✓](./v0.80.0.md) | all users |

**SHA selection is pending.** The release notes are drafted thematically, which is more faithful to the narrative than to git archaeology. Before tagging, the release manager must pick the exact SHA boundaries; minor content adjustments may follow. Guidance:

- Pick SHAs at natural "theme-complete" points (last commit with the release's primary subject).
- If a commit from a different theme snuck into the range, either (a) accept it and mention it in a "Also included" footer, or (b) bump the release up/down.
- Prefer consolidation over sprawl — if two adjacent releases have tiny deltas, merge them.
- Never rewrite history (`git rebase`, `git cherry-pick`) to force a clean cut. Tags are narrative, not audit.

---

## Voice and style reference

See `/private` working memory doc `project_release_style_guide.md` (internal
reference; not published). Short version:

- **CHANGELOG.md voice** — technical, metrics-driven, bullets with bold titles,
  LOC/delta callouts. Keep a Changelog format.
- **GitHub release body voice** — narrative, user-impact focus, stats summaries,
  install matrix. Banner art + install table + stats box.
- **Banner** — `╔═══...═══╗` (double-line) for minor/major; `┌──...──┐` (single-line)
  for tips and warnings inline.
- **Emoji** — 🦋 Nika, ⚠️ breaking/security, ✨ features, 🐛 fixes, 🚀 CI/perf, 🔐 security.
- **Stats block** — always include: commits count, LOC delta, test count, clippy
  warnings (should be 0), breaking changes (should be "None" when possible).
- **Install block** — brew / cargo / docker / direct download / VS Code.
- **Breaking changes format** — `**Impact:**` + `**Migration:**` with before/after YAML.

---

## Distribution channels (per release)

Every tagged release publishes to:

1. **GitHub Releases** — `gh release create vX.Y.Z --notes-file docs/releases/vX.Y.Z.md`
   (7 pre-built binaries: macOS arm64/x64, Linux gnu arm64/x64, Linux musl x64, Windows x64)
2. **crates.io** — 13 crates publish in order with 30-second index-propagation delays,
   leaf-first (nika-core → nika-event → ... → nika binary)
3. **Homebrew tap** — `supernovae-st/homebrew-tap` (formula auto-PR via CI)
4. **Docker Hub + ghcr.io** — multi-arch image tags `X.Y.Z`, `latest`, `sha-<shortsha>`
5. **Scoop bucket** — `supernovae-st/scoop-nika` (Windows)
6. **VS Code Marketplace** — `supernovae.nika-lang` (when the extension changed)
7. **OpenVSX Registry** — Cursor, Windsurf, VSCodium

(npm + PyPI pipelines were removed in v0.80.0 alongside the `nika-napi` + `nika-py` crate deletions.)

---

## How to ship a release

```bash
# 1. Pick the SHA (start of PR review session)
git log --oneline v0.79.3..HEAD | less

# 2. Bump the workspace Cargo.toml version
#    Update tools/Cargo.toml [workspace.package] version = "0.79.X"
#    Run ./scripts/sync-versions.sh (if it exists) to propagate to npm/VS Code

# 3. Update CHANGELOG.md
#    Prepend a new entry for the tag, referencing this docs/releases/vX.Y.Z.md

# 4. Commit + tag
git add tools/Cargo.toml Cargo.lock tools/nika/CHANGELOG.md
git commit -m "release: v0.79.X"
git tag v0.79.X
git push origin main v0.79.X

# 5. CI takes over
#    Builds 7 platforms in parallel
#    Creates the GitHub release (pulls body from docs/releases/vX.Y.Z.md
#      via an `--notes-file` flag in the release job)
#    Publishes to all 7 channels
#    Opens a PR on homebrew-tap with the new formula

# 6. Post-release
#    Watch the homebrew PR; merge when CI green
#    Verify: brew update && brew upgrade nika && nika --version
#    Post the release body to the launch channel (Discord/Twitter/HN/etc.)
```

---

## Pre-flight checks before every tag

```bash
# Stale references (grep MUST return 0)
rg 'nika ui|nika chat|nika studio|--course|nika course' \
   packages/npm/ scripts/ install.sh docs/ README.md

# Workspace version coherent with intended tag
grep '^version' tools/Cargo.toml

# Tests + clippy
cd tools
cargo test --workspace --lib
cargo clippy --workspace --all-targets
cargo check --workspace
cargo check --workspace --no-default-features

# Dry-run changelog from git-cliff
cd ..
git cliff --tag v0.79.X --unreleased | less

# Homebrew tap audit (sibling repo)
cd ../homebrew-tap
rg 'ui|chat|studio|course' Formula/*.rb   # should be 0 for v0.80.0+
cd ../nika
```

---

## Links

- [SECURITY.md](../../SECURITY.md) — threat model, 5-layer Shield
- [MANIFESTO.md](../../MANIFESTO.md) — why Nika exists
- [CHANGELOG.md](../../tools/nika/CHANGELOG.md) — full history (Keep a Changelog format)
- [CONTRIBUTING.md](../../CONTRIBUTING.md) — build, test, conventions
- [AGENTS.md](../../AGENTS.md) — AI coding assistant context

> 🦋 *Built in Paris. Open source (AGPL-3.0-or-later). Forever.*
