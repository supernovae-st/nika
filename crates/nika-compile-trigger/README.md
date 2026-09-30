# nika-compile-trigger

The trigger a request names, read from its words. `phrase_words` splits a folded phrase,
`stated_cadence` gives its coarsest cadence and its time of day (`time_of_day`), `multiple`
tells a period five cron fields cannot hold (« every other Monday », « every 5 hours »),
`schedule::fields` proposes the cron fields a phrase states whole, and `classify` reads the
form of a trigger clause (a distribution, a sequence, a schedule or an event; `arriving` tells
an item that comes with the invocation). `words` holds the words only this reading reads. The
reading is pure over the words: nothing here binds a requirement, asks a question, reads a
request or grants authority; `nika-compile` turns what it reads into `requested_trigger`. It is
a size-cap member of the `nika-onboard` unit (ADR-142 · D-2026-07-09-N1 · the ADR-141
precedent), ascended from `nika-compile` at the 15k prod-LOC wall with the tables the reader
never read: `nika-compile` depends on this crate and this crate depends on the reader, never
the reverse. `TriggerForm` moved as it was and is not `#[non_exhaustive]`: the unit matches it
exhaustively.
