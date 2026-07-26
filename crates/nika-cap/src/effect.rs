// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The fine-grained builtin effect classification — WHICH capability a
//! builtin carries and WHICH arg names its target.
//!
//! PURE DATA over `(tool, args)` — the shape.rs convention. Extracted
//! from `nika-schema::check::permits_fit` (2026-07-14 · W4: the crate
//! hit its 15k prod budget again — the same pressure that extracted the
//! permits half on 2026-07-03 and the arg-shape rules on 2026-07-07).
//! Both effect tables now live side by side in this crate: this
//! fine-grained boundary table (escape checking · capability inference)
//! and the COARSE policy projection ([`crate::EffectClass::classify`] ·
//! spec 10) — one home, so the two can never drift apart unseen (the
//! coherence test below pins their overlap).
//!
//! Ground truth: spec `stdlib/builtins-v0.1.md` (File builtins ·
//! Network builtins · Media builtins).

/// The statically-checkable effect signature of a builtin tool — the ONE
/// classification table both the escape checker and capability inference
/// read, so verification and inference cannot drift.
///
/// `nika:glob` is deliberately ABSENT: its arg is itself a glob `pattern:`,
/// and glob-pattern ⊆ permits-glob inclusion is not soundly decidable
/// statically — the runtime `NIKA-SEC-004` owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinEffect {
    /// An HTTP egress whose host comes from a literal URL-shaped arg.
    Net {
        /// The arg key carrying the URL (`url` for fetch · `target` for
        /// webhook notify).
        url_arg: &'static str,
    },
    /// A filesystem effect on a literal path-carrying arg.
    Fs {
        /// The arg key carrying the path (`path` for the file builtins ·
        /// `output_dir` for `nika:image_generate`).
        path_arg: &'static str,
        /// The effect reads the path (read · grep · edit's find phase).
        reads: bool,
        /// The effect writes the path (write · edit's replace phase).
        writes: bool,
        /// The effect descends under the path (`nika:grep` is a recursive
        /// reader · `nika:image_generate` lands files INSIDE the dir) —
        /// inference grants `<path>/**`, not just the path.
        recursive: bool,
    },
}

/// Classify a builtin invoke's statically-checkable effect, `None` for
/// pure-compute builtins (log · jq · hash · …) and for MCP tools (their
/// effects are server-side — the `tools:` grant is the boundary).
/// NEP-0003 · the pure-internal builtins (mirrors `canon/builtins.yaml`
/// LAW-AUTH-0311): no fs/net/program/tool egress by construction, so under
/// an ABSENT `permits:` block they require nothing — they are the « pure
/// compute » class the legal zero admits. Under a DECLARED block the
/// default-deny still governs them (the ratified asymmetry).
pub const PURE_INTERNAL_TOOLS: &[&str] = &[
    "nika:assert",
    "nika:compose",
    "nika:convert",
    "nika:date",
    "nika:decide",
    "nika:done",
    "nika:emit",
    "nika:hash",
    "nika:inspect",
    "nika:jq",
    "nika:json_diff",
    "nika:json_merge_patch",
    "nika:log",
    "nika:prompt",
    "nika:uuid",
    "nika:validate",
    "nika:wait",
];

/// NEP-0003 · is `tool` in the pure-internal class (no authority needed
/// under an absent block)?
#[must_use]
pub fn is_pure_internal(tool: &str) -> bool {
    PURE_INTERNAL_TOOLS.contains(&tool)
}

