// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `shape` rules under test — a child module of [`super`], so it runs
//! under `--lib` (the invocation the mutation floor uses). It lives in its
//! own file because `shape.rs` plus this battery crosses the 1500-line cap.

use super::*;
use serde_json::json;

/// Findings for one `invoke:` — the whole surface under test.
fn f(tool: &str, args: &serde_json::Value) -> Vec<String> {
    builtin_shape_findings(tool, Some(args))
}

/// Exactly one finding, returned for content assertions. Pinning the
/// COUNT is half the point: a rule that fires twice, or a sibling rule
/// that fires when it should not, is as wrong as silence.
fn only(tool: &str, args: &serde_json::Value) -> String {
    let out = f(tool, args);
    assert_eq!(out.len(), 1, "expected exactly one finding, got {out:#?}");
    out.into_iter().next().unwrap_or_default()
}

/// The shape holds — no finding at all.
fn silent(tool: &str, args: &serde_json::Value) {
    let out = f(tool, args);
    assert!(out.is_empty(), "expected silence, got {out:#?}");
}

// ── dispatch ──────────────────────────────────────────────────────

#[test]
fn unknown_tool_carries_no_shape_rules() {
    silent("nika:read", &json!({"anything": true}));
    assert!(builtin_shape_findings("nika:read", None).is_empty());
}

#[test]
fn loop_only_builtins_refuse_a_standalone_invoke() {
    assert!(only("nika:done", &json!({})).contains("NIKA-BUILTIN-DONE-001"));
    assert!(only("nika:compose", &json!({})).contains("NIKA-BUILTIN-COMPOSE-001"));
}

#[test]
fn hash_rejects_literal_null_types_and_unknown_choices() {
    for (args, key) in [
        (json!({"content": null}), "content"),
        (json!({"content": "", "algo": null}), "algo"),
        (json!({"content": "", "algo": 256}), "algo"),
        (json!({"content": "", "algo": "md5"}), "algo"),
        (json!({"content": "", "algo": "SHA256"}), "algo"),
        (json!({"content": "", "encoding": null}), "encoding"),
        (json!({"content": "", "encoding": true}), "encoding"),
        (json!({"content": "", "encoding": "rot13"}), "encoding"),
    ] {
        assert!(only("nika:hash", &args).contains(key));
    }
}

#[test]
fn hash_accepts_json_and_defers_only_the_templated_choice() {
    for content in [json!(""), json!(false), json!(0), json!([]), json!({})] {
        silent("nika:hash", &json!({"content": content}));
    }
    for algo in ["blake3", "sha256", "sha512"] {
        for encoding in ["hex", "base64"] {
            silent(
                "nika:hash",
                &json!({"content": "", "algo": algo, "encoding": encoding}),
            );
        }
    }
    silent(
        "nika:hash",
        &json!({"content": "${{ inputs.value }}", "algo": "${{ inputs.algo }}"}),
    );
    assert!(
        only(
            "nika:hash",
            &json!({"content": "", "algo": "${{ inputs.algo }}", "encoding": "rot13"})
        )
        .contains("encoding")
    );
    assert!(
        only(
            "nika:hash",
            &json!({"content": "", "algo": "md5", "encoding": "${{ inputs.encoding }}"})
        )
        .contains("algo")
    );
    // Required/unknown keys belong to the catalogue checker, not this rule.
    silent("nika:hash", &json!({"unrelated": true}));
}

#[test]
fn wait_takes_duration_xor_until() {
    // Neither and both are the two failures; exactly one is the contract.
    assert!(only("nika:wait", &json!({})).contains("XOR"));
    assert!(only("nika:wait", &json!({"duration": "1s", "until": "t"})).contains("XOR"));
    silent("nika:wait", &json!({"duration": "1s"}));
    silent("nika:wait", &json!({"until": "2026-01-01T00:00:00Z"}));
    // No args at all is the "neither" case, not a silent pass.
    assert_eq!(builtin_shape_findings("nika:wait", None).len(), 1);
}

// ── nika:decide ───────────────────────────────────────────────────

