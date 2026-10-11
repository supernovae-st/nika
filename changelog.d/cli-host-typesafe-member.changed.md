- **The TypeSafe decision seat lives in a size-cap member.** `nika-cli-host-typesafe` now holds
  the System One seat (its transport, the Session's journaling seat and the batch exchange);
  `nika-cli-host` re-exports it whole as `compile::typesafe`, so no consumer changes. The host
  crate drops from 14,995 to about 14,076 production lines. The seat's key accessor now
  returns a `Secret`, read only through an explicit `expose`.
