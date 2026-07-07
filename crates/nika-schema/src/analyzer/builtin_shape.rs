// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Builtin arg-shape rules — the statically-checkable contracts of
//! `stdlib/builtins-v0.1.md` the JSON Schema cannot express (deep
//! conformance fixtures 009-012) · `nika:write` requires `content:` ·
//! `nika:done` is valid only inside an `agent:` tools whitelist (the
//! loop sentinel · `NIKA-BUILTIN-DONE-001`) · `nika:jq` takes exactly
//! `expression:` · `nika:wait` takes `duration:` XOR `until:`.

use nika_types::extract::{EXTRACT_MODE_NAMES, ExtractMode};
use nika_types::net::MAX_TRAVERSE_PAGES;

use crate::error::SchemaError;
use crate::raw::{RawAction, RawTask};
use crate::source::{Span, Spanned};

/// Run every builtin arg-shape rule over a task's action (and its
/// `on_finally:` cleanup actions — same `invoke:` surface).
pub(super) fn check_builtin_shapes(tasks: &[Spanned<RawTask>], errors: &mut Vec<SchemaError>) {
    for task in tasks {
        let id = task.value.id.value.as_str();
        check_action(&task.value.action, id, errors);
        for cleanup in &task.value.on_finally {
            check_action(&cleanup.value.action, id, errors);
        }
    }
}

fn check_action(action: &RawAction, task: &str, errors: &mut Vec<SchemaError>) {
    let RawAction::Invoke(invoke) = action else {
        return;
    };
    let tool = invoke.tool.value.as_str();
    let span = invoke.tool.span;
    let args = invoke.args.as_ref().map(|a| &a.value);
    let has = |key: &str| -> bool {
        matches!(args, Some(serde_json::Value::Object(map)) if map.contains_key(key))
    };

    // Flat required-arg contracts (`nika:write` content · `nika:jq`
    // expression · `nika:fetch` url · …) now live in the catalog's
    // `Builtin::required` and are checked by `check::tools::scan_missing_args`
    // — ONE source, no double report. This ladder keeps only the
    // NON-FLAT contracts the catalog set cannot express.
    match tool {
        "nika:done" => errors.push(shape(
            task,
            tool,
            "is the agent-loop completion sentinel — valid ONLY inside an \
             `agent:` tools whitelist · never a standalone invoke \
             (02-verbs.md §loop semantics · NIKA-BUILTIN-DONE-001)",
            span,
        )),
        "nika:wait" if has("duration") == has("until") => errors.push(shape(
            task,
            tool,
            "takes `duration:` XOR `until:` — exactly one \
             (builtins-v0.1.md §nika:wait)",
            span,
        )),
        // `nika:fetch` keeps its WHOLE contract here (incl. the required
        // `url:`) because the mode-dependent `jq`/`selector` pairings are
        // conditional — splitting `url` to the catalog set would double the
        // surface for one builtin. Its `Builtin::required` stays empty.
        "nika:fetch" => check_fetch_shape(task, tool, args, span, errors),
        "nika:image_generate" => check_image_generate_shape(task, tool, args, span, errors),
        "nika:tts_generate" => check_tts_generate_shape(task, tool, args, span, errors),
        _ => {}
    }
}

/// `nika:tts_generate` static contracts (stdlib §Audio): closed enums +
/// literal numeric ranges, mirroring the runtime parser — templated
/// values are runtime business (statically silent).
fn check_tts_generate_shape(
    task: &str,
    tool: &str,
    args: Option<&serde_json::Value>,
    span: Span,
    errors: &mut Vec<SchemaError>,
) {
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
            errors.push(shape(
                task,
                tool,
                &format!(
                    "`{key}: {value}` is not in the closed set — {}",
                    allowed.join(" · ")
                ),
                span,
            ));
        }
    }
    if let Some(value) = object.and_then(|map| map.get("speed"))
        && let Some(s) = value.as_f64()
        && !(0.25..=4.0).contains(&s)
    {
        errors.push(shape(
            task,
            tool,
            &format!("`speed: {s}` out of range — 0.25..=4.0"),
            span,
        ));
    }
    if let Some(value) = object.and_then(|map| map.get("timeout_ms"))
        && let Some(n) = value.as_i64()
        && !(1_000..=600_000).contains(&n)
    {
        errors.push(shape(
            task,
            tool,
            &format!("`timeout_ms: {n}` out of range — 1000..=600000"),
            span,
        ));
    }
}

