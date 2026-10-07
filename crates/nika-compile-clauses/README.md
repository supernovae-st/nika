# nika-compile-clauses

How one clause of a request reads above the reader. `parts::parts` cuts a request into the
parts a verifier asks alone, each an exact excerpt of it, and `parts::{restricts,
asks_an_operation}` read whether a part restricts and whether it may ask an operation of its
own. `prohibition` reads whether a clause forbids by negation alone (`pure_prohibition`),
demands by a negation of forgetting (`negated_demand`) or states an operation of its own beside
a law or a negation (`states_operation`). `words` holds the word tables of the proposal merge
and their classifiers (the key a clause is folded by, a draft that is no language work, a format
kept by construction, the content words of a clause), and `spellings::stated_spellings` the
literals a clause states that the host observed spelled with other bytes. The readings are pure
over the words: nothing here asks a judge, reads a candidate or grants authority. It is a
size-cap member of the `nika-onboard` unit (ADR-145 · D-2026-07-09-N1 · the ADR-142
precedent), placed above the reader and below `nika-compile-cognition`: cognition depends on
this crate and this crate depends on the reader, never the reverse.
