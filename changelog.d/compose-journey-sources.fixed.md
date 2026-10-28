- **The data journey counts what `nika:image_compose` reads.** Its journey
  read only `out:`, so a contact sheet built from
  `./customers/passport-scan.png` showed `0 sources` and stayed `internal`
  while a `nika:read` of the same file made the journey sensitive. Each
  literal `inputs[]` path is now a source, judged from the same slots the
  `PERMITS` lane checks; a templated list names none.