/// `nika:fetch` static contracts (stdlib §fetch + extract-modes-v0.1.md ·
/// conformance `stdlib/extract-modes/001..004`): the mode set is CLOSED
/// at v0.1 · `jq:` pairs with `mode: jq` exactly · `selector:` pairs
/// with `mode: selector` exactly. A templated `mode:` (`${{ … }}`) is
/// runtime business — statically silent.
fn check_fetch_shape(
    task: &str,
    tool: &str,
    args: Option<&serde_json::Value>,
    span: Span,
    errors: &mut Vec<SchemaError>,
) {
    let object = args.and_then(serde_json::Value::as_object);

    // `url:` is fetch's one REQUIRED argument — a fetch with nothing to
    // fetch should die at check time, not at runtime (review lens 1 P1).
    if !object.is_some_and(|map| map.contains_key("url")) {
        errors.push(shape(
            task,
            tool,
            "requires a `url:` argument — a fetch with nothing to fetch \
             (builtins-v0.1.md §nika:fetch)",
            span,
        ));
        return;
    }
    check_fetch_payload_shape(task, tool, object, span, errors);
    if object.is_some_and(|map| map.contains_key("traverse")) {
        check_fetch_traverse_shape(task, tool, object, span, errors);
        return; // traverse owns the whole surface — no mode pairing below
    }
    let mode = match object.and_then(|map| map.get("mode")) {
        // Absent → the spec default (markdown).
        None => Some(ExtractMode::Markdown),
        Some(serde_json::Value::String(raw)) => {
            if raw.contains("${{") {
                None // templated — the pairing is unknowable statically
            } else if let Ok(parsed) = raw.parse::<ExtractMode>() {
                Some(parsed)
            } else {
                errors.push(shape(
                    task,
                    tool,
                    &format!(
                        "`mode: {raw}` is not a stdlib v0.1 extract mode — the set \
                         is closed: {EXTRACT_MODE_NAMES} (extract-modes-v0.1.md)"
                    ),
                    span,
                ));
                return;
            }
        }
        Some(_) => {
            errors.push(shape(
                task,
                tool,
                "`mode:` must be a string (extract-modes-v0.1.md)",
                span,
            ));
            return;
        }
    };

    let has = |key: &str| object.is_some_and(|map| map.contains_key(key));
    let Some(mode) = mode else { return };

    if has("jq") && mode != ExtractMode::Jq {
        errors.push(shape(
            task,
            tool,
            "`jq:` is «a jq expression · only with mode: jq» — pair it with \
             `mode: jq` or drop it (builtins-v0.1.md §nika:fetch)",
            span,
        ));
    }
    if mode == ExtractMode::Jq && !has("jq") {
        errors.push(shape(
            task,
            tool,
            "`mode: jq` requires the `jq:` expression to apply \
             (builtins-v0.1.md §nika:fetch)",
            span,
        ));
    }
    if has("selector") && mode != ExtractMode::Selector {
        errors.push(shape(
            task,
            tool,
            "`selector:` pairs with `mode: selector` only \
             (extract-modes-v0.1.md §selector)",
            span,
        ));
    }
    if mode == ExtractMode::Selector && !has("selector") {
        errors.push(shape(
            task,
            tool,
            "`mode: selector` requires the `selector:` CSS selector \
             (extract-modes-v0.1.md §selector)",
            span,
        ));
    }
}

