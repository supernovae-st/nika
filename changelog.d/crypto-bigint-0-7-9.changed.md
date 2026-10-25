- **`crypto-bigint` moves from the yanked 0.7.6 to 0.7.9.** Upstream
  yanked 0.7.6, 0.7.7 and 0.7.8 for a breaking change, so `cargo deny
  check` refused the lockfile and blocked every push. The crate reaches
  the build through `p384`, which verifies the Sigstore timestamp
  authority's ECDSA P-384 leaf in trace anchoring. 0.7.9 keeps the same
  dependency set and has no build script or procedural macro.
  `Cargo.lock` and `fuzz/Cargo.lock` move together.