#[test]
fn decide_bundle_is_a_path_string_or_an_inline_object() {
    silent("nika:decide", &json!({"bundle": "./b.yaml"}));
    silent("nika:decide", &json!({"bundle": {"id": "b"}}));
    assert!(only("nika:decide", &json!({"bundle": 7})).contains("inline Decision Bundle"));
    assert!(only("nika:decide", &json!({"bundle": []})).contains("inline Decision Bundle"));
}

#[test]
fn decide_evidence_is_the_snapshot_object_or_a_template() {
    silent(
        "nika:decide",
        &json!({"evidence": {"t": 0, "evidence": []}}),
    );
    silent("nika:decide", &json!({"evidence": "${{ tasks.a.output }}"}));
    // A literal non-object can never satisfy it — including a plain string.
    assert!(only("nika:decide", &json!({"evidence": "nope"})).contains("EvidenceSnapshot"));
    assert!(only("nika:decide", &json!({"evidence": 3})).contains("EvidenceSnapshot"));
}

#[test]
fn decide_ignores_non_object_args() {
    silent("nika:decide", &json!("string args"));
    assert!(builtin_shape_findings("nika:decide", None).is_empty());
}

// ── nika:image_fx ─────────────────────────────────────────────────

#[test]
fn image_fx_ops_must_be_a_list_of_single_key_maps() {
    silent("nika:image_fx", &json!({}));
    silent("nika:image_fx", &json!({"ops": []}));
    assert!(only("nika:image_fx", &json!({"ops": {"dither": {}}})).contains("must be a LIST"));
    assert!(only("nika:image_fx", &json!({"ops": [7]})).contains("ops[0] must be a single-key"));
    let two = only(
        "nika:image_fx",
        &json!({"ops": [{"crop": {}, "grain": {}}]}),
    );
    assert!(two.contains("exactly ONE op key (got 2)"), "{two}");
    assert!(only("nika:image_fx", &json!({"ops": [{}]})).contains("(got 0)"));
}

#[test]
fn image_fx_op_names_are_a_closed_set() {
    for op in [
        "resize",
        "crop",
        "levels",
        "grayscale",
        "palette_map",
        "dither",
        "duotone",
        "pixelate",
        "halftone",
        "grain",
        "vignette",
        "chromatic_aberration",
        "scanlines",
        "glitch",
    ] {
        silent("nika:image_fx", &json!({"ops": [{op: {}}]}));
    }
    assert!(only("nika:image_fx", &json!({"ops": [{"blur": {}}]})).contains("unknown op `blur`"));
}

#[test]
fn image_fx_ascii_must_be_the_last_op() {
    // Last is fine; anywhere else changes the artifact type mid-chain.
    silent(
        "nika:image_fx",
        &json!({"ops": [{"crop": {}}, {"ascii": {}}]}),
    );
    silent("nika:image_fx", &json!({"ops": [{"ascii": {}}]}));
    let out = only(
        "nika:image_fx",
        &json!({"ops": [{"ascii": {}}, {"crop": {}}]}),
    );
    assert!(out.contains("ops[0]: `ascii` must be the LAST op"), "{out}");
}

#[test]
fn image_fx_op_enums_are_closed_and_templates_stay_silent() {
    silent(
        "nika:image_fx",
        &json!({"ops": [{"dither": {"mode": "bayer4"}}]}),
    );
    silent(
        "nika:image_fx",
        &json!({"ops": [{"dither": {"mode": "${{ v }}"}}]}),
    );
    let bad = only(
        "nika:image_fx",
        &json!({"ops": [{"dither": {"mode": "swirl"}}]}),
    );
    assert!(
        bad.contains("dither `mode: swirl` is not in the closed set"),
        "{bad}"
    );
    assert!(
        bad.contains("floyd_steinberg"),
        "the allowed set is spelled out: {bad}"
    );

    silent(
        "nika:image_fx",
        &json!({"ops": [{"palette_map": {"palette": "gameboy"}}]}),
    );
    assert!(
        only(
            "nika:image_fx",
            &json!({"ops": [{"palette_map": {"palette": "neon"}}]})
        )
        .contains("palette_map `palette: neon`")
    );
    // `palette` is checked for dither too — the arm is shared.
    assert!(
        only(
            "nika:image_fx",
            &json!({"ops": [{"dither": {"palette": "neon"}}]})
        )
        .contains("dither `palette: neon`")
    );

    silent(
        "nika:image_fx",
        &json!({"ops": [{"resize": {"filter": "bilinear"}}]}),
    );
    assert!(
        only(
            "nika:image_fx",
            &json!({"ops": [{"resize": {"filter": "lanczos"}}]})
        )
        .contains("resize `filter: lanczos`")
    );
    silent(
        "nika:image_fx",
        &json!({"ops": [{"ascii": {"emit": "ansi"}}]}),
    );
    assert!(
        only(
            "nika:image_fx",
            &json!({"ops": [{"ascii": {"emit": "pdf"}}]})
        )
        .contains("ascii `emit: pdf`")
    );
}

