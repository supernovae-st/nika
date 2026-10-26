- **The embedded language pack follows nika-spec `c5dcd97`.** `SPEC_PIN`
  and `crates/nika-pack/pack` advance together from `6390b08`, the commit
  0.123.0 and 0.123.1 embed. The examples and templates that `nika try` and
  `nika compile` serve no longer call a mock rehearsal offline: the mock
  replaces the model, not the network, so a fetch step still reaches its
  host (#1605, the pack half of #1833). `release-notes` no longer drafts
  notes from a sample log when `git log` fails (the sample is now an
  explicit `--var rehearse=true`), and `pr-review-fanout` closes its fan-out
  on a « no » at the gate instead of failing. The spec chapters and stdlib
  pages carry current model ids and the 0.123 reference-release amendments.
  The schema and `canon.yaml` are unchanged.
