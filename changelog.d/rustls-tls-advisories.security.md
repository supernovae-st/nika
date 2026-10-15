- **rustls 0.23.46 closes four TLS advisories.** RUSTSEC-2026-0346 to
  RUSTSEC-2026-0349 (published 2026-10-14) affect rustls 0.23.45, which
  the workspace, `nika-acp` and fuzz lockfiles resolved: client session
  `Debug` output exposing TLS secrets, buffered data sent to the outer
  server after an ECH rejection, a Brotli certificate decompression that
  can allocate 1 GiB before authentication, and an unauthenticated
  ServerHello downgrading later connections to classical key exchange.
  All three lockfiles now resolve rustls 0.23.46.
