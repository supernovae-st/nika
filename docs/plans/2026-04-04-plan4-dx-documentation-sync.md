# Plan 4: DX & Documentation Sync

**Date**: 2026-04-04 | **Version**: v0.68.0 (feature freeze)
**Priority**: HIGH for version sync (user-facing), MEDIUM for rest
**Source**: 7-agent mega audit (DX/Docs Explorer)

---

## Overview

| ID | Finding | Location | Effort |
|----|---------|----------|--------|
| DOC-1 | `project-info.json` stuck at v0.52.0 | `.claude/project-info.json` | 5m |
| DOC-2 | README footer says v0.65.1, 17 crates | `README.md` footer | 5m |
| DOC-3 | Tool count: "45+" should be "61" | `README.md:305,493` | 10m |
| DOC-4 | Transform count inconsistency (31 vs 50) | `CONVENTIONS.md` | 15m |
| DOC-5 | CHANGELOG stops at v0.66.0 | `CHANGELOG.md` | 30m |
| DOC-6 | CI badge could verify version | `.github/workflows/` | 30m |
| DOC-7 | Workspace Cargo.toml: workspace lints | `tools/Cargo.toml` | 15m |
| DOC-8 | async-stream / futures version standardization | `tools/Cargo.toml` | 10m |
| DOC-9 | Duplicate serde-saphyr versions (0.0.16 + 0.0.20) | `Cargo.lock` | 10m |

**Total**: ~2.5 hours

---

## DOC-1: Fix `project-info.json` Version

### Problem

```json
"nika": {
  "version": "0.52.0",     // ← 16 versions behind!
  "description": "...12 Rust crates",  // ← should be 18
}
```

### Fix

**File**: `/Users/thibaut/dev/supernovae/nika/.claude/project-info.json`

This is a symlink to `dx/.claude/project-info.json`. Update at the source:

```json
"nika": {
  "description": "Semantic YAML workflow engine — 5 verbs, 18 Rust crates",
  "version": "0.68.0",
  "language": "rust",
  "schema": "nika/workflow@0.12"
}
```

**Important**: This file is at the PARENT repo level (`supernovae-hq`), not nika itself.
The symlink chain is: `nika/.claude/ → ../../dx/.claude/`.

### Automation idea

Add to release script:
```bash
# scripts/release.sh — after version bump
jq '.workspaces.nika.version = "'$VERSION'"' dx/.claude/project-info.json > tmp && mv tmp dx/.claude/project-info.json
```

---

## DOC-2: Fix README Footer

### Problem

```
**Nika v0.65.1** · Schema `nika/workflow@0.12` · Rust 1.86+ · 17 crates · 9,930+ tests
```

### Fix

**File**: `README.md` (last lines)

```
**Nika v0.68.0** · Schema `nika/workflow@0.12` · Rust 1.86+ · 18 crates · 9,800+ tests
```

Note: test count went down slightly due to test consolidation in v0.66-v0.68.
Use the actual count from `cargo test --workspace --lib 2>&1 | tail -1`.

---

## DOC-3: Update Tool Count

### Problem

README says "45+ Builtin Tools" but CHANGELOG v0.66.0 documents 61 builtin tools.

### Files to update

**File**: `README.md:305`
```markdown
### 61 Builtin Tools
```

**File**: `README.md:493` (Mermaid diagram)
```
BUILT["61 Builtin Tools"]:::backend
```

### Verification

Cross-check with the actual tool registry:
```bash
rg "fn name\(&self\) -> &'static str" tools/nika-engine/src/runtime/builtin/ -c
# Should show count matching the claim
```

---

## DOC-4: Clarify Transform Count

### Problem

CONVENTIONS.md says "31 available" in one section header but more transforms exist.
AGENTS.md says "50 transforms". The actual count from nika.md rules is 50.

### Fix

**File**: `CONVENTIONS.md`

Update the section header to match reality:
```markdown
## Pipe Transforms (50 available)
```

Ensure the list includes all 50 transforms as documented in the nika rules file.

---

