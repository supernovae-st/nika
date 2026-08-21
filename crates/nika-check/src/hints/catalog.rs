// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Teach a hint identity — the token `nika check` prints in `[brackets]`.
//!
//! A finding carries `code` (`NIKA-PARSE-019`). A hint carries `kind`
//! (`jq-as-map`) and sometimes a numbered `code` (`native-first/006`).
//! The renderer puts whichever it has in the same slot. `nika explain`
//! used to resolve codes only, so the next gesture after a HINT row
//! 404'd (#1038). This table is the other half of that slot.

/// Teaching text for a hint identity `nika check` prints in `[brackets]`.
/// `None` for a token that is not a hint — callers then try the error
/// registries.
#[must_use]
pub fn hint_help(token: &str) -> Option<&'static str> {
    ROWS.iter()
        .find(|(identity, _)| *identity == token)
        .map(|(_, help)| *help)
}

/// Closed set of identities the check renderer can put in the HINT
/// slot (`hint.code.unwrap_or(hint.kind)`). Adding a kind or a numbered
/// native-first rule without a row here is the defect this file exists
/// to close.
const ROWS: &[(&str, &str)] = &[
    (
        "cost",
        "an `infer:`/`agent:` task with no token bound — add `max_tokens:` \
         (or `max_tokens_total:`) and the cost report becomes a hard ceiling",
    ),
    (
        "zero-cap",
        "a declared `max_tokens: 0` / `max_tokens_total: 0` is arithmetically \
         a $0 ceiling and a call no provider will honor — raise it or drop the field",
    ),
    (
        "thinking-budget",
        "a reasoning-capable model seated with `max_tokens` but no `thinking:` \
         — the reasoning share lives inside that budget, and a heavy think ends \
         in a paid blank (NIKA-INFER-004 at run). Declare `thinking:`",
    ),
    (
        "dead-spend",
        "a pure `infer:` whose output nobody reads — every token it spends is \
         dead. Bind it, put it in `outputs:`, or delete the task",
    ),
    (
        "typing",
        "a task whose output is deeply referenced (`tasks.X.output.field`) \
         but declares no `schema:` / `output:` — declare a shape and the \
         dataflow typer starts proving those references",
    ),
    (
        "permits",
        "no `permits:` block, and the body is pure compute (F-O8 legal zero) \
         — declare `permits: {}` to state the zero explicitly",
    ),
    (
        "strictness",
        "an object schema admitting undeclared keys — close it \
         (`additionalProperties: false`) and the output shape is deterministic",
    ),
    (
        "schema-portability",
        "a schema keyword no provider grammar enforces — the check accepts it; \
         constrained decoding at run will not. Prefer keywords the catalog's \
         grammars actually apply",
    ),
    (
        "redundant-gate",
        "`after: {x: terminal}` beside a value edge to `x` changes nothing \
         (edges compose by intersection). Tighten to `success` or drop the entry",
    ),
    (
        "retry-effects",
        "`retry:` on a task whose effects are not contracted as idempotent — \
         a second attempt can double a write or a send. Drop retry, or make \
         the effect safe to repeat",
    ),
    (
        "secrets-store",
        "a referenced `secrets.X` whose source is not yet runtime-resolvable \
         — check is green, run fails NIKA-1702. Use `source: env` or \
         `source: file` until vault resolution ships",
    ),
    (
        "native-first",
        "an `exec:` whose literal command a stdlib builtin (or an MCP tool) \
         covers. The numbered rules `native-first/001..006` name the family \
         (http · file · data · media · helper · utility). Advisory unless \
         `nika check --native-strict`",
    ),
    (
        "native-first/001",
        "`exec:` of `curl`/`wget`/`xh`/`http(s)` (or an interpreter one-liner \
         around `fetch(`/`axios`) — use `nika:fetch` (`multipart:` · \
         `traverse:`). `nika check --native-strict` promotes this to a finding",
    ),
    (
        "native-first/002",
        "`exec:` of `cat`/`tee`/`cp`/`mv`/`mkdir`/`touch`/`head`/`tail`/`ls` \
         — use `nika:read` / `nika:write` (`create_dirs: true`) / `nika:glob`. \
         `nika check --native-strict` promotes this to a finding",
    ),
    (
        "native-first/003",
        "`exec:` of `jq`/`sed`/`awk` — use `nika:jq` (or an `extract:` binding) \
         for JSON, `nika:edit` for in-place literal file edits. \
         `nika check --native-strict` promotes this to a finding",
    ),
    (
        "native-first/004",
        "`exec:` of an image/speech provider endpoint — use \
         `nika:image_generate` / `nika:tts_generate`. \
         `nika check --native-strict` promotes this to a finding",
    ),
    (
        "native-first/005",
        "`exec:` of an interpreter (`node` · `python` · `sh` · …) running a \
         script file — inventory the helper (HTTP→fetch · files→read/write · \
         JSON→jq · a product API→MCP) and keep only a genuine subprocess. \
         `nika check --native-strict` promotes this to a finding",
    ),
    (
        "native-first/006",
        "a utility with an exact builtin: `sleep`→`nika:wait` (`duration:`) · \
         `date`→`nika:date` (`op:`) · `uuidgen`→`nika:uuid` · \
         `sha256sum`→`nika:hash` (`algo:`) · `yq`→`nika:convert` · \
         `grep`/`rg`/`ag`→`nika:grep` · `find`→`nika:glob`. Names the builtin \
         AND its argument shape. `nika check --native-strict` promotes this \
         to a finding",
    ),
    (
        "exec-json-capture",
        "`capture: structured` plus a binding that parses `.stdout | fromjson` \
         and no binding reads `exit_code`/`stderr` — for a JSON-producing \
         helper use `capture: stdout` so a non-zero exit fails as NIKA-EXEC-001 \
         instead of becoming data",
    ),
    (
        "swallowed-exit",
        "`capture: structured` makes a non-zero exit DATA, not a failure, and \
         nothing reads `exit_code` — a failing command here reports success. \
         Read `exit_code` or switch capture",
    ),
    (
        "unwrapped-ref",
        "an `outputs:` value that spells a reference path without `${{ }}` \
         rides as the literal string (the run returns the path text, not the \
         value). Wrap it",
    ),
    (
        "envelope-output",
        "an `outputs:` binding referencing a bare `tasks.X` captures the whole \
         envelope (status · timestamps · output), so goldens drift. Bind \
         `tasks.X.output` for the value",
    ),
    (
        "run-clock",
        "a task `timeout:` whose time source the envelope never names — the \
         deadline rides the ambient system clock. Declare `run: { clock: … }` \
         to pin the choice (F-P3)",
    ),
    (
        "analysis",
        "past the analysis task cap the width/pinch/blast read and the pair \
         scan of the write-write law are skipped (the O(n²) DoS floor) — the \
         skip is stated, never silent",
    ),
    (
        "consent",
        "an egress-capable descendant of a confirm-mode `nika:prompt` sits \
         behind a gate the checker cannot prove consumes the answer. The \
         proven non-affirmative route is NIKA-SEC-014; this hint is the \
         undecidable remainder. Teach `with: go` + `when:`",
    ),
    (
        "headless-prompt",
        "`nika:prompt` declares no `default:` — unattended (CI, an agent) \
         there is no answer. Declare the `default:` the unattended path \
         should take, or keep the gate attended-only",
    ),
    (
        "fail-open-consent",
        "`nika:prompt` carries `default: true` and the file has an effect — \
         unattended, the engine answers `true` and the effect fires with no \
         human (spec 06). `default: false` refuses; omitting `default:` parks",
    ),
    (
        "digit-string-enum",
        "a string enum of digits only — models emit JSON numbers (`3` not \
         `\"3\"`); constrained decoding can reject the call before Nika \
         stringifies. Prefer `type: integer` with a numeric enum. \
         A paid-run hint: `is_clean` stays green, `paid_ready` does not",
    ),
    (
        "glob-readme",
        "`nika:glob` of a markdown pattern includes a README in the same \
         directory — the next infer classifies the table of contents as a \
         record. `exclude: \"**/README.md\"`. A paid-run hint: `is_clean` \
         stays green, `paid_ready` does not",
    ),
    (
        "jq-as-map",
        "`nika:jq` binds `. as $name` then calls `map(` on the current value \
         — after a later construct that value is often a pair, not the bound \
         array. Write `($name | map(...))`. A paid-run hint: `is_clean` stays \
         green, `paid_ready` does not",
    ),
    (
        "infer-as-law",
        "an `infer:` asks the model to name a belt/level/score — that is the \
         law, not a fact. Extract integer facts, then `nika:jq` or \
         `nika:decide`. A paid-run hint: `is_clean` stays green, `paid_ready` \
         does not",
    ),
    (
        "unproven-law",
        "`nika:jq` / `nika:decide` scores an infer extract and no \
         const-fixture `nika:assert` proves the law on known answers. \
         `is_clean` does not compile the law. A paid-run hint: `paid_ready` \
         is false until an assert pins it",
    ),
    (
        "assert-quarantine",
        "a red `nika:assert` quarantines writes to \
         `.nika/quarantine/<trace>/` — authors hunt an empty `out/` otherwise",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_printed_jq_as_map_slot_has_a_teaching() {
        // #1038 · the measured case: check prints `[jq-as-map]`, explain 404'd.
        let help = hint_help("jq-as-map").expect("jq-as-map is a printed identity");
        assert!(help.contains("($name | map"), "{help}");
        assert!(help.contains("paid_ready"), "{help}");
    }

    #[test]
    fn a_numbered_native_first_rule_has_a_teaching() {
        let help = hint_help("native-first/006").expect("006 is a printed identity");
        assert!(help.contains("nika:wait"), "{help}");
        assert!(help.contains("duration:"), "{help}");
    }

    #[test]
    fn every_catalogued_identity_explains_and_unknown_stays_none() {
        assert!(hint_help("ghost-hint").is_none());
        assert!(hint_help("NIKA-PARSE-019").is_none());
        for (identity, help) in ROWS {
            assert_eq!(hint_help(identity), Some(*help), "{identity}");
            assert!(!help.is_empty(), "{identity} must teach something");
        }
        let identities: Vec<&str> = ROWS.iter().map(|(id, _)| *id).collect();
        assert!(identities.contains(&"jq-as-map"));
        assert!(identities.contains(&"native-first/001"));
        assert!(identities.contains(&"native-first/006"));
        let mut sorted = identities.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), identities.len(), "duplicate hint identities");
    }
}
