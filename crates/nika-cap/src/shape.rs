// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin arg-shape rules — the statically-checkable contracts of
//! `stdlib/builtins-v0.1.md` the JSON Schema cannot express: the
//! `nika:done` loop sentinel · `nika:wait`'s `duration` XOR `until` ·
//! the whole `nika:fetch` contract (required `url` · closed mode set ·
//! mode pairings · the vNext payload families `body ⊥ form ⊥ multipart`
//! · the closed `traverse:` crawl spec) · the `nika:image_generate` /
//! `nika:tts_generate` closed enums and literal ranges.
//!
//! PURE DATA over `serde_json::Value` — no parser types, no spans, no
//! error enums: each rule appends a human-readable finding message to
//! `out`, and the consumer (the reference checker's analyzer) wraps
//! them in its own diagnostic type with its own spans. Extracted from
//! `nika-schema::analyzer::builtin_shape` (2026-07-07 · the crate hit
//! its 15k prod budget when fetch vNext + native-first landed — the
//! same pressure that extracted this crate's permits half on
//! 2026-07-03). Templated values (`${{ … }}`) never make a static
//! claim — the runtime re-vets everything.

use nika_types::extract::{EXTRACT_MODE_NAMES, ExtractMode};
use nika_types::net::MAX_TRAVERSE_PAGES;

use crate::{HashAlgorithm, HashEncoding};

/// Every statically-checkable arg-shape finding for one `invoke:` of
/// `tool` with `args` — empty when the shape holds (or when the tool
/// carries no shape rules). Task identity/spans are the CALLER's
/// concern (pure data in, messages out).
#[must_use]
pub fn builtin_shape_findings(tool: &str, args: Option<&serde_json::Value>) -> Vec<String> {
    let mut out = Vec::new();
    match tool {
        "nika:done" => out.push(
            "is the agent-loop completion sentinel — valid ONLY inside an \
             `agent:` tools whitelist · never a standalone invoke \
             (02-verbs.md §loop semantics · NIKA-BUILTIN-DONE-001)"
                .to_owned(),
        ),
        // The done sentinel's sibling (agent battery A3 · 2026-07-11): the
        // runtime refuses a standalone `nika:compose` (COMPOSE-001) but the
        // check blessed it — the same check≡run seam class as the SSRF
        // floor. Both loop-only builtins (ADR-096) now refuse at check.
        "nika:compose" => out.push(
            "is the agent-loop sub-workflow spawner — valid ONLY inside an \
             `agent:` tools whitelist · never a standalone invoke \
             (02-verbs.md §loop semantics · NIKA-BUILTIN-COMPOSE-001)"
                .to_owned(),
        ),
        "nika:wait" => {
            let has = |key: &str| -> bool {
                matches!(args, Some(serde_json::Value::Object(map)) if map.contains_key(key))
            };
            if has("duration") == has("until") {
                out.push(
                    "takes `duration:` XOR `until:` — exactly one \
                     (builtins-v0.1.md §nika:wait)"
                        .to_owned(),
                );
            }
        }
        "nika:fetch" => check_fetch_shape(args, &mut out),
        "nika:image_generate" => check_image_generate_shape(args, &mut out),
        "nika:chart" => check_chart_shape(args, &mut out),
        "nika:image_fx" => check_image_fx_shape(args, &mut out),
        "nika:tts_generate" => check_tts_generate_shape(args, &mut out),
        "nika:decide" => check_decide_shape(args, &mut out),
        "nika:hash" => check_hash_shape(args, &mut out),
        _ => {}
    }
    out
}

/// Judge literal choices independently: one templated argument does not
/// hide an invalid literal sibling. Resolved values are re-vetted at runtime.
fn check_hash_shape(args: Option<&serde_json::Value>, out: &mut Vec<String>) {
    let Some(map) = args.and_then(serde_json::Value::as_object) else {
        return;
    };
    if map.get("content").is_some_and(serde_json::Value::is_null) {
        out.push(
            "`content:` must be a non-null JSON value (builtins-v0.1.md §nika:hash)".to_owned(),
        );
    }
    for key in ["algo", "encoding"] {
        let Some(value) = map.get(key) else { continue };
        let Some(text) = value.as_str() else {
            out.push(format!(
                "`{key}:` must be a string (builtins-v0.1.md §nika:hash)"
            ));
            continue;
        };
        if text.contains("${{") {
            continue;
        }
        let valid = match key {
            "algo" => HashAlgorithm::parse(text).is_some(),
            _ => HashEncoding::parse(text).is_some(),
        };
        if !valid {
            out.push(format!(
                "unsupported hash `{key}: {text}` (builtins-v0.1.md §nika:hash)"
            ));
        }
    }
}