/// `traverse:` static contracts (stdlib §fetch · traverse): the crawl
/// excludes the single-fetch extraction args (`mode`/`selector`/`jq` —
/// the payload families are already covered by the exclusivity rule in
/// [`check_fetch_payload_shape`] since traverse is GET-only), forces
/// GET, and its own shape is CLOSED (`max_pages` 1..=25 required ·
/// `respect_robots` bool). Templated values are runtime business.
fn check_fetch_traverse_shape(
    task: &str,
    tool: &str,
    object: Option<&serde_json::Map<String, serde_json::Value>>,
    span: Span,
    errors: &mut Vec<SchemaError>,
) {
    let has = |key: &str| object.is_some_and(|map| map.contains_key(key));
    for key in ["mode", "selector", "jq", "body", "form", "multipart"] {
        if has(key) {
            errors.push(shape(
                task,
                tool,
                &format!(
                    "`traverse:` excludes `{key}:` — the crawl emits the fixed \
                     page-digest shape (builtins-v0.1.md §nika:fetch · traverse)"
                ),
                span,
            ));
        }
    }
    if let Some(method) = object
        .and_then(|map| map.get("method"))
        .and_then(serde_json::Value::as_str)
        && !method.contains("${{")
        && !method.eq_ignore_ascii_case("GET")
    {
        errors.push(shape(
            task,
            tool,
            &format!("`traverse:` crawls with GET only — drop `method: {method}`"),
            span,
        ));
    }
    let traverse = object.and_then(|map| map.get("traverse"));
    let map = match traverse {
        Some(serde_json::Value::String(s)) if s.contains("${{") => return, // runtime
        Some(serde_json::Value::Object(map)) => map,
        Some(_) | None => {
            errors.push(shape(
                task,
                tool,
                "`traverse:` must be an object — `{ max_pages: N, respect_robots?: bool }`",
                span,
            ));
            return;
        }
    };
    if let Some(unknown) = map
        .keys()
        .find(|k| !matches!(k.as_str(), "max_pages" | "respect_robots"))
    {
        errors.push(shape(
            task,
            tool,
            &format!("`traverse.{unknown}:` is not a traverse field — the shape is closed"),
            span,
        ));
    }
    match map.get("max_pages") {
        None => errors.push(shape(
            task,
            tool,
            &format!(
                "`traverse.max_pages:` is required — an integer 1..={MAX_TRAVERSE_PAGES} \
                 (the crawl bound)"
            ),
            span,
        )),
        Some(serde_json::Value::String(s)) if s.contains("${{") => {} // runtime
        Some(value) => match value.as_u64() {
            Some(n) if (1..=MAX_TRAVERSE_PAGES).contains(&n) => {}
            Some(n) => errors.push(shape(
                task,
                tool,
                &format!("`traverse.max_pages: {n}` out of range — 1..={MAX_TRAVERSE_PAGES}"),
                span,
            )),
            None => errors.push(shape(
                task,
                tool,
                &format!("`traverse.max_pages:` must be an integer 1..={MAX_TRAVERSE_PAGES}"),
                span,
            )),
        },
    }
    match map.get("respect_robots") {
        None | Some(serde_json::Value::Bool(_)) => {}
        Some(serde_json::Value::String(s)) if s.contains("${{") => {} // runtime
        Some(_) => errors.push(shape(
            task,
            tool,
            "`traverse.respect_robots:` must be a boolean",
            span,
        )),
    }
}

