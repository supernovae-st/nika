- **`capture: structured` no longer turns a jail EPERM into a green run
  (#1068).** A confinement denial (`cat: …: Operation not permitted` /
  `Permission denied`, a `sandbox-exec:` / `bwrap:` line, status 126) is
  `NIKA-SEC-001` in every capture mode. A program's own non-zero stays
  data under `structured`. Seatbelt and bwrap share the same stderr table
  so Linux cannot re-open the hole the macOS path already closed.