/// `nika:decide` static contracts (spec 11 §nika:decide): `bundle:` is a
/// path STRING or the inline bundle OBJECT; `evidence:` is the
/// `EvidenceSnapshot` OBJECT. A whole-string `${{ … }}` template can
/// resolve to any shape at runtime, so templated values stay silent.
fn check_decide_shape(args: Option<&serde_json::Value>, out: &mut Vec<String>) {
    let Some(serde_json::Value::Object(map)) = args else {
        return;
    };
    if let Some(bundle) = map.get("bundle")
        && !bundle.is_string()
        && !bundle.is_object()
    {
        out.push(
            "`bundle:` takes a ./path string or the inline Decision Bundle \
             object (11-decision.md §nika:decide)"
                .to_owned(),
        );
    }
    if let Some(evidence) = map.get("evidence")
        && !evidence.is_object()
        && !evidence.as_str().is_some_and(|s| s.contains("${{"))
    {
        out.push(
            "`evidence:` is the EvidenceSnapshot object `{ t, evidence: [...] }` \
             — a literal non-object can never satisfy it (11-decision.md \
             §evidence IR)"
                .to_owned(),
        );
    }
}

/// `nika:image_fx` static contracts (stdlib §Media): the op list is a
/// list of single-key maps over a closed vocabulary; enums checked when
/// literal — templated values are runtime business (statically silent).
fn check_image_fx_shape(args: Option<&serde_json::Value>, out: &mut Vec<String>) {
    const OPS: [&str; 15] = [
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
        "ascii",
    ];
    let Some(object) = args.and_then(serde_json::Value::as_object) else {
        return;
    };
    let Some(ops) = object.get("ops") else { return };
    let Some(list) = ops.as_array() else {
        out.push(
            "`ops:` must be a LIST of single-key op maps (builtins-v0.1.md §nika:image_fx)"
                .to_owned(),
        );
        return;
    };
    for (i, entry) in list.iter().enumerate() {
        let Some(map) = entry.as_object() else {
            out.push(format!(
                "ops[{i}] must be a single-key map, e.g. `- dither: {{mode: bayer4}}`"
            ));
            continue;
        };
        if map.len() != 1 {
            out.push(format!(
                "ops[{i}] must carry exactly ONE op key (got {})",
                map.len()
            ));
            continue;
        }
        if let Some(name) = map.keys().next() {
            if !OPS.contains(&name.as_str()) {
                out.push(format!(
                    "ops[{i}]: unknown op `{name}` (v1: resize · crop · levels · grayscale · \
                     palette_map · dither · duotone · pixelate · halftone · grain · vignette · \
                     chromatic_aberration · scanlines · glitch · ascii)"
                ));
            }
            if name == "ascii" && i + 1 != list.len() {
                out.push(format!(
                    "ops[{i}]: `ascii` must be the LAST op (it changes the artifact type)"
                ));
            }
            check_image_fx_op_enums(i, name, map.get(name), out);
        }
    }
}