/// The fetch vNext payload families (stdlib §fetch): `body ⊥ form ⊥
/// multipart` · `form:`/`multipart:` need a body-bearing method · the
/// multipart part shape is CLOSED. A templated value (`${{ … }}`) is
/// runtime business — statically silent (the runtime re-vets all of it).
fn check_fetch_payload_shape(
    task: &str,
    tool: &str,
    object: Option<&serde_json::Map<String, serde_json::Value>>,
    span: Span,
    errors: &mut Vec<SchemaError>,
) {
    let has = |key: &str| object.is_some_and(|map| map.contains_key(key));
    if ["body", "form", "multipart"]
        .iter()
        .filter(|key| has(key))
        .count()
        > 1
    {
        errors.push(shape(
            task,
            tool,
            "at most one of `body:` · `form:` · `multipart:` \
             (builtins-v0.1.md §nika:fetch)",
            span,
        ));
    }
    if has("form") || has("multipart") {
        match object
            .and_then(|map| map.get("method"))
            .and_then(serde_json::Value::as_str)
        {
            None => errors.push(shape(
                task,
                tool,
                "`form:`/`multipart:` need `method: POST` (or PUT/PATCH) — \
                 the default GET carries no body",
                span,
            )),
            Some(raw) if raw.contains("${{") => {} // templated — runtime business
            Some(raw) => {
                let upper = raw.to_uppercase();
                if matches!(upper.as_str(), "GET" | "HEAD" | "DELETE") {
                    errors.push(shape(
                        task,
                        tool,
                        &format!(
                            "`form:`/`multipart:` need a body-bearing method — \
                             `{raw}` carries no body (use POST · PUT · PATCH)"
                        ),
                        span,
                    ));
                }
            }
        }
    }
    if let Some(form) = object.and_then(|map| map.get("form"))
        && !form.is_object()
        && !matches!(form, serde_json::Value::String(s) if s.contains("${{"))
    {
        errors.push(shape(
            task,
            tool,
            "`form:` must be an object of scalar fields (builtins-v0.1.md §nika:fetch)",
            span,
        ));
    }
    if let Some(parts) = object.and_then(|map| map.get("multipart")) {
        check_multipart_parts(task, tool, parts, span, errors);
    }
}

