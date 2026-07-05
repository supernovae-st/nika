// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Type-aware bounded output summaries — the storyboard's shape tail.
//!
//! One law: **shape, never a data dump**. An object reads as its keys
//! (`{verdict, findings[4], summary}`), an array as its length (`[12]`),
//! a string as its bounded head (`"Horaires des …"`), scalars literally.
//! Every summary is deterministic and width-bounded, so a tail can ride
//! a live storyboard row without ever flooding it.
//!
//! Secrets: the runtime drops the `output` trace field wholesale when a
//! task's output text carries a resolved secret value (ADR-099 §1 — the
//! stamp filter), so no summary here can ever see one. This module adds
//! NO secret read of its own; the bounded width is defence in depth.

use crate::display::theme::{Role, Theme};

/// Widest a shape summary grows (display cells) before it ellipsizes —
/// keeps a typical storyboard row graceful under 80 columns.
pub(crate) const SHAPE_CELLS: usize = 24;

/// A byte size for humans: `89B` · `1.2KB` · `3.4MB` (no space — the
/// tail vocabulary, distinct from runtime note prose like `34 KB`).
#[must_use]
pub fn fmt_bytes(n: usize) -> String {
    if n < 1_000 {
        return format!("{n}B");
    }
    #[allow(clippy::cast_precision_loss)] // display-only magnitude
    let f = n as f64;
    if n < 1_000_000 {
        return format!("{:.1}KB", f / 1_000.0);
    }
    format!("{:.1}MB", f / 1_000_000.0)
}

/// Summarize one task output (its compact JSON text) as a bounded,
/// type-aware shape. `None` when the text is not valid JSON — never
/// render garbage from a hand-edited trace.
#[must_use]
pub fn summarize(json_text: &str, max_cells: usize) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json_text).ok()?;
    Some(fit(&shape_of(&value), max_cells))
}

/// The unbounded shape text for one JSON value (fit separately).
fn shape_of(value: &serde_json::Value) -> String {
    use serde_json::Value;
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => format!("\"{}\"", s.replace(['\n', '\r'], " ")),
        Value::Array(items) => format!("[{}]", items.len()),
        Value::Object(map) => {
            // Key names only (values stay in `peek`) — an array-valued
            // key carries its length so the DAG's fan-out reads at a
            // glance. Four keys shown, the rest counted by `…`.
            let mut keys: Vec<String> = map
                .iter()
                .take(4)
                .map(|(k, v)| match v {
                    Value::Array(items) => format!("{k}[{}]", items.len()),
                    _ => k.clone(),
                })
                .collect();
            if map.len() > 4 {
                keys.push("…".to_owned());
            }
            format!("{{{}}}", keys.join(", "))
        }
    }
}

/// Truncate to `max` display cells, ellipsis-marked — a string head keeps
/// its closing quote so the shape stays readable when clipped.
fn fit(shape: &str, max: usize) -> String {
    if shape.chars().count() <= max {
        return shape.to_owned();
    }
    let quoted = shape.starts_with('"');
    // Reserve the ellipsis (and the closing quote for a string head).
    let keep = max.saturating_sub(if quoted { 2 } else { 1 });
    let mut out: String = shape.chars().take(keep).collect();
    out.push('…');
    if quoted {
        out.push('"');
    }
    out
}

