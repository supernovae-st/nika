# nika-compile-cognition

The seats' doors of the Compile core. A model proposes a private semantic plan (the COLD
door: decoded, merged, composed and assembled by the core), writes the `.nika` itself (the
native door: parsed, checked, judged by the fidelity laws, repaired over bounded rounds,
replayed on every answer round with zero calls), or sketches its structure first and fills
typed holes (the sketch door); beside them the verified transform (a seat's jq program run on
the seat's own example), the knowledge door (the Foundry snapshot recalled per intent) and the
bounded decision seats. It is a size-cap member of the `nika-onboard` unit (ADR-140 ·
D-2026-07-09-N1 · the ADR-137 and ADR-138 precedents): it depends on `nika-compile` and
`nika-compile-reader` and `nika-compile-fidelity` and reads the core's stated `surface`; the core never depends back.
`nika-onboard` re-exports the unit at the paths every caller reads (`nika_onboard::compile`).

Hosts can opt into `compile_with_cognition_rehearsed` and provide the existing `Rehearse`
port. Native creation and revision rehearse the materialized, checked candidate before
acceptance; task failures and missing outputs consume the existing repair budget. Every
final Ready candidate also crosses the same barrier, including replay and COLD. A safe
`NotRun` remains explicit and grants nothing; an invalid host report stops compilation
without asking the author to repair the harness. The old entry offers no rehearsal host.

The compile decision records this invocation's reports and `Usage`: exact candidate
digest, attempt, output preview and coverage, copied-file digests, room lifecycle and
denied-effect counters. These facts are data, never a replayable approval. A host must
still bind the proposed bytes and observed world to its later consent and run. The native
repair count bounds these attempts; this API does not claim a Session-wide spending
allowance or general behavioural certification.

Native CREATE answer rounds reconstruct exact read/write paths from the core's question
application before rehearsal. Only paths introduced by the recorded questions' applied
answers join the request's paths; content strings and saved path metadata grant no reads.
The final barrier uses the same projection and the exact checked candidate. Source-list
bindings and paths inherited or superseded by EDIT are not extended by this native seam.
