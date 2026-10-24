- **Big-integer arithmetic is past its October constant-time advisory.** `crypto-bigint` 0.7.6
  keeps subtraction, zero counting, division and Montgomery multiplication free of secret-dependent
  branches (RUSTSEC-2026-0387), in the workspace and fuzz lockfiles.
