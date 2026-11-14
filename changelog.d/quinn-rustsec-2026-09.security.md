- **`quinn-proto` and `quinn-udp` are past three QUIC advisories.** In
  `quinn-proto` 0.11.18 a `MAX_STREAM_DATA` frame could open streams beyond
  the stream limit and a long Retry token could panic or exhaust memory in
  a client (RUSTSEC-2026-0523, RUSTSEC-2026-0524); in `quinn-udp` before
  0.5.16 the receive loop could spin after a truncated datagram
  (RUSTSEC-2026-0538). The lockfiles carry both crates only through the
  optional `http3` feature of `reqwest`, which no crate here enables, so
  no build of the engine compiled them; the audit reads every lockfile,
  though, and the advisories refused each push. The three lockfiles (the
  root, `fuzz/` and `crates/nika-acp/`) move to 0.11.19 and 0.5.16, and
  nothing else in them changes.
