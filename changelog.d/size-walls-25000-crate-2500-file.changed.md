- **The size walls are 25,000 production lines per crate and 2,500 lines per file.** The CI
  ratchets and the pre-push gate now block a crate above 25,000 production lines and a source
  file above 2,500 lines; the hygiene dashboard warns, without blocking, above 15,000 and 1,500
  and names the crate closest to its wall, and a file with a `LOC-EXEMPT` marker stays capped at
  5,000 lines. One ceiling applies to every crate: the native renderer's 19,000-line exception
  is absorbed. ADR-154 records the decision and amends ADR-004, ADR-023 and ADR-143.
