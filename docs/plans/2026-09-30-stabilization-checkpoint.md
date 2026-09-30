# Stabilization checkpoint — 30 September 2026

This document records the implemented and verified engine state, the work in
progress, and the remaining release criteria. It is a status checkpoint,
not a declaration that the intent loop or release acceptance is complete.

The integration pull request is [#1753](https://github.com/supernovae-st/nika/pull/1753).

## Published implementation and evidence

This change strengthens request grounding, native answer review and approval replay protection, and connects the terminal session to typed consent and scheduling behavior.

- Ground an unnamed field only when an observed literal identifies it unambiguously; otherwise ask. Judge record scope and chronological ordering against the request.
- Require a judgment of the final candidate before a native answer round becomes ready. Provider-forbidden replays remain incomplete without making calls.
- Reject approval gates controlled only by caller inputs, and bind each decided approval to a single run across operator homes and project records.
- Preserve the session renderer with `--ascii`, keep keys typed during work out of the next decision, restore the terminal on hangup, and keep the composer responsive.
- Refuse a zero cost ceiling when a task certainly needs a priced model without a token ceiling. Mock and local execution retain their documented treatment.
- Support exact month-end schedules and anchored weekly intervals, including civil-time behavior across clock changes and the corresponding scheduler readiness information.

The three previously failing service and canon-explanation tests passed individually on this head. The normal pre-push completed successfully, including all 9,615 selected tests (8 skipped), clippy, hygiene, ratchets, dependency policy and unused-dependency checks. The separate hygiene preflight reported 40 green, 8 advisory yellow and no red results. All 42 CI checks passed on published head `24d1782cd`, including the required Rust test check. Release qualification remains separate.

## Work still in progress

The verified implementation head is `24d1782cd120b672ac09c5c6763cb979c1a5d3e8`. All 42 checks passed on that exact head. The integration pull request is not merged and no new release has been tagged or installed.

**Completed and verified:** the implementation described above, normal local publication gates, and the remote CI run. A development binary was built from that head and its version and specification identities checked. That binary still refers to a draft specification; it is not release-qualified.

**Prepared, not integrated or behavior-qualified:** the next terminal UI changes, restored-session identity work, strict knowledge admission, confined rehearsal interfaces, deterministic behavioral contracts, and further public conformance fixtures. Source review has found and corrected several test-oracle assumptions; those reviews are not engine test results. Terminal UI reconstruction has started, but its first commit attempt was interrupted during a hook and produced no commit. Its prepared files and diagnostic evidence are preserved.

**Concrete remaining work:** finish terminal UI integration and its isolated process tests; connect default knowledge to a qualified distribution; enforce rehearsal on copies with external effects and paid calls denied; judge requested behavior across discriminating fixtures; bind preview and consent to the selected result; then perform the graduated acceptance runs with a recorded ledger. The reference workflow is a witness, not a required source representation.

**Open findings and limits:** a prepared fix masks short resolved values in item evidence and task notes; it still needs actual regression execution. The machine trace reader needs a precise task-error projection. The trace-conformance adapter is being moved to the existing structured JSON evidence, with malformed, unsupported and incomplete outcomes kept distinct. Runtime confinement and cancellation remain unqualified until the corresponding tests run. Local permission blocks also prevent completion of the independent measurement preparation and one targeted checkout repair; no results are inferred from those blocked operations.

**Release status:** the current release remains `v0.121.0`; `0.122.0` is the planned next minor, subject to the final API and specification checks. Main integration, canonical specification pins, the full intent loop, acceptance measurements, the official release artifact, installation and user proof are all still pending. No functional success rate or release-readiness claim is made here.

This documentation checkpoint adds no runtime behavior. Its publication uses the normal repository hooks. The implementation test counts above belong to the named implementation head; they do not claim execution of the pending work.