/// Per-op literal enum checks for `nika:image_fx` — closed vocabularies
/// mirrored from the runtime parser; `${{…}}`-templated values are runtime
/// business (statically silent · the tts pattern).
fn check_image_fx_op_enums(
    i: usize,
    op: &str,
    body: Option<&serde_json::Value>,
    out: &mut Vec<String>,
) {
    let literal = |key: &str| -> Option<String> {
        let value = body?.as_object()?.get(key)?;
        let text = value.as_str()?;
        (!text.contains("${{")).then(|| text.to_owned())
    };
    let check = |key: &str, allowed: &[&str], out: &mut Vec<String>| {
        if let Some(v) = literal(key)
            && !allowed.contains(&v.as_str())
        {
            out.push(format!(
                "ops[{i}]: {op} `{key}: {v}` is not in the closed set ({})",
                allowed.join(" | ")
            ));
        }
    };
    match op {
        "dither" | "palette_map" => {
            if op == "dither" {
                check(
                    "mode",
                    &[
                        "bayer2",
                        "bayer4",
                        "bayer8",
                        "blue_noise",
                        "ign",
                        "floyd_steinberg",
                        "atkinson",
                        "jjn",
                    ],
                    out,
                );
            }
            check(
                "palette",
                &["bw", "gray4", "gameboy", "cga", "okabe_ito"],
                out,
            );
        }
        "resize" => check("filter", &["auto", "nearest", "bilinear"], out),
        "ascii" => check("emit", &["png", "text", "ansi"], out),
        "halftone" => {
            if let Some(n) = body
                .and_then(serde_json::Value::as_object)
                .and_then(|m| m.get("angle"))
                .and_then(serde_json::Value::as_i64)
                && ![0, 15, 45, 75].contains(&n)
            {
                out.push(format!(
                    "ops[{i}]: halftone `angle: {n}` (0 | 15 | 45 | 75)"
                ));
            }
        }
        _ => {}
    }
}

/// `nika:chart` static contracts (stdlib §Media · CHT): the closed
/// `type`/`semantic`/`compile_to` enums + the per-type channel requirements,
/// mirroring the runtime parser (`nika-builtin::chart` — same values,
/// caught at check time instead of after compute). Templated values
/// (`${{ … }}`) are runtime business — statically silent.
fn check_chart_shape(args: Option<&serde_json::Value>, out: &mut Vec<String>) {
    let object = args.and_then(serde_json::Value::as_object);
    let chart = object
        .and_then(|o| o.get("chart"))
        .and_then(serde_json::Value::as_object);
    let literal =
        |map: Option<&serde_json::Map<String, serde_json::Value>>, key: &str| -> Option<String> {
            let value = map?.get(key)?.as_str()?;
            (!value.contains("${{")).then(|| value.to_owned())
        };

    let chart_type = literal(chart, "type");
    if let Some(t) = &chart_type
        && !["bar", "line", "area_band", "scatter", "heatmap"].contains(&t.as_str())
    {
        out.push(format!(
            "`chart.type: {t}` is not a chart type — the set is closed: \
                 bar | line | area_band | scatter | heatmap \
                 (NIKA-BUILTIN-CHART-004)"
        ));
    }
    match (chart_type.as_deref(), chart) {
        (Some("area_band"), Some(c)) => {
            if !c.contains_key("y_lo") || !c.contains_key("y_hi") {
                out.push(
                    "`chart.type: area_band` requires `y_lo:` and `y_hi:` band bounds \
                     (NIKA-BUILTIN-CHART-004)"
                        .to_owned(),
                );
            }
        }
        (Some("heatmap"), Some(c)) => {
            if !c.contains_key("color") {
                out.push(
                    "`chart.type: heatmap` requires `color:` (the value field) \
                     (NIKA-BUILTIN-CHART-004)"
                        .to_owned(),
                );
            }
        }
        _ => {}
    }
    if let Some(target) = literal(object, "compile_to")
        && target != "vega_lite"
    {
        out.push(format!(
            "`compile_to: {target}` is unknown — the set is closed: vega_lite \
             (NIKA-BUILTIN-CHART-004)"
        ));
    }
    if let Some(path) = literal(object, "out")
        && !std::path::Path::new(&path)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
    {
        out.push(format!(
            "`out: {path}` must end in .svg — SVG is the attestation surface \
             (NIKA-BUILTIN-CHART-004)"
        ));
    }
    if let Some(semantics) = object
        .and_then(|o| o.get("semantics"))
        .and_then(serde_json::Value::as_object)
    {
        const SET: [&str; 8] = [
            "usd",
            "duration_ms",
            "tokens",
            "count",
            "delta",
            "percent",
            "timestamp",
            "category",
        ];
        for (field, value) in semantics {
            if let Some(name) = value.as_str()
                && !name.contains("${{")
                && !SET.contains(&name)
            {
                out.push(format!(
                    "`semantics.{field}: {name}` is not a semantic — the set is closed: \
                         usd | duration_ms | tokens | count | delta | percent | timestamp | \
                         category (NIKA-BUILTIN-CHART-004)"
                ));
            }
        }
    }
}

