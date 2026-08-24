- **The next tagged binary includes the harness access class.**
  `release.yml` builds `--features local-infer,access-harness`. The
  160 KB `nika-harness` crate has been on main, API-frozen, since
  2026-08-08, and was compiled out of every downloadable binary.
  `agent:` tasks can sit on a detected harness via `--access <seat>`
  once that tag ships. Infer-grade harness (P4 · `infer:` on a seat)
  stays parked. `crates/nika-acp` stays a quarantined workspace (the
  official SDK's `preserve_order` must never unify into the engine);
  Diamond CI now runs its five batteries against `nika-harness` over
  a process boundary. `metal` stays off (candle 0.10 kernel dies at
  first token).