#[test]
fn image_fx_halftone_angle_is_one_of_four() {
    for angle in [0, 15, 45, 75] {
        silent(
            "nika:image_fx",
            &json!({"ops": [{"halftone": {"angle": angle}}]}),
        );
    }
    let out = only(
        "nika:image_fx",
        &json!({"ops": [{"halftone": {"angle": 30}}]}),
    );
    assert!(
        out.contains("halftone `angle: 30` (0 | 15 | 45 | 75)"),
        "{out}"
    );
    // A non-integer angle is runtime business, not a static claim.
    silent(
        "nika:image_fx",
        &json!({"ops": [{"halftone": {"angle": "${{ a }}"}}]}),
    );
}

// ── nika:chart ────────────────────────────────────────────────────

#[test]
fn chart_type_is_a_closed_set() {
    for t in ["bar", "line", "scatter"] {
        silent("nika:chart", &json!({"chart": {"type": t}}));
    }
    let out = only("nika:chart", &json!({"chart": {"type": "pie"}}));
    assert!(
        out.contains("`chart.type: pie` is not a chart type"),
        "{out}"
    );
    assert!(out.contains("NIKA-BUILTIN-CHART-004"), "{out}");
    silent("nika:chart", &json!({"chart": {"type": "${{ t }}"}}));
}

#[test]
fn chart_area_band_requires_both_band_bounds() {
    silent(
        "nika:chart",
        &json!({"chart": {"type": "area_band", "y_lo": "a", "y_hi": "b"}}),
    );
    for partial in [
        json!({"type": "area_band", "y_lo": "a"}),
        json!({"type": "area_band"}),
    ] {
        assert!(only("nika:chart", &json!({"chart": partial})).contains("`y_lo:` and `y_hi:`"));
    }
    assert!(
        only(
            "nika:chart",
            &json!({"chart": {"type": "area_band", "y_hi": "b"}})
        )
        .contains("y_lo")
    );
}

#[test]
fn chart_heatmap_requires_a_color_field() {
    silent(
        "nika:chart",
        &json!({"chart": {"type": "heatmap", "color": "v"}}),
    );
    assert!(
        only("nika:chart", &json!({"chart": {"type": "heatmap"}}))
            .contains("`chart.type: heatmap` requires `color:`")
    );
    // The per-type requirement belongs to its own type only.
    silent("nika:chart", &json!({"chart": {"type": "bar"}}));
}

#[test]
fn chart_compile_target_and_output_extension_are_pinned() {
    silent(
        "nika:chart",
        &json!({"compile_to": "vega_lite", "out": "c.svg"}),
    );
    assert!(only("nika:chart", &json!({"compile_to": "d3"})).contains("`compile_to: d3`"));
    // SVG is the attestation surface — case-insensitively.
    silent("nika:chart", &json!({"out": "c.SVG"}));
    assert!(only("nika:chart", &json!({"out": "c.png"})).contains("must end in .svg"));
    assert!(only("nika:chart", &json!({"out": "c"})).contains("must end in .svg"));
    silent("nika:chart", &json!({"out": "${{ p }}"}));
}

#[test]
fn chart_semantics_are_a_closed_vocabulary() {
    silent(
        "nika:chart",
        &json!({"semantics": {"y": "usd", "x": "timestamp"}}),
    );
    let out = only("nika:chart", &json!({"semantics": {"y": "furlongs"}}));
    assert!(
        out.contains("`semantics.y: furlongs` is not a semantic"),
        "{out}"
    );
    silent("nika:chart", &json!({"semantics": {"y": "${{ s }}"}}));
    // A non-string value makes no static claim.
    silent("nika:chart", &json!({"semantics": {"y": 3}}));
}

