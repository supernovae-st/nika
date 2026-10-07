# nika-compile-seats

The two capabilities a host lends a preparation of the Compile core. `decide` is the bounded
decision seat of the WARM strategy: a `ChoiceQuestion` offers options Nika already found
admissible plus NONE, a `DecisionSeat` answers exactly one of them (a `ProviderChoice` seats a
generative provider through a JSON-schema enum, one physical call, no retry), and the answer is
revalidated (`admit`) before any assembly and projected for provenance (`record`). `rehearse`
is the rehearsal port: a host that can run a candidate in a safe room built from the observed
world answers a `RehearsalReport` (the exact bytes, the world it admitted, whether a run began
and how it ended, the effects its denied seams saw attempted), and `judged_run` maps it to the
behavioural judge's run. `reasoning` holds the reasoning one call is asked for and the record of
what it reported, shared with every authoring call of the seats' doors. Neither capability
grants authority. It is a size-cap member of the `nika-onboard` unit (ADR-146 ·
D-2026-07-09-N1 · the ADR-144 precedent), below `nika-compile-cognition`, which keeps both
modules at their historical paths `nika_compile_cognition::{decide, rehearse}`; this crate
depends on `nika-compile`, never the reverse.