/// The closed multipart part shape — `{name, value}` XOR `{name, path,
/// filename?, content_type?}`. Presence rules are static even when the
/// VALUES are templated; a fully-templated `multipart:` string is silent.
fn check_multipart_parts(
    task: &str,
    tool: &str,
    parts: &serde_json::Value,
    span: Span,
    errors: &mut Vec<SchemaError>,
) {
    const PART_KEYS: [&str; 5] = ["name", "value", "path", "filename", "content_type"];
    let items = match parts {
        serde_json::Value::String(s) if s.contains("${{") => return,
        serde_json::Value::Array(items) => items,
        _ => {
            errors.push(shape(
                task,
                tool,
                "`multipart:` must be an array of parts (builtins-v0.1.md §nika:fetch)",
                span,
            ));
            return;
        }
    };
    if items.is_empty() {
        errors.push(shape(
            task,
            tool,
            "`multipart:` needs at least one part",
            span,
        ));
        return;
    }
    for (i, item) in items.iter().enumerate() {
        let Some(map) = item.as_object() else {
            errors.push(shape(
                task,
                tool,
                &format!("multipart part {i} must be an object"),
                span,
            ));
            continue;
        };
        if let Some(unknown) = map.keys().find(|k| !PART_KEYS.contains(&k.as_str())) {
            errors.push(shape(
                task,
                tool,
                &format!(
                    "multipart part {i}: unknown key `{unknown}` — the shape is \
                     {{name, value}} or {{name, path, filename?, content_type?}}"
                ),
                span,
            ));
        }
        if !map.contains_key("name") {
            errors.push(shape(
                task,
                tool,
                &format!("multipart part {i} needs a `name:`"),
                span,
            ));
        }
        match (map.contains_key("value"), map.contains_key("path")) {
            (true, true) | (false, false) => errors.push(shape(
                task,
                tool,
                &format!("multipart part {i}: exactly one of `value:` (text) | `path:` (file)"),
                span,
            )),
            (true, false) => {
                if map.contains_key("filename") || map.contains_key("content_type") {
                    errors.push(shape(
                        task,
                        tool,
                        &format!(
                            "multipart part {i}: `filename:`/`content_type:` belong to \
                             file parts (`path:`)"
                        ),
                        span,
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
fn check_image_generate_shape(
    task: &str,
    tool: &str,
    args: Option<&serde_json::Value>,
    span: Span,
    errors: &mut Vec<SchemaError>,
) {
    let object = args.and_then(serde_json::Value::as_object);
    let literal = |key: &str| -> Option<&str> {
        let value = object?.get(key)?.as_str()?;
        (!value.contains("${{")).then_some(value)
    };

    check_image_v1_reservations(task, tool, object, span, errors);

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
            errors.push(shape(
                task,
                tool,
                &format!(
                    "`{key}: {value}` is not in the closed set — {}",
                    allowed.join(" · ")
                ),
                span,
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
            errors.push(shape(
                task,
                tool,
                &format!("`{key}: {n}` is out of range — {min}..={max}"),
                span,
            ));
        }
    }

    // `size:` grammar — `auto` or `WIDTHxHEIGHT`.
    if let Some(value) = literal("size")
        && value != "auto"
        && !value
            .split_once('x')
            .is_some_and(|(w, h)| w.parse::<u32>().is_ok() && h.parse::<u32>().is_ok())
    {
        errors.push(shape(
            task,
            tool,
            &format!("`size: {value}` must be `WIDTHxHEIGHT` (e.g. 1024x1024) or `auto`"),
            span,
        ));
    }

    // Transparency needs an alpha-capable format — the one cross-arg
    // conflict decidable statically (provider/model support is runtime).
    if literal("background") == Some("transparent")
        && matches!(literal("format"), Some("jpeg" | "jpg"))
    {
        errors.push(shape(
            task,
            tool,
            "`background: transparent` needs an alpha-capable `format:` — png or webp \
             (builtins-v0.1.md §nika:image_generate)",
            span,
        ));
    }
}

/// The V1 reservations — structurally parsed, loudly refused, never
/// silently ignored (the mission's honest-boundary contract).
fn check_image_v1_reservations(
    task: &str,
    tool: &str,
    object: Option<&serde_json::Map<String, serde_json::Value>>,
    span: Span,
    errors: &mut Vec<SchemaError>,
) {
    let literal = |key: &str| -> Option<&str> {
        let value = object?.get(key)?.as_str()?;
        (!value.contains("${{")).then_some(value)
    };
    let is_edit = match literal("mode") {
        None | Some("generate") => false,
        Some("edit") => true,
        Some(other) => {
            errors.push(shape(
                task,
                tool,
                &format!("`mode: {other}` is not a mode — one of generate · edit"),
                span,
            ));
            false
        }
    };
    let has = |k: &str| object.is_some_and(|m| m.contains_key(k));
    if is_edit {
        // edit REQUIRES a source; `image` XOR `images`; a templated path is
        // runtime business (statically silent).
        if !has("image") && !has("images") {
            errors.push(shape(
                task,
                tool,
                "`mode: edit` requires `image:` (a path) or `images:` (paths)",
                span,
            ));
        }
        if has("image") && has("images") {
            errors.push(shape(
                task,
                tool,
                "`image:` and `images:` are mutually exclusive — use one",
                span,
            ));
        }
    } else {
        // edit-only keys in generate mode are a loud static error.
        for key in ["image", "images", "mask"] {
            if has(key) {
                errors.push(shape(
                    task,
                    tool,
                    &format!("`{key}:` requires `mode: edit`"),
                    span,
                ));
            }
        }
    }
    if object.is_some_and(|map| map.contains_key("reference_images")) {
        errors.push(shape(
            task,
            tool,
            "`reference_images:` is not in V1 — text-to-image only; reference-image \
             composition is on the media roadmap",
            span,
        ));
    }
    if object.and_then(|map| map.get("save")) == Some(&serde_json::Value::Bool(false)) {
        errors.push(shape(
            task,
            tool,
            "`save: false` is not in V1 — assets always land in `output_dir:` \
             (image bytes never ride workflow outputs)",
            span,
        ));
    }
}

fn shape(task: &str, tool: &str, reason: &str, span: Span) -> SchemaError {
    SchemaError::BadBuiltinArgs {
        task: task.to_owned(),
        tool: tool.to_owned(),
        reason: reason.to_owned(),
        span: Some(span),
    }
}

#[cfg(test)]
mod tests {
    use crate::analyzer::analyze;
    use crate::error::SchemaError;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;

    fn has_shape_error(yaml: &str, tool: &str) -> bool {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        analyze(&wf)
            .err()
            .unwrap_or_default()
            .iter()
            .any(|e| matches!(e, SchemaError::BadBuiltinArgs { tool: t, .. } if t == tool))
    }

    /// Run one `(args · tool · violates?)` truth table — each row is one
    /// contract direction.
    fn assert_shape_cases(cases: &[(&str, &str, bool)]) {
        for (args, tool, violates) in cases {
            let yaml = format!(
                "nika: v1\nworkflow: t\ntasks:\n  - id: a\n    invoke:\n      \
                 tool: \"{tool}\"\n      args: {args}\n"
            );
            assert_eq!(
                has_shape_error(&yaml, tool),
                *violates,
                "{tool} · args {args}"
            );
        }
    }

    #[test]
    fn shape_rules_table() {
        // (args yaml · tool · violates?) — one row per contract direction.
        // NOTE: flat required-arg contracts (`nika:write` content · `nika:jq`
        // expression) moved to the catalog `Builtin::required` set + the
        // `check::tools::scan_missing_args` check — they are NOT shape-rule
        // findings anymore (tested there). This table keeps the non-flat
        // contracts: the `done` sentinel · the `wait` XOR · `fetch` pairings.
        let cases = [
            ("{}", "nika:done", true), // standalone · always the sentinel error
            (
                r#"{ duration: "5s", until: "2026-12-01T00:00:00Z" }"#,
                "nika:wait",
                true, // both modes
            ),
            ("{}", "nika:wait", true),                     // neither mode
            (r#"{ duration: "5s" }"#, "nika:wait", false), // exactly one
            // nika:fetch — the closed mode set + arg pairings
            // (conformance stdlib/extract-modes/001..004).
            ("{}", "nika:fetch", true),                 // url: is REQUIRED
            ("{ mode: markdown }", "nika:fetch", true), // still no url
            (r#"{ url: "https://x.test" }"#, "nika:fetch", false), // default markdown
            (
                r#"{ url: "https://x.test", mode: article }"#,
                "nika:fetch",
                false,
            ),
            (
                r#"{ url: "https://x.test", mode: raw }"#,
                "nika:fetch",
                false,
            ),
            (
                r#"{ url: "https://x.test", mode: html }"#,
                "nika:fetch",
                true,
            ), // 001: not a mode
            (
                r#"{ url: "https://x.test", mode: llm-txt }"#,
                "nika:fetch",
                true,
            ), // RESERVED
            (
                r#"{ url: "https://x.test", mode: markdown, jq: ".x" }"#,
                "nika:fetch",
                true, // 003: jq only with mode: jq
            ),
            (
                r#"{ url: "https://x.test", jq: ".x" }"#,
                "nika:fetch",
                true, // jq with the DEFAULT mode (markdown) — same violation
            ),
            (
                r#"{ url: "https://x.test", mode: jq, jq: ".items | map(.name)" }"#,
                "nika:fetch",
                false, // 004: the valid pairing
            ),
            (r#"{ url: "https://x.test", mode: jq }"#, "nika:fetch", true), // jq needs jq:
            (
                r#"{ url: "https://x.test", mode: selector }"#,
                "nika:fetch",
                true,
            ), // needs selector:
            (
                r#"{ url: "https://x.test", mode: selector, selector: "div.c" }"#,
                "nika:fetch",
                false,
            ),
            (
                r#"{ url: "https://x.test", mode: text, selector: "div.c" }"#,
                "nika:fetch",
                true, // selector: only with mode: selector
            ),
            (
                r#"{ url: "https://x.test", mode: "${{ inputs.m }}" }"#,
                "nika:fetch",
                false, // templated mode — runtime business, statically silent
            ),
            (r#"{ url: "https://x.test", mode: 5 }"#, "nika:fetch", true), // not a string
        ];
        for (args, tool, violates) in &cases {
            let yaml = format!(
                "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke:\n      \
                 tool: \"{tool}\"\n      args: {args}\n"
            );
            assert_eq!(
                has_shape_error(&yaml, tool),
                *violates,
                "{tool} · args {args}"
            );
        }
    }

    #[test]
    fn fetch_payload_shape_rules_table() {
        // nika:fetch vNext — payload families (stdlib §fetch):
        // body ⊥ form ⊥ multipart · body-bearing method · closed part shape.
        let cases = [
            (
                r#"{ url: "https://x.test", form: { a: "b" } }"#,
                "nika:fetch",
                true, // form on the default GET — no body to carry
            ),
            (
                r#"{ url: "https://x.test", method: POST, form: { a: "b" } }"#,
                "nika:fetch",
                false, // the valid form pairing
            ),
            (
                r#"{ url: "https://x.test", method: post, form: { a: "b" } }"#,
                "nika:fetch",
                false, // method case-folds at runtime — static agrees
            ),
            (
                r#"{ url: "https://x.test", method: POST, form: { a: "b" }, body: "x" }"#,
                "nika:fetch",
                true, // body ⊥ form
            ),
            (
                r#"{ url: "https://x.test", method: "${{ vars.m }}", form: { a: "b" } }"#,
                "nika:fetch",
                false, // templated method — runtime business
            ),
            (
                r#"{ url: "https://x.test", method: POST, form: "nope" }"#,
                "nika:fetch",
                true, // form must be an object
            ),
            (
                r#"{ url: "https://x.test", method: PATCH, multipart: [{ name: f, value: v }] }"#,
                "nika:fetch",
                false, // valid text part on a body-bearing method
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: [] }"#,
                "nika:fetch",
                true, // needs at least one part
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: [{ name: f, value: v, path: p }] }"#,
                "nika:fetch",
                true, // exactly one of value | path
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: [{ name: f, value: v, surprise: 1 }] }"#,
                "nika:fetch",
                true, // unknown part key — the shape is closed
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: [{ name: f, value: v, filename: x }] }"#,
                "nika:fetch",
                true, // filename belongs to file parts
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: [{ name: f, path: "out/a.png" }] }"#,
                "nika:fetch",
                false, // valid file part
            ),
            (
                r#"{ url: "https://x.test", method: POST, multipart: "${{ tasks.prep.output }}" }"#,
                "nika:fetch",
                false, // fully-templated parts — runtime business
            ),
            (
                r#"{ url: "https://x.test", method: DELETE, multipart: [{ name: f, value: v }] }"#,
                "nika:fetch",
                true, // DELETE carries no body
            ),
        ];
        for (args, tool, violates) in &cases {
            let yaml = format!(
                "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    invoke:\n      \
                 tool: \"{tool}\"\n      args: {args}\n"
            );
            assert_eq!(
                has_shape_error(&yaml, tool),
                *violates,
                "{tool} · args {args}"
            );
        }
    }

    #[test]
    fn fetch_traverse_shape_rules_table() {
        // nika:fetch traverse — the bounded crawl (stdlib §fetch · traverse).
        let cases = [
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 5 } }"#,
                "nika:fetch",
                false, // the valid crawl
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 5, respect_robots: false } }"#,
                "nika:fetch",
                false, // robots opt-out is a bool field
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 0 } }"#,
                "nika:fetch",
                true, // below the range
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 26 } }"#,
                "nika:fetch",
                true, // above the cap
            ),
            (
                r#"{ url: "https://x.test", traverse: {} }"#,
                "nika:fetch",
                true, // max_pages is required
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 5, depth: 2 } }"#,
                "nika:fetch",
                true, // the shape is closed
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 5 }, mode: raw }"#,
                "nika:fetch",
                true, // traverse excludes the extraction args
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: 5 }, method: POST }"#,
                "nika:fetch",
                true, // GET only
            ),
            (
                r#"{ url: "https://x.test", traverse: "${{ vars.crawl }}" }"#,
                "nika:fetch",
                false, // fully-templated spec — runtime business
            ),
            (
                r#"{ url: "https://x.test", traverse: { max_pages: "${{ vars.n }}" } }"#,
                "nika:fetch",
                false, // templated field — runtime business
            ),
        ];
        for (args, tool, violates) in cases {
            let yaml = format!(
                "nika: v1\nworkflow: t\ntasks:\n  - id: a\n    invoke:\n      \
                 tool: \"{tool}\"\n      args: {args}\n"
            );
            assert_eq!(
                has_shape_error(&yaml, tool),
                violates,
                "{tool} · args {args}"
            );
        }
    }

    #[test]
    fn image_generate_v1_reservations_and_enum_rules() {
        assert_shape_cases(&[
            // nika:image_generate — V1 reservations · closed enums ·
            // ranges · size grammar · the transparent×jpeg conflict
            // (stdlib §Media). Flat required args (prompt/output_dir) are
            // the catalog missing-args check's concern — silent here.
            ("{}", "nika:image_generate", false),
            (
                r#"{ prompt: "x", output_dir: "./o", mode: edit }"#,
                "nika:image_generate",
                true, // edit without a source image → requires image:
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", mode: remix }"#,
                "nika:image_generate",
                true, // not a mode at all
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", mode: "${{ inputs.m }}" }"#,
                "nika:image_generate",
                false, // templated — runtime business
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", save: false }"#,
                "nika:image_generate",
                true, // V1: assets always land on disk
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", save: true }"#,
                "nika:image_generate",
                false,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", reference_images: ["a.png"] }"#,
                "nika:image_generate",
                true, // V1: text-to-image only
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", provider: midjourney }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", provider: mock }"#,
                "nika:image_generate",
                false,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", provider: local }"#,
                "nika:image_generate",
                false, // the sovereign path (v1.1)
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", provider: xai }"#,
                "nika:image_generate",
                false, // v1.1
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", format: gif }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", quality: hd }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", background: clear }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", aspect_ratio: "5:4" }"#,
                "nika:image_generate",
                true, // not in the closed common set
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", aspect_ratio: "16:9" }"#,
                "nika:image_generate",
                false,
            ),
        ]);
    }

    #[test]
    fn image_generate_range_size_and_conflict_rules() {
        assert_shape_cases(&[
            (
                r#"{ prompt: "x", output_dir: "./o", n: 0 }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", n: 11 }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", n: 3 }"#,
                "nika:image_generate",
                false,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", compression: 101 }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", timeout_ms: 999 }"#,
                "nika:image_generate",
                true,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", size: "1024" }"#,
                "nika:image_generate",
                true, // not WxH
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", size: auto }"#,
                "nika:image_generate",
                false,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", size: "1536x864" }"#,
                "nika:image_generate",
                false,
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", background: transparent, format: jpeg }"#,
                "nika:image_generate",
                true, // jpeg carries no alpha
            ),
            (
                r#"{ prompt: "x", output_dir: "./o", background: transparent, format: webp }"#,
                "nika:image_generate",
                false, // provider/model support is runtime business
            ),
        ]);
    }

    #[test]
    fn done_in_agent_whitelist_is_legal_and_on_finally_is_checked() {
        // The sentinel is LEGAL as an agent tools entry…
        let agent = "nika: v1\nworkflow: t\ntasks:\n  - id: l\n    agent:\n      \
                     prompt: \"go\"\n      tools: [\"nika:done\"]\n";
        assert!(!has_shape_error(agent, "nika:done"));
        // …and cleanup actions face the same shape rules as task actions —
        // a `nika:wait` with neither duration NOR until in an on_finally is
        // the XOR violation (a flat-required miss like `nika:write` content
        // is now the missing-args check's concern · tested there).
        let finally = "nika: v1\nworkflow: t\ntasks:\n  - id: w\n    \
                       exec: { command: echo }\n    on_finally:\n      - invoke:\n          \
                       tool: \"nika:wait\"\n          args: {}\n";
        assert!(has_shape_error(finally, "nika:wait"));
    }
}
