- **`nika doctor` says "unknown" when it could not see.** A harness seat
  whose login check did not answer in time was a warning; it is now its own
  level, `unknown` (glyph `?`), counted apart in the verdict line and in
  `--json` as `summary.unknown`, never as ok, warn or fail. The exit code
  is unchanged: an unknown row never makes `nika doctor` exit 3.