/// `nika:tts_generate` static contracts (stdlib §Audio): closed enums +
/// literal numeric ranges, mirroring the runtime parser — templated
/// values are runtime business (statically silent).
fn check_tts_generate_shape(args: Option<&serde_json::Value>, out: &mut Vec<String>) {
    let object = args.and_then(serde_json::Value::as_object);
    let literal = |key: &str| -> Option<&str> {
        let value = object?.get(key)?.as_str()?;
        (!value.contains("${{")).then_some(value)
    };

    let enums: [(&str, &[&str]); 2] = [
        ("provider", &["local", "openai", "elevenlabs", "mock"]),
        ("format", &["mp3", "wav", "auto"]),
    ];
    for (key, allowed) in enums {
        if let Some(value) = literal(key)
            && !allowed.contains(&value)
        {
            out.push(format!(
                "`{key}: {value}` is not in the closed set — {}",
                allowed.join(" · ")
            ));
        }
    }
    if let Some(value) = object.and_then(|map| map.get("speed"))
        && let Some(s) = value.as_f64()
        && !(0.25..=4.0).contains(&s)
    {
        out.push(format!("`speed: {s}` out of range — 0.25..=4.0"));
    }
    if let Some(value) = object.and_then(|map| map.get("timeout_ms"))
        && let Some(n) = value.as_i64()
        && !(1_000..=600_000).contains(&n)
    {
        out.push(format!("`timeout_ms: {n}` out of range — 1000..=600000"));
    }
}

/// `nika:fetch` static contracts (stdlib §fetch + extract-modes-v0.1.md ·
/// conformance `stdlib/extract-modes/001..004`): the mode set is CLOSED
/// at v0.1 · `jq:` pairs with `mode: jq` exactly · `selector:` pairs
/// with `mode: selector` exactly. A templated `mode:` (`${{ … }}`) is
/// runtime business — statically silent.
fn check_fetch_shape(args: Option<&serde_json::Value>, out: &mut Vec<String>) {
    let object = args.and_then(serde_json::Value::as_object);

    // `url:` is fetch's one REQUIRED argument — a fetch with nothing to
    // fetch should die at check time, not at runtime (review lens 1 P1).
    if !object.is_some_and(|map| map.contains_key("url")) {
        out.push(
            "requires a `url:` argument — a fetch with nothing to fetch \
             (builtins-v0.1.md §nika:fetch)"
                .to_owned(),
        );
        return;
    }
    check_fetch_payload_shape(object, out);
    if object.is_some_and(|map| map.contains_key("traverse")) {
        check_fetch_traverse_shape(object, out);
        return; // traverse owns the whole surface — no mode pairing below
    }
    let mode = match object.and_then(|map| map.get("mode")) {
        // Absent → the spec default (markdown).
        None => Some(ExtractMode::Markdown),
        Some(serde_json::Value::String(raw)) => {
            if raw.contains("${{") {
                None // templated — the pairing is unknowable statically
            } else {
                match raw.parse::<ExtractMode>() {
                    Ok(parsed) => Some(parsed),
                    Err(unknown) => {
                        // ONE suggestion ladder for both voices (the L0
                        // error owns it) — check and runtime teach the
                        // same route: typo-of-a-real-mode first (the
                        // rename is the fix), wrong-concept hint second.
                        let hint = unknown
                            .suggestion()
                            .map(|m| format!(" · did you mean `{m}`?"))
                            .or_else(|| unknown.hint().map(|h| format!(" · {h}")))
                            .unwrap_or_default();
                        out.push(format!(
                            "`mode: {raw}` is not a stdlib v0.1 extract mode — the set \
                         is closed: {EXTRACT_MODE_NAMES} (extract-modes-v0.1.md){hint}"
                        ));
                        return;
                    }
                }
            }
        }
        Some(_) => {
            out.push("`mode:` must be a string (extract-modes-v0.1.md)".to_owned());
            return;
        }
    };

    let has = |key: &str| object.is_some_and(|map| map.contains_key(key));
    let Some(mode) = mode else { return };

    if has("jq") && mode != ExtractMode::Jq {
        out.push(
            "`jq:` is «a jq expression · only with mode: jq» — pair it with \
             `mode: jq` or drop it (builtins-v0.1.md §nika:fetch)"
                .to_owned(),
        );
    }
    if mode == ExtractMode::Jq && !has("jq") {
        out.push(
            "`mode: jq` requires the `jq:` expression to apply \
             (builtins-v0.1.md §nika:fetch)"
                .to_owned(),
        );
    }
    if has("selector") && mode != ExtractMode::Selector {
        out.push(
            "`selector:` pairs with `mode: selector` only \
             (extract-modes-v0.1.md §selector)"
                .to_owned(),
        );
    }
    if mode == ExtractMode::Selector && !has("selector") {
        out.push(
            "`mode: selector` requires the `selector:` CSS selector \
             (extract-modes-v0.1.md §selector)"
                .to_owned(),
        );
    }
}

