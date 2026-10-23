- **TLS certificate checks and constant-time helpers are past their October advisories.**
  `rustls-webpki` 0.103.16 refuses distrusted intermediates (RUSTSEC-2026-0382), `webpki-roots`
  1.0.11 drops the distrusted emSign roots (RUSTSEC-2026-0381), and `ctutils` 0.4.5 makes its mask
  generation and `u128` comparisons constant-time again (RUSTSEC-2026-0385), in the workspace,
  fuzz and ACP lockfiles.
