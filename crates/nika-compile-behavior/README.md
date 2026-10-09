# nika-compile-behavior

The behavioural contract a request states, independent of any candidate, and its typed
judgment over what a round of rehearsals consumed and wrote. `behavior::contract_of` and
`behavior::contract_of_request` state, from the reader's plan of the request alone, where each
requested result lands, whether the request wants it written and what it must hold;
`behavior::judge` reads the evidence a host hands over (what each run consumed, what it read
back, how it ended) through the canonical readings (exact JSON numbers, the `nika:convert` CSV
reading, every value bound to the sha256 of the bytes read) and tallies each obligation's
outcomes with an explicit dominance, a stop never certified; `behavior::select` judges several
candidates against one contract. `instant_shape` is the form and offset of a date-time text,
read before text order may stand for time order. Nothing here reads a candidate document, runs
a rehearsal, touches the file system, calls a model or grants authority. It is a size-cap
member of the `nika-onboard` unit (ADR-149 · D-2026-07-09-N1 · the ADR-146 precedent),
descended from `nika-compile-fidelity` at the 15k prod-LOC wall: fidelity depends on this crate
and keeps it at its historical paths (`nika_compile_fidelity::behavior`,
`nika_compile_fidelity::fidelity::instant_shape`), and this crate depends on the reader, never
the reverse.
