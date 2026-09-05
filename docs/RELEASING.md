<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# Release documentation

The engine release ceremony has one owner:
[RELEASING.md at the repository root](../RELEASING.md).
Follow that document for preparation, the whole-tree estate projection,
pre-tag gates, publication, recovery and the verified timeline record.
This page is an index, not a second executable release recipe.

## Published and candidate identities

A Git tag is an immutable source coordinate, not proof of publication.
Stable consumers follow a published, non-draft release and its independently
verified artifacts. A failed train may have a newer tag than the latest
published release; never select stable with the highest local Git tag.

Candidate source, a development build, a GitHub release and an installed npm
package are different observations. Record their exact version, commit and
artifact identity; do not relabel old measured evidence to match a new version.
The workspace manifest owns the engine version and the canonical release
sweep owns its carriers.

The executable is built as **nika**, not renamed from a different binary
during packaging. [ADR-135](adr/adr-135-the-public-executable-is-born-nika.md)
owns this identity.

## Recovery boundaries

The root ceremony defines the visibility barrier and its residual authority.
It is not a cross-registry transaction: already-published bytes are immutable
and recovery converges forward, never by moving tags or replacing divergent
assets. Keep failed tags intact.

Dispatch an existing-tag replay from the current workflow with `--ref main`;
the separate tag input identifies the immutable source. Missing tag-context
SLSA requires the original tag-push run, not a branch-context replacement.
The root ceremony also owns required `NPM_TOKEN` and `TAP_DEPLOY_KEY` setup,
the newest-public-stable proof, and post-public Homebrew/latest convergence.
Do not use a manual formula write to bypass those checks.

## Downstream owners

- The [SDK release workflow](https://github.com/supernovae-st/nika-client/blob/main/.github/workflows/release.yml)
  and [release evidence](https://github.com/supernovae-st/nika-client/blob/main/docs/testing.md)
  own the public-engine replay and prepared publication of the four native
  payloads plus the root SDK. A bare `npm publish` is not this release train.
- [Editor publishing](https://github.com/supernovae-st/nika-vscode/blob/main/PUBLISHING.md)
  owns the readiness, native-host and manual gates plus both Marketplace and
  OpenVSX publications. The matching public engine is a prerequisite.
- The portable Agent Plugins mirror is downstream of the immutable engine
  tag; its resync and reviewable PR follow the root ceremony. Never copy the
  published mirror back over the engine's canonical skills.
- The root ceremony closes the cross-repository release record through the
  spec-owned timeline and its rendered public page.

A downstream source PR, a draft or a green local test is not a published
installation verdict. Each owner must prove the exact bytes it ships.
