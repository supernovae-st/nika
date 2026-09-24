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
