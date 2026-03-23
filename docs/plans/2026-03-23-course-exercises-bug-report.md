# Bug Report: Course Exercise Validation Failures

**Date:** 2026-03-23
**Scope:** `nika init --course` — 44 exercises across 12 levels
**Files:** `tools/nika-engine/src/init/course/exercises.rs`, `exercises_advanced.rs`

## Summary

Running `nika check` on course exercise templates produced NIKA-005 errors ("null is not of type array") because 8 templates had `tasks:` with only TODO comments and no actual YAML array items. Additionally, 1 solution had wrong retry field names.

## Bugs Found & Fixed

### BUG-1: CRITICAL — 8 templates with null `tasks:` array

**Root cause:** When `tasks:` is followed by only `# TODO` comments with no YAML list items (`- id: ...`), YAML parses `tasks` as `null`. The JSON schema requires `tasks` to be a non-empty array (`type: array, minItems: 1`), so `nika check` fails with NIKA-005.

**Affected templates:**

| Exercise | File | Issue |
|----------|------|-------|
| L1-01 Hello World | exercises.rs | `tasks:` null + missing `schema:` and `workflow:` |
| L1-02 Shell Commands | exercises.rs | `tasks:` null |
| L1-03 HTTP Requests | exercises.rs | `tasks:` null |
| L1-04 Provider Selection | exercises.rs | `tasks:` null |
| L1-05 Validation & DAG | exercises.rs | `tasks:` null |
| L3-02 For Each Basic | exercises.rs | `tasks:` null |
| L3-03 For Each Concurrent | exercises.rs | `tasks:` null |
| L3-04 Chained Pipeline | exercises.rs | `tasks:` null |

**Fix applied:** Added a placeholder stub task to each:
```yaml
tasks:
  # ↓ Replace this starter task with your solution ↓
  - id: starter
    exec: "echo 'Replace me with your solution!'"
```

For L1-01 specifically, also added pre-filled `schema:` and `workflow:` (changed TODOs to "understand" rather than "add").

**Why 36 other templates passed:** They already included at least one real task item before the TODO comments (e.g., L2-01 gives `get_date` and `get_hostname` as starters).

### BUG-2: MEDIUM — L11-02 wrong `retry:` field names

**Root cause:** Both template AND solution used `delay_ms` and `backoff` instead of the correct `backoff_ms` and `multiplier` (per `RetryConfig` struct in `ast/action.rs`).

**Affected:** `exercises_advanced.rs` lines 1610-1611 (template) and 1673-1674 (solution)

**Impact:** The wrong field names are silently ignored (serde doesn't deny unknown fields), so the retry uses defaults which happen to match (1000ms, 2.0x). Functionally works but teaches learners **wrong API names**.

**Fix applied:**
```yaml
# Before (wrong)
retry:
  max_attempts: 2
  delay_ms: 1000
  backoff: 2.0

# After (correct)
retry:
  max_attempts: 2
  backoff_ms: 1000
  multiplier: 2.0
```

## Non-Blocking Observations

### Templates with TODO strings in numeric fields (informational)

Level 8 and 12 templates use `"TODO: number"` strings for fields expecting integers (`max_turns`, `concurrency`, `min_words`, etc.). These cause deserialization errors rather than helpful validation messages. However, this is by design — the learner must fix all TODOs.

**Affected:** L8-01, L8-02, L8-03, L12-03, L12-04, L12-05

### L4-02 naming mismatch (cosmetic)

Exercise registered as "Imports" (filename `02-imports.nika.yaml`) but content teaches "Multi-Step Data Pipeline". The `include:` concept is not taught in this exercise.

## All Solutions Status

| Level | Solutions | Status |
|-------|-----------|--------|
| L1 Jailbreak (5) | All 5 | PASS |
| L2 Hot Wire (4) | All 4 | PASS |
| L3 Fork Bomb (4) | All 4 | PASS |
| L4 Root Access (3) | All 3 | PASS |
| L5 Shapeshifter (3) | All 3 | PASS |
| L6 Pay-Per-Dream (3) | All 3 | PASS |
| L7 Swiss Knife (3) | All 3 | PASS |
| L8 Gone Rogue (3) | All 3 | PASS |
| L9 Data Heist (4) | All 4 | PASS |
| L10 Open Protocol (3) | All 3 | PASS |
| L11 Pixel Pirate (4) | 3/4 | L11-02 had wrong retry fields (FIXED) |
| L12 SuperNovae (5) | All 5 | PASS |

**Total: 44/44 solutions now valid**