// ── nika:tts_generate ─────────────────────────────────────────────

#[test]
fn tts_enums_are_closed() {
    silent(
        "nika:tts_generate",
        &json!({"provider": "elevenlabs", "format": "wav"}),
    );
    assert!(
        only("nika:tts_generate", &json!({"provider": "azure"}))
            .contains("`provider: azure` is not in the closed set")
    );
    assert!(only("nika:tts_generate", &json!({"format": "ogg"})).contains("`format: ogg`"));
    silent("nika:tts_generate", &json!({"provider": "${{ p }}"}));
}

#[test]
fn tts_numeric_ranges_are_inclusive_at_both_ends() {
    for speed in [0.25, 1.0, 4.0] {
        silent("nika:tts_generate", &json!({"speed": speed}));
    }
    assert!(only("nika:tts_generate", &json!({"speed": 0.2})).contains("out of range"));
    assert!(only("nika:tts_generate", &json!({"speed": 4.5})).contains("out of range"));
    for ms in [1000, 600_000] {
        silent("nika:tts_generate", &json!({"timeout_ms": ms}));
    }
    assert!(only("nika:tts_generate", &json!({"timeout_ms": 999})).contains("1000..=600000"));
    assert!(only("nika:tts_generate", &json!({"timeout_ms": 600_001})).contains("out of range"));
}

// ── nika:fetch · url + mode pairing ───────────────────────────────

#[test]
fn fetch_requires_a_url_and_stops_there() {
    let out = f("nika:fetch", &json!({"mode": "nonsense", "jq": ".x"}));
    assert_eq!(
        out.len(),
        1,
        "url is the gate — nothing else reports: {out:#?}"
    );
    assert!(out[0].contains("requires a `url:` argument"), "{out:#?}");
    assert_eq!(builtin_shape_findings("nika:fetch", None).len(), 1);
}

#[test]
fn fetch_default_mode_is_markdown_and_pairs_with_nothing() {
    silent("nika:fetch", &json!({"url": "https://x.dev"}));
}

#[test]
fn fetch_mode_is_a_closed_string_set() {
    // The closed set itself, never a hand-typed mirror of it (nika#1386 ·
    // a mirror is green on the day it is typed and blind the day after).
    for mode in nika_types::ExtractMode::ALL {
        // `selector` and `jq` pair with their expression argument; the
        // other modes pair with nothing.
        let mut args = json!({"url": "u", "mode": mode.as_str()});
        match mode {
            nika_types::ExtractMode::Selector => args["selector"] = json!("h1"),
            nika_types::ExtractMode::Jq => args["jq"] = json!("."),
            _ => {}
        }
        silent("nika:fetch", &args);
    }
    let out = only("nika:fetch", &json!({"url": "u", "mode": "markdwon"}));
    assert!(out.contains("is not a stdlib v0.1 extract mode"), "{out}");
    assert!(
        out.contains("did you mean `markdown`?"),
        "the typo ladder fires: {out}"
    );
    assert!(
        only("nika:fetch", &json!({"url": "u", "mode": 7})).contains("`mode:` must be a string")
    );
    silent(
        "nika:fetch",
        &json!({"url": "u", "mode": "${{ m }}", "jq": ".a"}),
    );
}

#[test]
fn fetch_jq_and_selector_pair_with_their_own_mode() {
    silent("nika:fetch", &json!({"url": "u", "mode": "jq", "jq": ".a"}));
    silent(
        "nika:fetch",
        &json!({"url": "u", "mode": "selector", "selector": "h1"}),
    );
    assert!(only("nika:fetch", &json!({"url": "u", "jq": ".a"})).contains("only with mode: jq"));
    assert!(
        only("nika:fetch", &json!({"url": "u", "mode": "jq"}))
            .contains("requires the `jq:` expression")
    );
    assert!(
        only("nika:fetch", &json!({"url": "u", "selector": "h1"}))
            .contains("pairs with `mode: selector` only")
    );
    assert!(
        only("nika:fetch", &json!({"url": "u", "mode": "selector"}))
            .contains("requires the `selector:` CSS selector")
    );
}

