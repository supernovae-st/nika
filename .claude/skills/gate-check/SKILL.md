---
name: gate-check
description: Audit the evidence for a named crate's 12 admission gates. Use for admission readiness or gate progress, not ordinary code validation.
argument-hint: [crate-name]
allowed-tools: Bash, Read, Grep
---

# Audit crate admission evidence

Use `docs/adr/adr-003-12-gate-admission.md`, the candidate's
`docs/crate-specs/<crate>.md`, and the current revision. Validate the crate name
against the manifest before interpolating it into commands. This is an audit:
do not modify workspace membership, commit, or launch execution outside scope.

Report each gate as PASS, FAIL, PENDING or a justified N/A, with its actual
command/result or evidence path. Review available evidence first; execute the
missing authorized checks needed for the requested audit. A test count does not
prove test-first development, a file's existence does not prove its test ran,
and an empty warning search does not prove the compiler succeeded.

| Gate | Evidence |
|---|---|
| 1 Spec | Purpose, layer, API, dependencies and applicable exemptions |
| 2 Test first | Recorded failing-before-passing behavior and relevant history |
| 3 Implementation | `cargo test -p <crate> --lib --locked`, plus affected targets |
| 4 Clippy | `cargo clippy -p <crate> --all-targets -- -D warnings` exits successfully |
| 5 Mutation | `bash scripts/ci/check-mutation-floor.sh <crate>`; report score and limitations |
| 6 Property | Executed properties for parser, encoding and security contracts |
| 7 Benchmarks | Executed relevant hot-path benchmarks, or supported N/A |
| 8 Documentation | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p <crate>` and public API coverage |
| 9 Canary | Actual end-to-end canary result, or supported exemption |
| 10 Parity | Executed comparison with legacy behavior on the same inputs |
| 11 Review | Three independent reviews, findings and resolution at candidate revision |
| 12 Atomic commit | Exact admission diff and commit; pending until committed |

Preserve command exit statuses; do not pipe a compiler into `tail`, `grep` or
`wc` and use the last process as its verdict. Reuse relevant current evidence
and identify what became stale after edits. Missing tools or permissions stay
PENDING. Admission is not ready while a required gate is failed or pending.
