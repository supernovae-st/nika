# nika-compile-fidelity

The laws a candidate `.nika` document is judged by, and the two forms that tie a candidate
to the reader's plan. `fidelity` holds the laws, pure over (request · plan · projected
document): every source the request states is read, every destination it states is written,
an effect a stated approval holds back runs only on a confirm `nika:prompt`'s yes, a
prohibited effect is absent, no path or host the request never wrote appears, and the text
of a read is not records. Each refusal is one structured diagnostic a seat can repair from.
`sketch` is the constrained intermediate a seat proposes, its structural laws, its typed
holes and the document it states; `candidate` is the plan a candidate document states by
its structure, and a revision's delta. Nothing here reads an intent, calls a model, touches
the file system or grants authority. It is a size-cap member of the `nika-onboard` unit
(ADR-141 · D-2026-07-09-N1 · the ADR-137 and ADR-138 precedents), ascended from
`nika-compile-reader` at the 15k prod-LOC wall: `nika-compile` depends on this crate and this
crate depends on the reader, never the reverse. Its public types (`fidelity::Diagnostic`,
`sketch::{Sketch, SketchTask, Edge, Verb, Hole, Fill}`) moved as they were and are not yet
`#[non_exhaustive]`: that ratchet is owed, not claimed.