// ── nika:fetch · traverse ─────────────────────────────────────────

#[test]
fn traverse_excludes_the_single_fetch_extraction_args() {
    for key in ["mode", "selector", "jq"] {
        let out = f(
            "nika:fetch",
            &json!({"url": "u", "traverse": {"max_pages": 2}, key: "v"}),
        );
        assert!(
            out.iter()
                .any(|m| m.contains(&format!("`traverse:` excludes `{key}:`"))),
            "{key} must be refused: {out:#?}"
        );
    }
    // And traverse owns the surface: no mode pairing is reported on top.
    let out = f(
        "nika:fetch",
        &json!({"url": "u", "traverse": {"max_pages": 2}, "jq": ".a"}),
    );
    assert!(
        !out.iter().any(|m| m.contains("only with mode: jq")),
        "{out:#?}"
    );
}

#[test]
fn traverse_crawls_with_get_only() {
    silent(
        "nika:fetch",
        &json!({"url": "u", "traverse": {"max_pages": 1}, "method": "get"}),
    );
    let out = only(
        "nika:fetch",
        &json!({"url": "u", "traverse": {"max_pages": 1}, "method": "POST"}),
    );
    assert!(
        out.contains("crawls with GET only — drop `method: POST`"),
        "{out}"
    );
    silent(
        "nika:fetch",
        &json!({"url": "u", "traverse": {"max_pages": 1}, "method": "${{ m }}"}),
    );
}

#[test]
fn traverse_shape_is_closed() {
    silent(
        "nika:fetch",
        &json!({"url": "u", "traverse": {"max_pages": 25}}),
    );
    silent(
        "nika:fetch",
        &json!({"url": "u", "traverse": {"max_pages": 1, "respect_robots": false}}),
    );
    silent("nika:fetch", &json!({"url": "u", "traverse": "${{ t }}"}));
    assert!(
        only("nika:fetch", &json!({"url": "u", "traverse": 7}))
            .contains("`traverse:` must be an object")
    );
    assert!(
        only(
            "nika:fetch",
            &json!({"url": "u", "traverse": {"max_pages": 1, "depth": 3}})
        )
        .contains("`traverse.depth:` is not a traverse field")
    );
}

#[test]
fn traverse_max_pages_is_required_and_bounded() {
    assert!(
        only("nika:fetch", &json!({"url": "u", "traverse": {}}))
            .contains("`traverse.max_pages:` is required")
    );
    for n in [0, 26] {
        let out = only(
            "nika:fetch",
            &json!({"url": "u", "traverse": {"max_pages": n}}),
        );
        assert!(
            out.contains(&format!("max_pages: {n}` out of range")),
            "{out}"
        );
    }
    assert!(
        only(
            "nika:fetch",
            &json!({"url": "u", "traverse": {"max_pages": "two"}})
        )
        .contains("must be an integer 1..=25")
    );
    silent(
        "nika:fetch",
        &json!({"url": "u", "traverse": {"max_pages": "${{ n }}"}}),
    );
}

#[test]
fn traverse_respect_robots_is_a_boolean() {
    silent(
        "nika:fetch",
        &json!({"url": "u", "traverse": {"max_pages": 1, "respect_robots": true}}),
    );
    silent(
        "nika:fetch",
        &json!({"url": "u", "traverse": {"max_pages": 1, "respect_robots": "${{ r }}"}}),
    );
    assert!(
        only(
            "nika:fetch",
            &json!({"url": "u", "traverse": {"max_pages": 1, "respect_robots": 1}})
        )
        .contains("must be a boolean")
    );
}

// ── nika:fetch · payload families ─────────────────────────────────

#[test]
fn payload_families_are_mutually_exclusive() {
    silent(
        "nika:fetch",
        &json!({"url": "u", "method": "POST", "body": "x"}),
    );
    let out = f(
        "nika:fetch",
        &json!({"url": "u", "method": "POST", "body": "x", "form": {}}),
    );
    assert!(out.iter().any(|m| m.contains("at most one of")), "{out:#?}");
    let three = f(
        "nika:fetch",
        &json!({"url": "u", "method": "POST", "body": "x", "form": {}, "multipart": []}),
    );
    assert!(
        three.iter().any(|m| m.contains("at most one of")),
        "{three:#?}"
    );
}

