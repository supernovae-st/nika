// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Static builtin tool catalog — 25 `nika:*` tools in a sorted array.
//!
//! Case-sensitive lookup via binary search. Tool names are engine-controlled,
//! always lowercase.
//!
//! Source of truth · `nika/spec/stdlib/builtins-v0.1.md` (canonical 22 per
//! D-2026-05-22-N6 stdlib-collapse 42→26 · `jq` subsumes ~13 data builtins ·
//! `JSONPath` dropped · media DEFERRED to stdlib v0.x) + 2026-05-27 follow-on
//! `nika:json_merge` cut + ADR-086 `csv_to_json` → `convert` + ADR-087
//! `sleep` + `wait_until` → `wait` + ADR-088 `cost`+`records`+`dag_info`+
//! `threads` → `inspect` view-discriminated (`jaq` source-verified ·
//! `nika:json_merge_patch` stays for RFC-7396 null-delete which `jq *` cannot
//! express) + `nika:compose` (ADR-096) + `nika:image_generate` (stdlib
//! §Media · the first deferred-media graduate) + `nika:tts_generate`
//! (stdlib §Audio · sovereign-first).
//!
//! 6 categories · Core 6 · File 5 · Data 8 · Network 2 · Introspection 2 · Media 3 = 26.

use crate::types::builtin::{Builtin, BuiltinCategory};

use BuiltinCategory::{Core, Data, File, Introspection, Media, Network};

/// All 26 builtin tools, **sorted alphabetically by name**.
///
/// Invariant: array MUST be sorted for `binary_search` to work.
/// This is validated by a unit test.
pub static ALL_BUILTINS: &[Builtin] = &[
    Builtin::with_required("assert", Core, &["condition", "message"], &["condition"]),
    // `compose` (the agent loop's self-verification intrinsic — checks a
    // workflow draft the model wrote · `nika check`: conformance +
    // secret-flow + permits + the AARA certificate · never executes it ·
    // loop-only + loop-served like `done` · NIKA-BUILTIN-COMPOSE-001 ·
    // ADR-096 · the static sibling of `inspect`'s runtime view).
    Builtin::with_required(
        "compose",
        Introspection,
        &["workflow_yaml"],
        &["workflow_yaml"],
    ),
    Builtin::with_required(
        "convert",
        Data,
        &["input", "from", "to", "has_header", "formula_guard"],
        &["input", "from", "to"],
    ),
    Builtin::with_required(
        "date",
        Data,
        &[
            "op", "tz", "base", "duration", "input", "format", "start", "end", "unit",
        ],
        &["op"],
    ),
    // `done` (the agent-loop completion sentinel · loop-only · no required
    // args · the builtin_shape ladder rejects it as a standalone invoke).
    Builtin::with_args("done", Core, &["result"]),
    Builtin::with_required(
        "edit",
        File,
        &["path", "find", "replace", "count"],
        &["path", "find", "replace"],
    ),
    Builtin::with_required("emit", Core, &["event_type", "payload"], &["event_type"]),
    // `fetch` · `url` IS required, but `jq`/`selector` are mode-conditional —
    // the whole fetch contract (incl. `url`) is owned by `builtin_shape`'s
    // `check_fetch_shape` (the mode-pairing logic), so required stays `&[]`
    // here to avoid a double finding.
    Builtin::with_args(
        "fetch",
        Network,
        &[
            "url",
            "method",
            "headers",
            "body",
            "form",
            "multipart",
            "traverse",
            "mode",
            "selector",
            "jq",
        ],
    ),
    Builtin::with_required("glob", File, &["pattern", "exclude"], &["pattern"]),
    Builtin::with_required(
        "grep",
        File,
        &["pattern", "path", "case_insensitive"],
        &["pattern"],
    ),
    Builtin::with_required("hash", Data, &["content", "algo", "encoding"], &["content"]),
    // `image_fx` (stdlib §Media · deferred-media graduate #3: the `image
    // editing` row · deterministic pure transform — byte-identical artifacts,
    // recipe-in-chunk, NIKA-BUILTIN-IMAGE_FX-001..006 · FX master plan).
    Builtin::with_required(
        "image_fx",
        Media,
        &["input", "out", "ops", "seed"],
        &["input", "out", "ops"],
    ),
    // `image_generate` (stdlib §Media · the first deferred-media graduate ·
    // local/openai/gemini/xai/mock — local-first per sovereignty Rule 3 ·
    // assets land on disk, outputs carry paths+hashes, never base64 ·
    // NIKA-BUILTIN-IMAGE_GENERATE-001..007).
    Builtin::with_required(
        "image_generate",
        Media,
        &[
            "provider",
            "model",
            "prompt",
            "mode",
            "image",
            "images",
            "mask",
            "n",
            "aspect_ratio",
            "size",
            "quality",
            "format",
            "compression",
            "background",
            "seed",
            "reference_images",
            "provider_options",
            "output_dir",
            "filename_prefix",
            "save",
            "manifest",
            "metadata",
            "timeout_ms",
            "debug",
        ],
        &["prompt", "output_dir"],
    ),
    Builtin::with_required("inspect", Introspection, &["view"], &["view"]),
    Builtin::with_required("jq", Data, &["expression", "input"], &["expression"]),
    Builtin::with_required(
        "json_diff",
        Data,
        &["before", "after"],
        &["before", "after"],
    ),
    Builtin::with_required(
        "json_merge_patch",
        Data,
        &["target", "patch"],
        &["target", "patch"],
    ),
    Builtin::with_required("log", Core, &["level", "message", "data"], &["message"]),
    Builtin::with_required(
        "notify",
        Network,
        &["channel", "target", "message", "severity", "data"],
        &["target", "message"],
    ),
    Builtin::with_required(
        "prompt",
        Core,
        &["mode", "message", "choices", "default"],
        &["message"],
    ),
    Builtin::with_required("read", File, &["path", "binary"], &["path"]),
    // `tts_generate` (stdlib §Audio · the second Media-class builtin ·
    // local/openai/elevenlabs/mock — sovereign-first · ONE audio file on
    // disk, output carries path+sha256+duration, never bytes ·
    // NIKA-BUILTIN-TTS_GENERATE-001..007).
    Builtin::with_required(
        "tts_generate",
        Media,
        &[
            "provider",
            "model",
            "text",
            "voice",
            "format",
            "speed",
            "output_dir",
            "filename_prefix",
            "manifest",
            "metadata",
            "timeout_ms",
        ],
        &["text", "output_dir"],
    ),
    Builtin::with_args("uuid", Data, &["version"]),
    Builtin::with_required(
        "validate",
        Data,
        &["data", "schema", "format"],
        &["data", "schema"],
    ),
    // `wait` · `duration` XOR `until` — a CONDITIONAL exactly-one-of, not a
    // flat required set · `builtin_shape` owns it (required stays `&[]`).
    Builtin::with_args("wait", Core, &["duration", "until", "timeout"]),
    Builtin::with_required(
        "write",
        File,
        &["path", "content", "overwrite", "create_dirs"],
        &["path", "content"],
    ),
];