/// The assembled row tail: `→ <shape> · <size>[ · <tok> tok]` — painted
/// dim as one metadata unit, ASCII-parity arrow (`->`). `None` when the
/// row carries no output (a skip · a failure · the engine's secret-drop).
// `&Theme` to match the render borrow that threads it here — the same
// one-calling-convention rationale as `frame_impl`.
#[allow(clippy::trivially_copy_pass_by_ref)]
#[must_use]
pub fn output_tail(
    output_json: Option<&str>,
    tokens: Option<u64>,
    theme: &Theme,
) -> Option<String> {
    use std::fmt::Write as _;
    let text = output_json?;
    let shape = summarize(text, SHAPE_CELLS)?;
    let arrow = crate::display::vocab::arrow(theme.ascii);
    let mut tail = format!("{arrow} {shape} · {}", fmt_bytes(text.len()));
    if let Some(tok) = tokens {
        let _ = write!(tail, " · {tok} tok");
    }
    Some(theme.paint(Role::Dim, &tail))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Theme = Theme::new(false, false, false);
    const ASCII: Theme = Theme::new(false, true, false);

    /// Objects read as their key set — array-valued keys carry `[N]`,
    /// values never leak into the shape (the design's one law).
    #[test]
    fn object_shape_is_keys_with_array_lengths() {
        let json = r#"{"verdict":"P0","findings":[1,2,3,4],"summary":"long text"}"#;
        assert_eq!(
            summarize(json, 60).as_deref(),
            Some("{findings[4], summary, verdict}"),
            "serde_json map iteration is key-sorted (BTreeMap-backed)"
        );
        assert!(
            !summarize(json, 60).expect("shape").contains("P0"),
            "values never leak into an object shape"
        );
    }

    /// More than four keys collapse into `…` — the fan-out stays bounded
    /// however wide the object grows.
    #[test]
    fn object_shape_caps_at_four_keys() {
        let json = r#"{"a":1,"b":2,"c":3,"d":4,"e":5,"f":6}"#;
        assert_eq!(summarize(json, 60).as_deref(), Some("{a, b, c, d, …}"));
    }

    /// Strings render their quoted head; arrays their length; scalars
    /// literally — one deterministic form per JSON type.
    #[test]
    fn string_array_and_scalar_shapes() {
        let horaires = "\"Horaires des marées pour demain matin à Saint-Malo\"";
        let s = summarize(horaires, SHAPE_CELLS).expect("string shape");
        assert!(s.starts_with("\"Horaires des"), "head kept: {s}");
        assert!(s.ends_with("…\""), "clipped head keeps its quote: {s}");
        assert_eq!(summarize("[1,2,3]", 60).as_deref(), Some("[3]"));
        assert_eq!(summarize("42", 60).as_deref(), Some("42"));
        assert_eq!(summarize("true", 60).as_deref(), Some("true"));
        assert_eq!(summarize("null", 60).as_deref(), Some("null"));
    }

    /// Nested containers stay ONE level deep: an object-valued key reads
    /// as its bare name, a nested array only by its top-level count —
    /// depth never explodes the tail.
    #[test]
    fn nesting_never_descends_past_level_one() {
        let json = r#"{"head":{"title":"x","deep":{"more":1}},"items":[[1,2],[3]]}"#;
        assert_eq!(summarize(json, 60).as_deref(), Some("{head, items[2]}"));
    }

    /// The width bound holds for every type — a shape never exceeds its
    /// cell budget (the storyboard row's graceful-under-80 guarantee).
    #[test]
    fn width_bound_holds_for_every_type() {
        let long_string = format!("\"{}\"", "x".repeat(500));
        let wide_object = format!(
            "{{{}}}",
            (0..8)
                .map(|i| format!("\"very_long_key_name_{i}\":1"))
                .collect::<Vec<_>>()
                .join(",")
        );
        for json in [long_string.as_str(), wide_object.as_str()] {
            let s = summarize(json, SHAPE_CELLS).expect("shape");
            assert!(
                s.chars().count() <= SHAPE_CELLS,
                "bounded ≤{SHAPE_CELLS}: {s} ({})",
                s.chars().count()
            );
        }
    }

    /// Not-JSON renders NOTHING — a truncated or hand-edited trace field
    /// must never paint garbage on the storyboard.
    #[test]
    fn invalid_json_summarizes_to_none() {
        assert_eq!(summarize("{not json", 60), None);
        assert_eq!(summarize("", 60), None);
    }

    /// The engine invariant extends to previews structurally: a resolved
    /// secret value never reaches this module because the runtime drops
    /// the whole `output` trace field when the output text leaks one
    /// (ADR-099 §1 stamp filter) — the tail then has NO input at all.
    /// Pinned here as the no-output arm; the fold-side test pins that a
    /// field-less completion folds to `output_json: None`.
    #[test]
    fn no_output_means_no_tail_the_secret_drop_arm() {
        assert_eq!(output_tail(None, Some(90), &PLAIN), None);
    }

    /// The assembled tail: arrow · shape · byte size · tokens — with
    /// full ASCII parity (`→` → `->`) and byte-size ramps.
    #[test]
    fn tail_assembles_arrow_shape_size_and_tokens() {
        let json = r#"{"verdict":"P0","findings":[1,2]}"#;
        assert_eq!(
            output_tail(Some(json), Some(90), &PLAIN).as_deref(),
            Some("→ {findings[2], verdict} · 33B · 90 tok")
        );
        assert_eq!(
            output_tail(Some(json), None, &ASCII).as_deref(),
            Some("-> {findings[2], verdict} · 33B"),
            "no tokens reported → no tok segment · ascii arrow"
        );
        let ascii = output_tail(Some(json), Some(1), &ASCII).expect("tail");
        assert!(!ascii.contains('→'), "no unicode leaks into --ascii");
    }

    #[test]
    fn byte_format_scales() {
        assert_eq!(fmt_bytes(0), "0B");
        assert_eq!(fmt_bytes(89), "89B");
        assert_eq!(fmt_bytes(999), "999B");
        assert_eq!(fmt_bytes(1_200), "1.2KB");
        assert_eq!(fmt_bytes(999_949), "999.9KB");
        assert_eq!(fmt_bytes(3_400_000), "3.4MB");
    }
}