#[test]
fn form_and_multipart_need_a_body_bearing_method() {
    silent(
        "nika:fetch",
        &json!({"url": "u", "method": "POST", "form": {"a": 1}}),
    );
    silent(
        "nika:fetch",
        &json!({"url": "u", "method": "patch", "form": {"a": 1}}),
    );
    assert!(
        only("nika:fetch", &json!({"url": "u", "form": {"a": 1}})).contains("need `method: POST`")
    );
    for verb in ["GET", "HEAD", "DELETE"] {
        let out = only(
            "nika:fetch",
            &json!({"url": "u", "method": verb, "form": {"a": 1}}),
        );
        assert!(out.contains("carries no body"), "{verb}: {out}");
    }
    silent(
        "nika:fetch",
        &json!({"url": "u", "method": "${{ m }}", "form": {"a": 1}}),
    );
}

#[test]
fn form_and_multipart_own_their_content_type() {
    let out = only(
        "nika:fetch",
        &json!({"url": "u", "method": "POST", "form": {}, "headers": {"Content-Type": "x"}}),
    );
    assert!(out.contains("set their own content-type"), "{out}");
    // Another header is fine, and body: carries its own content-type.
    silent(
        "nika:fetch",
        &json!({"url": "u", "method": "POST", "form": {}, "headers": {"Accept": "x"}}),
    );
    silent(
        "nika:fetch",
        &json!({"url": "u", "method": "POST", "body": "b", "headers": {"content-type": "x"}}),
    );
}

#[test]
fn form_is_an_object_or_a_template() {
    silent(
        "nika:fetch",
        &json!({"url": "u", "method": "POST", "form": "${{ f }}"}),
    );
    let out = f(
        "nika:fetch",
        &json!({"url": "u", "method": "POST", "form": "literal"}),
    );
    assert!(
        out.iter().any(|m| m.contains("`form:` must be an object")),
        "{out:#?}"
    );
}

#[test]
fn multipart_parts_carry_a_closed_shape() {
    let post = |parts: serde_json::Value| {
        f(
            "nika:fetch",
            &json!({"url": "u", "method": "POST", "multipart": parts}),
        )
    };
    assert!(post(json!([{"name": "a", "value": "v"}])).is_empty());
    assert!(
        post(json!([{"name": "a", "path": "./f", "filename": "f", "content_type": "t"}]))
            .is_empty()
    );
    silent(
        "nika:fetch",
        &json!({"url": "u", "method": "POST", "multipart": "${{ m }}"}),
    );

    assert!(
        post(json!({}))
            .iter()
            .any(|m| m.contains("must be an array of parts"))
    );
    assert!(
        post(json!([]))
            .iter()
            .any(|m| m.contains("needs at least one part"))
    );
    assert!(
        post(json!([7]))
            .iter()
            .any(|m| m.contains("part 0 must be an object"))
    );
    assert!(
        post(json!([{"name": "a", "value": "v", "size": 1}]))
            .iter()
            .any(|m| m.contains("unknown key `size`"))
    );
    assert!(
        post(json!([{"value": "v"}]))
            .iter()
            .any(|m| m.contains("part 0 needs a `name:`"))
    );
}

#[test]
fn multipart_part_is_a_text_part_xor_a_file_part() {
    let post = |part: serde_json::Value| {
        f(
            "nika:fetch",
            &json!({"url": "u", "method": "POST", "multipart": [part]}),
        )
    };
    for ambiguous in [
        json!({"name": "a", "value": "v", "path": "./f"}),
        json!({"name": "a"}),
    ] {
        assert!(
            post(ambiguous)
                .iter()
                .any(|m| m.contains("exactly one of `value:`")),
            "both-or-neither must be refused"
        );
    }
    // File-only keys on a text part are a static error.
    assert!(
        post(json!({"name": "a", "value": "v", "filename": "f"}))
            .iter()
            .any(|m| m.contains("belong to file parts"))
    );
    assert!(
        post(json!({"name": "a", "value": "v", "content_type": "t"}))
            .iter()
            .any(|m| m.contains("belong to file parts"))
    );
}

// ── nika:image_generate ───────────────────────────────────────────

