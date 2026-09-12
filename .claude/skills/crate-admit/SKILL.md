---
name: crate-admit
description: Prepare and complete a named crate's admission to the Nika workspace under ADR-003. Use only when adding a crate to workspace membership.
argument-hint: [crate-name]
allowed-tools: Bash, Read, Edit, Grep
---

# Admit a crate

Complete the requested admission under `docs/adr/adr-003-12-gate-admission.md`,
`CONTRIBUTING.md` and the forward-compatibility invariants. The outcome is an
implemented crate with all applicable gate evidence and one coherent admission
commit. Publication follows the user's existing authorization.

Resolve the candidate, manifest, crate spec, HEAD and concurrent changes. Audit
current evidence using [gate-check](../gate-check/SKILL.md). Implement missing
work and fix failed gates within scope; a first red result is a diagnostic,
not an instruction to abandon the task. A genuinely missing decision, tool or
review remains a named blocker, not an invented pass.

Use an isolated checkout for any temporary workspace admission. Edit membership
structurally and review the diff; do not use a shell trap that restores a shared
Cargo.toml over another session's edits. Keep test-first evidence, mutation
results and contract-supported N/A explanations in the crate's existing spec.

Gate 11 uses [.claude/agents/review-swarm.md](../../agents/review-swarm.md) for
three independent perspectives. Give reviewers the candidate and relevant
contract, retain actionable findings and resolve admission blockers before
committing. Do not substitute a paid model or assume a named client tool exists.

Stage the exact crate, manifest/lockfile and affected documentation changes.
Inspect the staged diff immediately before the atomic commit. Use the subject
`feat(<crate>): admit to workspace — all 12 gates passed` only when the required
preceding gates actually passed; include their evidence and the trailer
`Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`.

Verify the resulting commit and report its revision, evidence and limitations.
Refresh generated status only when needed with the documented generator; do not
invent phase tags or ask again for a push the user already authorized.