## DOC-5: Update CHANGELOG for v0.67 and v0.68

### Problem

CHANGELOG.md ends at v0.66.0. Missing:
- v0.67.0: vault crate extraction, jaq 3.0, engine decoupling
- v0.68.0: `nika run URL`, `nika explain`, feature freeze

### Fix

Add entries based on git log:
```bash
git log v0.66.0..v0.68.0 --oneline
```

Structure:
```markdown
## [0.68.0] — 2026-04-04

### Added
- `nika run <URL>` — execute remote workflows
- `nika explain` — human-readable workflow summary

### Changed
- Feature freeze for launch preparation

## [0.67.0] — 2026-04-04

### Added
- `nika-vault` extracted as independent crate
- Upgraded jaq-core to 3.0 (from 1.5)

### Changed
- Engine decoupled from vault internals
- Architecture improvements for security audit
```

---

## DOC-6: CI Version Verification (Optional)

### Idea

Add a CI step that verifies README version matches Cargo.toml:

**File**: `.github/workflows/ci.yml`

```yaml
- name: Verify version consistency
  run: |
    CARGO_VERSION=$(grep '^version' tools/Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
    README_VERSION=$(grep -oP 'Nika v\K[\d.]+' README.md)
    if [ "$CARGO_VERSION" != "$README_VERSION" ]; then
      echo "::error::Version mismatch: Cargo.toml=$CARGO_VERSION README=$README_VERSION"
      exit 1
    fi
```

---

## DOC-7: Workspace Lint Configuration

### Problem

No `[workspace.lints]` in root Cargo.toml. Individual crates don't share lint config.

### Fix

**File**: `tools/Cargo.toml`

Add:
```toml
[workspace.lints.rust]
unsafe_code = "deny"
unused_must_use = "deny"

[workspace.lints.clippy]
unwrap_used = "warn"
expect_used = "warn"
```

Then in each crate's Cargo.toml:
```toml
[lints]
workspace = true
```

This centralizes lint policy and gradually enforces `unwrap()` awareness.

---

## DOC-8: Standardize async-stream and futures

### Problem

Mixed workspace vs pinned versions for:
- `async-stream`: some crates pin 0.3, others use workspace
- `futures`: both generic 0.3 and pinned 0.3.32

### Fix

**File**: `tools/Cargo.toml` (workspace dependencies section)

Ensure both are in `[workspace.dependencies]`:
```toml
[workspace.dependencies]
async-stream = "0.3"
futures = "0.3"
```

Then in each crate that uses them:
```toml
[dependencies]
async-stream = { workspace = true }
futures = { workspace = true }
```

### Verification

```bash
rg 'async-stream|futures' tools/*/Cargo.toml
# All should show { workspace = true }
```

---

## DOC-9: Unify serde-saphyr Versions

### Problem

Cargo.lock has both serde-saphyr 0.0.16 (via nika-media) and 0.0.20 (rest).
Two versions of the same YAML parser increases attack surface.

### Fix

Update the pinned version in the offending crate:
```bash
rg 'serde.saphyr' tools/nika-media/Cargo.toml
# Update to match workspace version
```

Then:
```bash
cd tools && cargo update -p serde-saphyr
cargo test --workspace --lib
```

---

## Execution Order

```
5 minutes (immediate):
├── DOC-1  project-info.json version
└── DOC-2  README footer

15 minutes:
├── DOC-3  Tool count (45+ → 61)
├── DOC-8  async-stream/futures workspace refs
└── DOC-9  serde-saphyr unification

30 minutes:
├── DOC-4  Transform count clarification
└── DOC-7  Workspace lints

1 hour:
├── DOC-5  CHANGELOG v0.67 + v0.68
└── DOC-6  CI version check (optional)
```

## Master Verification

```bash
# After all changes:
cargo check --workspace
cargo test --workspace --lib
# README version matches Cargo.toml
grep "Nika v" README.md
grep "^version" tools/Cargo.toml | head -1
# project-info.json matches
jq '.workspaces.nika.version' .claude/project-info.json
```