/// `traverse:` static contracts (stdlib §fetch · traverse): the crawl
/// excludes the single-fetch extraction args (`mode`/`selector`/`jq` —
/// the payload families are already covered by the exclusivity rule in
/// [`check_fetch_payload_shape`] since traverse is GET-only), forces
/// GET, and its own shape is CLOSED (`max_pages` 1..=25 required ·
/// `respect_robots` bool). Templated values are runtime business.
fn check_fetch_traverse_shape(
    object: Option<&serde_json::Map<String, serde_json::Value>>,
    out: &mut Vec<String>,
) {
    let has = |key: &str| object.is_some_and(|map| map.contains_key(key));
    for key in ["mode", "selector", "jq", "body", "form", "multipart"] {
        if has(key) {
            out.push(format!(
                "`traverse:` excludes `{key}:` — the crawl emits the fixed \
                     page-digest shape (builtins-v0.1.md §nika:fetch · traverse)"
            ));
        }
    }
    if let Some(method) = object
        .and_then(|map| map.get("method"))
        .and_then(serde_json::Value::as_str)
        && !method.contains("${{")
        && !method.eq_ignore_ascii_case("GET")
    {
        out.push(format!(
            "`traverse:` crawls with GET only — drop `method: {method}`"
        ));
    }
    let traverse = object.and_then(|map| map.get("traverse"));
    let map = match traverse {
        Some(serde_json::Value::String(s)) if s.contains("${{") => return, // runtime
        Some(serde_json::Value::Object(map)) => map,
        Some(_) | None => {
            out.push(
                "`traverse:` must be an object — `{ max_pages: N, respect_robots?: bool }`"
                    .to_owned(),
            );
            return;
        }
    };
    if let Some(unknown) = map
        .keys()
        .find(|k| !matches!(k.as_str(), "max_pages" | "respect_robots"))
    {
        out.push(format!(
            "`traverse.{unknown}:` is not a traverse field — the shape is closed"
        ));
    }
    match map.get("max_pages") {
        None => out.push(format!(
            "`traverse.max_pages:` is required — an integer 1..={MAX_TRAVERSE_PAGES} \
                 (the crawl bound)"
        )),
        Some(serde_json::Value::String(s)) if s.contains("${{") => {} // runtime
        Some(value) => match value.as_u64() {
            Some(n) if (1..=MAX_TRAVERSE_PAGES).contains(&n) => {}
            Some(n) => out.push(format!(
                "`traverse.max_pages: {n}` out of range — 1..={MAX_TRAVERSE_PAGES}"
            )),
            None => out.push(format!(
                "`traverse.max_pages:` must be an integer 1..={MAX_TRAVERSE_PAGES}"
            )),
        },
    }
    match map.get("respect_robots") {
        None | Some(serde_json::Value::Bool(_)) => {}
        Some(serde_json::Value::String(s)) if s.contains("${{") => {} // runtime
        Some(_) => out.push("`traverse.respect_robots:` must be a boolean".to_owned()),
    }
}

