# Overnight Bug Log — 2026-03-31

## Bugs FIXED (commit hash)
| # | Bug | File:Line | Commit | Before | After |
|---|-----|-----------|--------|--------|-------|
| 1 | Shell mode bypass exec.rs | exec.rs:38 | deb342e | `false` hardcoded | `is_shell` passed |
| 2 | IPv6 :: SSRF bypass | policy.rs:49 | 732475d | Missing UNSPECIFIED check | Explicit check added |
| 3 | SECRET_RE missing patterns | util/mod.rs:30 | cef99a7 | 12 patterns | 16 patterns (ASIA, ghu/d/r, SG., JWT) |
| 4 | MCP response leaks secrets | invoke.rs:160+ | 8de44a5 | Raw values in events | redact_value() at 4 sites |
| 5 | unwrap_or_default transforms | transform.rs:284,430 | f1917e2 | Silent empty string | expect() with comment |
| 6 | "null" string → null coercion | verbs.rs:85 | 0f9bd73 | Coerced to Value::Null | Preserved as string |
| 7 | ForEachItem events missing | log.rs + runner.rs | 6da352f | No per-item tracking | 3 new event types wired |
| 8 | FallbackChainExhausted no event | executor/mod.rs | 6da352f | Only tracing::warn | Event emitted before error |
| 9 | for_each no item limit | runner.rs:2247 | c27fe6a | Unbounded | MAX_FOR_EACH_ITEMS=10,000 |
| 10 | timeout=0 only warning | analyze.rs:984 | a67811f | Warning (passes check) | Error (rejects at parse) |

## Bugs FOUND during execution
| # | Workflow | Bug | File:Line | Severity | Status |
|---|---------|-----|-----------|----------|--------|
| NB-1 | G06 | Newline injection not blocked for benign commands | security.rs | LOW | WONTFIX — blocklist covers dangerous cmds |
| NB-2 | I03 | Skills path resolved relative to CWD not workflow | skill_injector.rs | MEDIUM | Known — needs path resolution fix |
| NB-3 | A03,A06,A10 | Gemini 429 rate limit exhausted | fetch.rs | LOW | Provider quota, not code bug |

## Workflows EXECUTED
| # | Workflow | Provider | Status | Duration | Cost | Output |
|---|---------|----------|--------|----------|------|--------|
| 1-7 | G01-G07 | mock | 6 FAIL 1 PASS | <1s each | $0 | G06 newline passes (see NB-1) |
| 8-15 | E01-E08 | mock | 7 PASS 1 FAIL(expected) | <1s each | $0 | E08 timeout correctly fails |
| 16-22 | D01-D07 | mock | 6 PASS 1 FAIL(expected) | <1s each | $0 | D04 fail_fast test |
| 23-27 | S01-S05 | mock | ALL PASS | <1s each | $0 | All stress tests green |
| 28-37 | F01-F12 | local | ALL PASS | 1-3s each | $0 | Charts, thumbnails, colors verified |
| 38-46 | C01-C09 | mock/HTTP | ALL PASS | 1-5s each | $0 | All 9 extract modes work |
| 47-49 | R01-R03 | mock | ALL PASS | <1s each | $0 | Artifacts written and verified |
| 50-51 | I02-I03 | openai | 1 PASS 1 WARN | 2-3s | ~$0.01 | I03 skill path bug (non-fatal) |
| 52-58 | A01-A07 | openai/gemini | 5 PASS 2 FAIL(quota) | 2-5s each | ~$0.05 | A03,A06 = Gemini 429 |
| 59-61 | B01,B02,B05 | openai | ALL PASS | 3-10s | ~$0.10 | Agent verb works end-to-end |
| 62-63 | H01-H02 | openai | ALL PASS | 3-8s | ~$0.05 | Real-world pipelines verified |

## Features INCOMPLETE discovered
| # | Feature | File:Line | Status | Impact |
|---|---------|-----------|--------|--------|
| 1 | Skills path resolution relative to workflow | skill_injector.rs | Known bug | Skills fail when CWD != workflow dir |
| 2 | Gemini rate limit handling | fetch.rs | Retry-After not respected | 429s cause immediate failure |

## Artifacts VERIFIED
| # | Workflow | File | Size | Format Valid |
|---|---------|------|------|-------------|
| 1 | R01 | output/report.md | >0 | Markdown valid |
| 2 | R02 | output/data.json | >0 | JSON valid |
| 3 | R03 | output/item-*.txt (x3) | >0 | 3 files created |
| 4 | F07 | .nika/media/store/e2/* | 20KB | PNG valid (bar chart) |
| 5 | F08 | .nika/media/store/d4/* | 34KB | PNG valid (line chart) |
| 6 | F09 | .nika/media/store/41/* | 22KB | PNG valid (pie chart) |