#[test]
fn image_enums_are_closed() {
    silent(
        "nika:image_generate",
        &json!({"provider": "gemini", "format": "webp", "quality": "ultra",
               "background": "opaque", "aspect_ratio": "21:9"}),
    );
    for (key, bad) in [
        ("provider", "midjourney"),
        ("format", "gif"),
        ("quality", "insane"),
        ("background", "checkered"),
        ("aspect_ratio", "5:4"),
    ] {
        let out = only("nika:image_generate", &json!({key: bad}));
        assert!(
            out.contains(&format!("`{key}: {bad}` is not in the closed set")),
            "{out}"
        );
    }
    silent("nika:image_generate", &json!({"provider": "${{ p }}"}));
}

#[test]
fn image_numeric_ranges_are_inclusive_at_both_ends() {
    for (key, lo, hi) in [
        ("n", 1, 10),
        ("compression", 0, 100),
        ("timeout_ms", 1_000, 600_000),
    ] {
        silent("nika:image_generate", &json!({key: lo}));
        silent("nika:image_generate", &json!({key: hi}));
        assert!(only("nika:image_generate", &json!({key: lo - 1})).contains("out of range"));
        assert!(only("nika:image_generate", &json!({key: hi + 1})).contains("out of range"));
    }
}

#[test]
fn image_size_is_auto_or_width_by_height() {
    silent("nika:image_generate", &json!({"size": "auto"}));
    silent("nika:image_generate", &json!({"size": "1024x1024"}));
    for bad in ["1024", "1024x", "x768", "big x small", "1024X768"] {
        let out = only("nika:image_generate", &json!({"size": bad}));
        assert!(out.contains("must be `WIDTHxHEIGHT`"), "{bad}: {out}");
    }
    silent("nika:image_generate", &json!({"size": "${{ s }}"}));
}

#[test]
fn transparent_background_needs_an_alpha_capable_format() {
    for fmt in ["png", "webp"] {
        silent(
            "nika:image_generate",
            &json!({"background": "transparent", "format": fmt}),
        );
    }
    for fmt in ["jpeg", "jpg"] {
        let out = only(
            "nika:image_generate",
            &json!({"background": "transparent", "format": fmt}),
        );
        assert!(
            out.contains("needs an alpha-capable `format:`"),
            "{fmt}: {out}"
        );
    }
    // Neither half alone is a conflict.
    silent("nika:image_generate", &json!({"format": "jpeg"}));
    silent("nika:image_generate", &json!({"background": "transparent"}));
}

#[test]
fn image_edit_mode_requires_a_source() {
    silent(
        "nika:image_generate",
        &json!({"mode": "edit", "image": "./a.png"}),
    );
    silent(
        "nika:image_generate",
        &json!({"mode": "edit", "images": ["./a.png"]}),
    );
    assert!(
        only("nika:image_generate", &json!({"mode": "edit"}))
            .contains("requires `image:` (a path) or `images:` (paths)")
    );
    let both = only(
        "nika:image_generate",
        &json!({"mode": "edit", "image": "./a", "images": ["./b"]}),
    );
    assert!(both.contains("mutually exclusive"), "{both}");
    assert!(
        only("nika:image_generate", &json!({"mode": "inpaint"}))
            .contains("`mode: inpaint` is not a mode")
    );
}

#[test]
fn edit_only_keys_are_refused_in_generate_mode() {
    for key in ["image", "images", "mask"] {
        let out = only("nika:image_generate", &json!({key: "./a.png"}));
        assert!(
            out.contains(&format!("`{key}:` requires `mode: edit`")),
            "{out}"
        );
        // Explicit generate is the same as the default.
        let explicit = only(
            "nika:image_generate",
            &json!({"mode": "generate", key: "./a.png"}),
        );
        assert!(explicit.contains("requires `mode: edit`"), "{explicit}");
    }
}

#[test]
fn v1_reservations_are_refused_loudly() {
    assert!(
        only("nika:image_generate", &json!({"reference_images": ["./a"]}))
            .contains("`reference_images:` is not in V1")
    );
    assert!(
        only("nika:image_generate", &json!({"save": false})).contains("`save: false` is not in V1")
    );
    // Only the literal `false` is reserved.
    silent("nika:image_generate", &json!({"save": true}));
}