/// The fetch vNext payload families (stdlib §fetch): `body ⊥ form ⊥
/// multipart` · `form:`/`multipart:` need a body-bearing method · the
/// multipart part shape is CLOSED. A templated value (`${{ … }}`) is
/// runtime business — statically silent (the runtime re-vets all of it).
fn check_fetch_payload_shape(
    object: Option<&serde_json::Map<String, serde_json::Value>>,
    out: &mut Vec<String>,
) {
    let has = |key: &str| object.is_some_and(|map| map.contains_key(key));
    if ["body", "form", "multipart"]
        .iter()
        .filter(|key| has(key))
        .count()
        > 1
    {
        out.push(
            "at most one of `body:` · `form:` · `multipart:` \
             (builtins-v0.1.md §nika:fetch)"
                .to_owned(),
        );
    }
    if (has("form") || has("multipart"))
        && let Some(headers) = object
            .and_then(|map| map.get("headers"))
            .and_then(serde_json::Value::as_object)
        && headers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("content-type"))
    {
        out.push(
            "`form:`/`multipart:` set their own content-type — drop the \
             `headers:` entry (builtins-v0.1.md §nika:fetch)"
                .to_owned(),
        );
    }
    if has("form") || has("multipart") {
        match object
            .and_then(|map| map.get("method"))
            .and_then(serde_json::Value::as_str)
        {
            None => out.push(
                "`form:`/`multipart:` need `method: POST` (or PUT/PATCH) — \
                 the default GET carries no body"
                    .to_owned(),
            ),
            Some(raw) if raw.contains("${{") => {} // templated — runtime business
            Some(raw) => {
                let upper = raw.to_uppercase();
                if matches!(upper.as_str(), "GET" | "HEAD" | "DELETE") {
                    out.push(format!(
                        "`form:`/`multipart:` need a body-bearing method — \
                             `{raw}` carries no body (use POST · PUT · PATCH)"
                    ));
                }
            }
        }
    }
    if let Some(form) = object.and_then(|map| map.get("form"))
        && !form.is_object()
        && !matches!(form, serde_json::Value::String(s) if s.contains("${{"))
    {
        out.push(
            "`form:` must be an object of scalar fields (builtins-v0.1.md §nika:fetch)".to_owned(),
        );
    }
    if let Some(parts) = object.and_then(|map| map.get("multipart")) {
        check_multipart_parts(parts, out);
    }
}

/// The closed multipart part shape — `{name, value}` XOR `{name, path,
/// filename?, content_type?}`. Presence rules are static even when the
/// VALUES are templated; a fully-templated `multipart:` string is silent.
fn check_multipart_parts(parts: &serde_json::Value, out: &mut Vec<String>) {
    use nika_types::net::MULTIPART_PART_KEYS as PART_KEYS;
    let items = match parts {
        serde_json::Value::String(s) if s.contains("${{") => return,
        serde_json::Value::Array(items) => items,
        _ => {
            out.push(
                "`multipart:` must be an array of parts (builtins-v0.1.md §nika:fetch)".to_owned(),
            );
            return;
        }
    };
    if items.is_empty() {
        out.push("`multipart:` needs at least one part".to_owned());
        return;
    }
    for (i, item) in items.iter().enumerate() {
        let Some(map) = item.as_object() else {
            out.push(format!("multipart part {i} must be an object"));
            continue;
        };
        if let Some(unknown) = map.keys().find(|k| !PART_KEYS.contains(&k.as_str())) {
            out.push(format!(
                "multipart part {i}: unknown key `{unknown}` — the shape is \
                     {{name, value}} or {{name, path, filename?, content_type?}}"
            ));
        }
        if !map.contains_key("name") {
            out.push(format!("multipart part {i} needs a `name:`"));
        }
        match (map.contains_key("value"), map.contains_key("path")) {
            (true, true) | (false, false) => out.push(format!(
                "multipart part {i}: exactly one of `value:` (text) | `path:` (file)"
            )),
            (true, false) => {
                if map.contains_key("filename") || map.contains_key("content_type") {
                    out.push(format!(
                        "multipart part {i}: `filename:`/`content_type:` belong to \
                             file parts (`path:`)"
                    ));
                }
            }
            (false, true) => {}
        }
    }
}

