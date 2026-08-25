- **The listen line names the next hop.** After bind it prints
  `GET /health`, authenticated `GET /v1/openapi.json`, and
  `POST /v1/jobs` with Bearer plus Idempotency-Key. A non-loopback
  bind adds that the blast radius is every workflow in `--workflows`.
  `GET /health` JSON stays the ADR-117 identity allowlist.