/// Find a builtin tool by name (case-sensitive, O(log n) binary search).
#[must_use]
pub fn find_builtin(name: &str) -> Option<&'static Builtin> {
    ALL_BUILTINS
        .binary_search_by_key(&name, |b| b.name)
        .ok()
        .map(|i| &ALL_BUILTINS[i])
}

/// Check if a tool name (without `nika:` prefix) is a known builtin.
#[must_use]
pub fn is_known_builtin(name: &str) -> bool {
    ALL_BUILTINS.binary_search_by_key(&name, |b| b.name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_count() {
        assert_eq!(ALL_BUILTINS.len(), 26);
    }

    #[test]
    fn sorted_order() {
        for pair in ALL_BUILTINS.windows(2) {
            assert!(
                pair[0].name < pair[1].name,
                "builtins not sorted: `{}` >= `{}`",
                pair[0].name,
                pair[1].name
            );
        }
    }

    #[test]
    fn find_known_builtins() {
        // Sample across all 6 categories.
        let names = ["wait", "read", "jq", "fetch", "inspect", "image_generate"];
        for name in names {
            assert!(find_builtin(name).is_some(), "builtin `{name}` not found");
        }
    }

    #[test]
    fn is_known_builtin_works() {
        // Spec-canonical builtins are known.
        assert!(is_known_builtin("wait"));
        assert!(is_known_builtin("jq"));
        assert!(is_known_builtin("validate"));
        assert!(is_known_builtin("notify"));
        assert!(is_known_builtin("inspect"));
        // Unknown + legacy (cut per D-N6 + ADR-086/087/088) return false.
        assert!(!is_known_builtin("typo_tool"));
        assert!(!is_known_builtin("json_query")); // deprecated, removed
        assert!(!is_known_builtin("map")); // legacy, subsumed by jq
        assert!(!is_known_builtin("json_merge")); // legacy, subsumed by jq
        assert!(!is_known_builtin("pipeline")); // legacy, media DEFERRED
        assert!(!is_known_builtin("sleep")); // ADR-087, merged into wait
        assert!(!is_known_builtin("wait_until")); // ADR-087, merged into wait
        assert!(!is_known_builtin("csv_to_json")); // ADR-086, replaced by convert
        assert!(!is_known_builtin("cost")); // ADR-088, merged into inspect
        assert!(!is_known_builtin("records")); // ADR-088, merged into inspect
        assert!(!is_known_builtin("dag_info")); // ADR-088, merged into inspect
        assert!(!is_known_builtin("threads")); // ADR-088, merged into inspect
    }

    #[test]
    fn unknown_returns_none() {
        assert!(find_builtin("nonexistent").is_none());
        assert!(find_builtin("").is_none());
    }

    #[test]
    fn case_sensitive() {
        // Builtins are case-sensitive — uppercase should NOT match
        assert!(find_builtin("Sleep").is_none());
        assert!(find_builtin("JQ").is_none());
    }

    #[test]
    fn all_builtins_have_non_empty_names() {
        for builtin in ALL_BUILTINS {
            assert!(!builtin.name.is_empty());
        }
    }

    #[test]
    fn category_counts() {
        let core = ALL_BUILTINS.iter().filter(|b| b.category == Core).count();
        let file = ALL_BUILTINS.iter().filter(|b| b.category == File).count();
        let data = ALL_BUILTINS.iter().filter(|b| b.category == Data).count();
        let network = ALL_BUILTINS
            .iter()
            .filter(|b| b.category == Network)
            .count();
        let intro = ALL_BUILTINS
            .iter()
            .filter(|b| b.category == Introspection)
            .count();
        let media = ALL_BUILTINS.iter().filter(|b| b.category == Media).count();
        assert_eq!(
            core, 6,
            "expected 6 core builtins (post-ADR-087 wait merge)"
        );
        assert_eq!(file, 5, "expected 5 file builtins");
        assert_eq!(data, 8, "expected 8 data builtins");
        assert_eq!(network, 2, "expected 2 network builtins");
        assert_eq!(
            intro, 2,
            "expected 2 introspection builtins (inspect runtime · compose static · ADR-096)"
        );
        assert_eq!(
            media, 3,
            "expected 3 media builtins (image_generate §Media · tts_generate §Audio · image_fx §Media)"
        );
        assert_eq!(
            core + file + data + network + intro + media,
            26,
            "total must equal 26"
        );
    }

    #[test]
    fn every_builtin_declares_at_least_one_arg_key_with_no_dupes() {
        // The arg vocabulary `nika check` validates against — every builtin
        // in stdlib v0.1 takes args, and a key listed twice would be a
        // copy-paste slip in the table.
        for b in ALL_BUILTINS {
            assert!(!b.args.is_empty(), "`{}` declares no args", b.name);
            let mut seen = b.args.to_vec();
            seen.sort_unstable();
            let before = seen.len();
            seen.dedup();
            assert_eq!(seen.len(), before, "`{}` has a duplicate arg key", b.name);
        }
    }

    #[test]
    fn required_is_always_a_subset_of_args() {
        // Every unconditionally-required key must be a DECLARED arg key —
        // a `required` outside `args` would be an unsatisfiable contract.
        for b in ALL_BUILTINS {
            for req in b.required {
                assert!(
                    b.args.contains(req),
                    "`{}` requires `{req}` which is not in its args",
                    b.name
                );
            }
            let mut seen = b.required.to_vec();
            seen.sort_unstable();
            let before = seen.len();
            seen.dedup();
            assert_eq!(
                seen.len(),
                before,
                "`{}` has a duplicate required key",
                b.name
            );
        }
    }

    #[test]
    fn conditional_required_builtins_declare_no_flat_required() {
        // `wait` (duration XOR until) and `fetch` (mode-dependent jq/selector
        // + url) carry their contracts in builtin_shape, NOT the flat
        // `required` set — so the two checks never double-report.
        for name in ["wait", "fetch"] {
            let b = find_builtin(name).expect("known");
            assert!(
                b.required.is_empty(),
                "`{name}` must keep a flat-empty required (conditional contract)"
            );
        }
    }

    #[test]
    fn jq_declares_input_not_data() {
        // The footgun anchor: `nika:jq` reads `input:`, NOT `data:` — the
        // checker leans on this row to catch the silent-null typo.
        let jq = find_builtin("jq").expect("jq");
        assert!(jq.args.contains(&"input"));
        assert!(!jq.args.contains(&"data"));
    }
}
