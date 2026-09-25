# nika-compile-reader

The frozen deterministic reader of the Compile core and the typed semantic plan it
produces. One free intent in, one `lexicon::Reading` out: a private `plan::Plan` of
operations, effects, obligations, bindings, constraints and typed rules, every element
anchored by a verbatim excerpt of the intent, read through closed multilingual head, cue
and marker tables and the structural laws of objects, gates, paths and columns. Nothing
here invents an element, calls a model, touches the file system or grants authority;
`serde_json` is its only dependency. It is the second size-cap member of the
`nika-compile` unit (ADR-138 · D-2026-07-09-N1 · the ADR-137 precedent): `nika-compile`
composes, assembles and previews what is read here and depends on this crate, never the
reverse. The reader is FROZEN (a safety floor): every public type is `#[non_exhaustive]`,
elements are built through their constructors, and every new law lives in the typed plan.
Five public types added after the split (`cardinality::{Bound, Measure}`,
`shape::{LiteralLookup, Shape}`, `structure::Law`) are not yet `#[non_exhaustive]`: that
ratchet is owed, not claimed. The laws a candidate document is judged by, the seat's sketch
and the plan a candidate states live one member above, in `nika-compile-fidelity` (ADR-141).
