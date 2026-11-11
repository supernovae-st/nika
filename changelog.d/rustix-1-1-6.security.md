- **`rustix` is past its extended-attribute unsoundness advisory.** Given a
  zero-length buffer, `getxattr`, `listxattr` and their `l` and `f` forms
  marked bytes they never wrote as initialized in 1.1.4
  (RUSTSEC-2026-0486); 1.1.6 returns an empty result instead. The engine's
  own code calls none of those functions, but the audit denies unsoundness
  in every lockfile, so the advisory refused each push. The workspace pin
  and the three lockfiles (the root, `fuzz/` and `crates/nika-acp/`) move
  together. The 0.38 series, which the advisory does not affect, stays
  where it was.
