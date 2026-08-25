# `changelog.d/` — one file per change

A change describes itself **here**, in its own file, not in `CHANGELOG.md`.
Nothing assembles these until the tag.

## Why

`CHANGELOG.md` was the one file every branch wrote to. Measured 2026-08-24 on
four open security pull requests — #1162, #1163, #1164, #1165 — `git merge-tree`
reported exactly **one conflict each, always `CHANGELOG.md`**, while the four
touched nine distinct crate files between them and collided on none. The
changelog alone cost four rebases for changes that never met.

The same shape had already been met once here, on `estate.yaml`: four pull
requests, four conflicts, 2026-08-20 (`.gitattributes` carries that story). A
shared append target is a shared index, and a shared index conflicts by
construction. Discipline cannot fix that; only the shape can.

Two branches can never write the same path. So they can never collide.

## Adding one

Create **one file per change**, named `<sort-key>.<section>.md`:

```
changelog.d/1068.changed.md
changelog.d/serve-bind-token-mint.added.md
```

- **sort key** — the issue number when there is one (they read naturally:
  `761` `905` `1068`), otherwise a slug. It decides the order inside a section
  and nothing else. Two authors picking the same number still write two
  different files, so it can never conflict.
- **section** — one of `added` `changed` `deprecated` `removed` `fixed`
  `security` (Keep a Changelog 1.1.0). The set is closed: a free-form section
  would let two fragments invent two spellings of the same heading.

The body is **one markdown bullet**, no blank lines inside it:

```markdown
- **The headline is a bold sentence.** Then the detail, wrapped at 76
  columns, continuation lines indented two spaces.
```

Line 1 must open `- **`. That is not cosmetic:
`scripts/ci/next-tag-project.sh` counts what the next tag *claims* by matching
that prefix, so a fragment written any other way is invisible to the
projection — the exact class of defect this repo has been paying for.

## Proving it

```sh
bash scripts/release/changelog-assemble.sh --check   # the gate
bash scripts/release/changelog-assemble.sh           # what it will render
bash scripts/release/changelog-assemble.sh --list    # one line per fragment
```

`--check` runs pre-push (`scripts/hooks/run-ci-ratchets.sh`) and in CI
(`diamond-ci.yml`, the `ratchet` matrix). It refuses a malformed fragment
**and** a bullet hand-written back into `## [Unreleased]`.

## At tag time

`RELEASING.md` step 1. `--fold <version>` splices the assembled body into
`CHANGELOG.md` as `## [<version>]`, restores the `[Unreleased]` stub, and
deletes the fragments it consumed. `render-notes.sh` then reads that section
for the release page, unchanged.

This file is permanent: it is the contract a contributor reads, and it keeps
the `changelog.d/**` estate glob non-empty when every fragment has been folded
(a rule matching nothing is a coverage hole, and the estate gate refuses one).