#[must_use]
pub fn builtin_effect(tool: &str, args: Option<&serde_json::Value>) -> Option<BuiltinEffect> {
    match tool {
        "nika:fetch" => Some(BuiltinEffect::Net { url_arg: "url" }),
        // notify is a net egress ONLY on the webhook channel (a literal
        // `target` URL); other channels ride engine-configured transports,
        // not a workflow-declared host. A dynamic channel is unclassifiable
        // statically (runtime concern).
        "nika:notify" if literal_str(args, "channel").as_deref() == Some("webhook") => {
            Some(BuiltinEffect::Net { url_arg: "target" })
        }
        "nika:read" => Some(BuiltinEffect::Fs {
            path_arg: "path",
            reads: true,
            writes: false,
            recursive: false,
        }),
        "nika:grep" => Some(BuiltinEffect::Fs {
            path_arg: "path",
            reads: true,
            writes: false,
            recursive: true,
        }),
        "nika:write" => Some(BuiltinEffect::Fs {
            path_arg: "path",
            reads: false,
            writes: true,
            recursive: false,
        }),
        // in-place find/replace reads the bytes, then rewrites the path
        "nika:edit" => Some(BuiltinEffect::Fs {
            path_arg: "path",
            reads: true,
            writes: true,
            recursive: false,
        }),
        // Media generators: assets (+ manifest) land INSIDE a literal
        // `output_dir:` — a recursive write (stdlib §Media · provider
        // egress rides the engine's media plane, not permits.net.http,
        // exactly like `infer:`). tts follows the image_generate shape.
        "nika:image_generate" | "nika:tts_generate" => Some(BuiltinEffect::Fs {
            path_arg: "output_dir",
            reads: false,
            writes: true,
            recursive: true,
        }),
        // Single-artifact writers: the WRITE side (`out:`) is the
        // statically-checkable effect. image_fx's `input:` read is
        // runtime-gated inside the builtin (the image edit-mode precedent
        // · one path_arg per effect); chart was INVISIBLE here until
        // 2026-07-11 — the inference wrote a boundary the run then
        // refused (the self-refusing class); its `compile_to` vega
        // sibling rides `chart_vl_sibling`.
        "nika:image_fx" | "nika:chart" => Some(BuiltinEffect::Fs {
            path_arg: "out",
            reads: false,
            writes: true,
            recursive: false,
        }),
        // decide is pure compute (spec 11 §nika:decide); its ONE
        // statically-visible effect is a literal string `bundle:` — a path
        // read declared like any fs.read. An inline object bundle needs no
        // filesystem at all; a templated path is runtime business
        // (NIKA-SEC-004 gates it there).
        "nika:decide" => literal_str(args, "bundle").map(|_| BuiltinEffect::Fs {
            path_arg: "bundle",
            reads: true,
            writes: false,
            recursive: false,
        }),
        _ => None,
    }
}

/// Can this builtin realize an EXTERNAL effect — net or fs-write (NEP-0002
/// v2.0's egress classification, beside the ONE effect table so the two
/// cannot drift)? `nika:notify` counts without a literal channel arg: an
/// agent picks the webhook channel at runtime, which no static reading of
/// a WHITELIST can see (conservative — the arg-reading call sites use
/// [`builtin_effect`] directly).
#[must_use]
pub fn builtin_egresses(tool: &str) -> bool {
    if tool == "nika:notify" {
        return true;
    }
    match builtin_effect(tool, None) {
        Some(BuiltinEffect::Net { .. }) => true,
        Some(BuiltinEffect::Fs { writes, .. }) => writes,
        None => false,
    }
}

/// `nika:chart` with `compile_to: vega_lite` writes a SECOND gated file —
/// the `.vl.json` sibling next to the svg. One derivation, shared by the
/// escape scan and the boundary inference, matching the runtime byte for
/// byte (`chart.rs`: `format!("{}.vl.json", out.trim_end_matches(".svg"))`).
/// `None` when the tool isn't chart, either arg is dynamic/absent, or the
/// compile target isn't the closed set's `vega_lite`.
#[must_use]
pub fn chart_vl_sibling(tool: &str, args: Option<&serde_json::Value>) -> Option<String> {
    if tool != "nika:chart" {
        return None;
    }
    if literal_str(args, "compile_to").as_deref() != Some("vega_lite") {
        return None;
    }
    let out = literal_str(args, "out")?;
    Some(format!("{}.vl.json", out.trim_end_matches(".svg")))
}

