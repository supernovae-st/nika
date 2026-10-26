- **An oversized PNG input is a size cap, and a PNG has one IHDR.**
  `nika:image_fx` refused an input over 16384 pixels on a side as
  `NIKA-BUILTIN-IMAGE_FX-004` (corrupt, re-produce it); the stdlib files that
  input cap under `-005`, which it now returns. The PNG decoder behind
  `image_fx` and `image_compose` now refuses an IHDR that is not the first
  chunk, or a second IHDR, so the size a pixel budget judged from the header
  is always the size decoded.