/// `nika:image_generate` static contracts (stdlib §Media): the V1
/// reservations are refused loudly, the closed literal enums + numeric
/// ranges mirror the runtime parser (`nika-builtin::image::args` — same
/// values, caught at check time instead of after a spent request), and
/// the transparent-background × jpeg conflict dies here too. A templated
/// value (`${{ … }}`) is runtime business — statically silent. The flat
/// required args (`prompt:` · `output_dir:`) live in the catalog
/// `Builtin::required` set (checked by `check::tools::scan_missing_args`).
fn check_image_generate_shape(args: Option<&serde_json::Value>, out: &mut Vec<String>) {
    let object = args.and_then(serde_json::Value::as_object);
    let literal = |key: &str| -> Option<&str> {
        let value = object?.get(key)?.as_str()?;
        (!value.contains("${{")).then_some(value)
    };

    check_image_v1_reservations(object, out);

    // Closed literal enums (builtins-v0.1.md §nika:image_generate).
    let enums: [(&str, &[&str]); 5] = [
        ("provider", &["local", "openai", "gemini", "xai", "mock"]),
        ("format", &["png", "jpeg", "jpg", "webp"]),
        ("quality", &["auto", "low", "medium", "high", "ultra"]),
        ("background", &["auto", "transparent", "opaque"]),
        (
            "aspect_ratio",
            &["1:1", "16:9", "9:16", "4:3", "3:4", "3:2", "2:3", "21:9"],
        ),
    ];
    for (key, allowed) in enums {
        if let Some(value) = literal(key)
            && !allowed.contains(&value)
        {
            out.push(format!(
                "`{key}: {value}` is not in the closed set — {}",
                allowed.join(" · ")
            ));
        }
    }

    // Literal numeric ranges (same bounds as the runtime parser).
    let ranges: [(&str, i64, i64); 3] = [
        ("n", 1, 10),
        ("compression", 0, 100),
        ("timeout_ms", 1_000, 600_000),
    ];
    for (key, min, max) in ranges {
        if let Some(value) = object.and_then(|map| map.get(key))
            && let Some(n) = value.as_i64()
            && !(min..=max).contains(&n)
        {
            out.push(format!("`{key}: {n}` is out of range — {min}..={max}"));
        }
    }

    // `size:` grammar — `auto` or `WIDTHxHEIGHT`.
    if let Some(value) = literal("size")
        && value != "auto"
        && !value
            .split_once('x')
            .is_some_and(|(w, h)| w.parse::<u32>().is_ok() && h.parse::<u32>().is_ok())
    {
        out.push(format!(
            "`size: {value}` must be `WIDTHxHEIGHT` (e.g. 1024x1024) or `auto`"
        ));
    }

    // Transparency needs an alpha-capable format — the one cross-arg
    // conflict decidable statically (provider/model support is runtime).
    if literal("background") == Some("transparent")
        && matches!(literal("format"), Some("jpeg" | "jpg"))
    {
        out.push(
            "`background: transparent` needs an alpha-capable `format:` — png or webp \
             (builtins-v0.1.md §nika:image_generate)"
                .to_owned(),
        );
    }
}

/// The V1 reservations — structurally parsed, loudly refused, never
/// silently ignored (the mission's honest-boundary contract).
fn check_image_v1_reservations(
    object: Option<&serde_json::Map<String, serde_json::Value>>,
    out: &mut Vec<String>,
) {
    let literal = |key: &str| -> Option<&str> {
        let value = object?.get(key)?.as_str()?;
        (!value.contains("${{")).then_some(value)
    };
    let is_edit = match literal("mode") {
        None | Some("generate") => false,
        Some("edit") => true,
        Some(other) => {
            out.push(format!(
                "`mode: {other}` is not a mode — one of generate · edit"
            ));
            false
        }
    };
    let has = |k: &str| object.is_some_and(|m| m.contains_key(k));
    if is_edit {
        // edit REQUIRES a source; `image` XOR `images`; a templated path is
        // runtime business (statically silent).
        if !has("image") && !has("images") {
            out.push("`mode: edit` requires `image:` (a path) or `images:` (paths)".to_owned());
        }
        if has("image") && has("images") {
            out.push("`image:` and `images:` are mutually exclusive — use one".to_owned());
        }
    } else {
        // edit-only keys in generate mode are a loud static error.
        for key in ["image", "images", "mask"] {
            if has(key) {
                out.push(format!("`{key}:` requires `mode: edit`"));
            }
        }
    }
    if object.is_some_and(|map| map.contains_key("reference_images")) {
        out.push(
            "`reference_images:` is not in V1 — text-to-image only; reference-image \
             composition is on the media roadmap"
                .to_owned(),
        );
    }
    if object.and_then(|map| map.get("save")) == Some(&serde_json::Value::Bool(false)) {
        out.push(
            "`save: false` is not in V1 — assets always land in `output_dir:` \
             (image bytes never ride workflow outputs)"
                .to_owned(),
        );
    }
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;