/// A literal string value of `args.<key>` — `None` when the arg is absent,
/// non-string, or carries a `${{ }}` interpolation (dynamic → runtime).
/// Same semantics as the checker's `literal_arg` (its adapter reads THIS).
fn literal_str(args: Option<&serde_json::Value>, key: &str) -> Option<String> {
    let s = args?.get(key)?.as_str()?;
    if s.contains("${{") {
        return None; // dynamic value · runtime concern
    }
    Some(s.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::EffectClass;
    use serde_json::json;

    #[test]
    fn notify_is_net_only_on_the_webhook_channel() {
        let webhook = json!({ "channel": "webhook", "target": "https://x.test/h" });
        assert_eq!(
            builtin_effect("nika:notify", Some(&webhook)),
            Some(BuiltinEffect::Net { url_arg: "target" })
        );
        let desktop = json!({ "channel": "desktop" });
        assert_eq!(builtin_effect("nika:notify", Some(&desktop)), None);
        // a templated channel is unclassifiable → None
        let dynamic = json!({ "channel": "${{ vars.c }}" });
        assert_eq!(builtin_effect("nika:notify", Some(&dynamic)), None);
    }

    #[test]
    fn decide_is_a_read_only_on_the_literal_path_form() {
        // Literal string bundle → a declared fs.read on `bundle:`.
        let path_form = json!({ "bundle": "./triage.bundle.json", "evidence": {} });
        assert_eq!(
            builtin_effect("nika:decide", Some(&path_form)),
            Some(BuiltinEffect::Fs {
                path_arg: "bundle",
                reads: true,
                writes: false,
                recursive: false,
            })
        );
        // Inline object bundle → pure compute, no filesystem at all.
        let inline = json!({ "bundle": { "manifest": {} }, "evidence": {} });
        assert_eq!(builtin_effect("nika:decide", Some(&inline)), None);
        // A templated path is unclassifiable statically → runtime concern.
        let dynamic = json!({ "bundle": "${{ vars.bundle }}", "evidence": {} });
        assert_eq!(builtin_effect("nika:decide", Some(&dynamic)), None);
        // No args at all (the coarse probes) → nothing to claim.
        assert_eq!(builtin_effect("nika:decide", None), None);
    }

    #[test]
    fn chart_vl_sibling_derives_the_exact_runtime_path() {
        let args = json!({ "out": "./viz/plot.svg", "compile_to": "vega_lite" });
        assert_eq!(
            chart_vl_sibling("nika:chart", Some(&args)).as_deref(),
            Some("./viz/plot.vl.json")
        );
        assert_eq!(chart_vl_sibling("nika:jq", Some(&args)), None);
        let svg_only = json!({ "out": "./viz/plot.svg" });
        assert_eq!(chart_vl_sibling("nika:chart", Some(&svg_only)), None);
    }

    /// The two effect tables — fine-grained (boundary) and coarse (policy
    /// · spec 10) — live in this one crate so their OVERLAP is pinned:
    /// where both classify a tool, they must agree on the story.
    #[test]
    fn coarse_and_fine_tables_agree_on_their_overlap() {
        // net: the coarse table's net members are Net-classified here
        // (notify on its webhook channel — the coarse table is
        // deliberately UNCONDITIONAL per the reference evaluator).
        assert!(matches!(
            builtin_effect("nika:fetch", None),
            Some(BuiltinEffect::Net { .. })
        ));
        let webhook = json!({ "channel": "webhook" });
        assert!(matches!(
            builtin_effect("nika:notify", Some(&webhook)),
            Some(BuiltinEffect::Net { .. })
        ));
        // write: every coarse Write member is a fine-grained Fs writer.
        for tool in ["nika:write", "nika:edit"] {
            let coarse = EffectClass::classify("invoke", Some(tool));
            assert!(coarse.contains(&EffectClass::Write), "{tool}");
            assert!(
                matches!(
                    builtin_effect(tool, None),
                    Some(BuiltinEffect::Fs { writes: true, .. })
                ),
                "{tool}"
            );
        }
        // builtin_egresses: net + fs-write + the notify-conservative arm,
        // never a read-only or pure-compute builtin (NEP-0002 v2.0's
        // agent-whitelist classification, from the ONE table).
        for tool in ["nika:fetch", "nika:write", "nika:edit", "nika:notify"] {
            assert!(crate::effect::builtin_egresses(tool), "{tool}");
        }
        for tool in ["nika:read", "nika:grep", "nika:jq", "nika:glob", "nika:log"] {
            assert!(!crate::effect::builtin_egresses(tool), "{tool}");
        }
        // read-only file builtins carry NO coarse class beyond tools
        // (reads are not gateable in v1 — spec 10).
        for tool in ["nika:read", "nika:grep"] {
            let coarse = EffectClass::classify("invoke", Some(tool));
            assert_eq!(
                coarse,
                std::collections::BTreeSet::from([EffectClass::Tools])
            );
            assert!(matches!(
                builtin_effect(tool, None),
                Some(BuiltinEffect::Fs { writes: false, .. })
            ));
        }
    }

    #[test]
    fn is_pure_internal_separates_the_two_classes() {
        // NEP-0003 · this predicate decides which tools need NO authority
        // under an absent permits block, which is F-O8 territory: absent
        // means zero authority, and this is the one door through it.
        // cargo-mutants killed neither `-> true` nor `-> false` here, so
        // a mutant declaring EVERY tool pure-internal (every tool allowed
        // under an absent block) went unnoticed. Both directions are
        // pinned, because one assertion only kills one of the two.
        assert!(
            is_pure_internal("nika:jq"),
            "a pure compute tool needs no authority"
        );
        assert!(
            is_pure_internal("nika:hash"),
            "a second pure tool · one example could be an accident"
        );
        assert!(
            !is_pure_internal("nika:fetch"),
            "network egress is NOT pure-internal · this is the assertion a \
             `-> true` mutant dies on"
        );
        assert!(
            !is_pure_internal("nika:write"),
            "a filesystem write is NOT pure-internal"
        );
        assert!(
            !is_pure_internal("nika:definitely_not_a_tool"),
            "an unknown name is not pure by default · the list is a closed \
             allowlist, never a fallback"
        );
    }
}
