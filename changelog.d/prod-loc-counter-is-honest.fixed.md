- **The 15k crate-size counter was wrong for 69 of 71 crates, in both
  directions at once.** It guards a *maintainability* budget, so every line
  it mis-charges gets paid for in deleted doc comments — which is how #1203
  surfaced it. Three defects: braces inside **string literals** ended a
  `#[cfg(test)]` module early (412 lines of test body charged to production
  in one file), `#[cfg(test)] mod foo;` — an attribute on a declaration —
  swallowed whichever block came next and **hid** production lines, and a
  phantom trailing line was charged once per file. `nika-display` was
  over-charged 846 lines, `nika-cap` 733; `nika-cli` had 31 production lines
  hidden, `nika-runtime` 65. The awk filter in `_lib.sh` had already learned
  the string-literal rule in August; three python copies never did. The
  counter now lives in one proven file (`scripts/ci/prod-loc.py`) that
  refuses to render a verdict unless its own fixtures pass, and the gate
  fails closed if they are missing. Corrected, the crate nearest the wall is
  `nika-check` at 14,697.
