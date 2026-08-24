- **`nika doctor` uses the same token-file policy as `nika serve`.** A
  short, non-graphic, or symlink `.nika/serve.token` is a fail with the
  openssl mint, not an owner-only green. Group/world-readable still
  names `chmod 600`. The row stays silent when the file is absent.
