// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-pack` — the embedded language pack for the Nika diamond.
//!
//! Every spec version ships a **pack**: the spec prose, the JSON Schema,
//! the canonical examples, the instantiable templates and the manifest
//! that hashes them (nika-spec README §The examples pack). This crate
//! embeds the pack of the language version this binary speaks, so
//! `nika try` / `nika new` / `nika spec` work
//! offline and an installed binary always carries the artifacts of
//! *its* version.
//!
//! This crate sits at **L0** (pure data): no I/O, no async, no error
//! enum — every accessor is a total function over compile-time bytes.
//! The snapshot under `pack/` is vendored by `scripts/sync-pack.sh` from the
//! exact nika-spec commit named by the engine root's `SPEC_PIN`; `pack/SPEC_SHA`
//! records that source identity. The runtime build refuses a pin/marker
//! mismatch. Manifest integrity tests cover the examples named by the public
//! manifest, while Diamond CI re-vendors and compares the whole mapped pack so
//! prose or registry drift fails before merge rather than reaching a user.
//!
//! ```rust
//! assert_eq!(nika_pack::pack_version(), "0.1.0-draft");
//! assert!(nika_pack::example("01-hello").is_some());
//! assert_eq!(nika_pack::example("hello"), nika_pack::example("01-hello"));
//! assert_eq!(nika_pack::first_shelf().len(), 5);
//! assert!(nika_pack::template("chain").is_some());
//! assert!(nika_pack::schema_json().contains("\"nika\""));
//! ```

use include_dir::{Dir, include_dir};

/// The vendored spec-pack snapshot (see `scripts/sync-pack.sh`).
static PACK: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/pack");

fn file_str(path: &str) -> Option<&'static str> {
    PACK.get_file(path).and_then(|f| f.contents_utf8())
}

/// The vendored motion SSOT (`design/motion.yaml` · spec #65) — the
/// per-verb motion identities every surface derives from.
#[must_use]
pub fn design_motion() -> Option<&'static str> {
    file_str("design-motion.yaml")
}

pub mod meta;
pub use meta::{ExampleMeta, meta};

/// The pack's language version — the spec repo's `VERSION` file, trimmed.
#[must_use]
pub fn pack_version() -> &'static str {
    file_str("VERSION").unwrap_or("").trim()
}

/// The raw pack manifest (`examples/manifest.yaml`) — workflow list,
/// tiers, constructs and the sha256 the integrity tests verify.
#[must_use]
pub fn manifest() -> &'static str {
    file_str("examples/manifest.yaml").unwrap_or("")
}

/// The canonical language registry (`canon.yaml`) — verbs, namespaces,
/// builtins, providers, extract modes, error namespaces.
#[must_use]
pub fn canon() -> &'static str {
    file_str("canon.yaml").unwrap_or("")
}

/// The shared visual vocabulary (`design-tokens.yaml` — the spec's
/// `design/tokens.yaml`, vendored): verb colors · severity · brand core.
/// `graph --format mermaid` derives its classDefs from it; a parity test
/// pins the renderer's hand-written consts against this file.
#[must_use]
pub fn design_tokens() -> &'static str {
    file_str("design-tokens.yaml").unwrap_or("")
}

/// One row of the canon's `error_codes` registry — the spec's normative
/// floor (`spec/05-errors.md` · projector-emitted flow-style rows).
/// Output-only: constructed solely by [`error_codes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ErrorCodeRow {
    /// The canonical code (`NIKA-DAG-003`).
    pub code: &'static str,
    /// The closed-enum category (`validation_error` · `variable_error` · …).
    pub category: &'static str,
    /// The transient marker (`false` · `engine-assessed`).
    pub transient: &'static str,
    /// The one-line failure description.
    pub failure: &'static str,
}

/// The canon's `error_codes` table, typed — THE one parser for the
/// registry rows (`nika explain`'s teach surface and the
/// emitted⊆registered ratchet both consume this; a second hand-rolled
/// parser of the same rows is the drift class this accessor kills).
///
/// The scan is anchored to the `error_codes:` section — a same-shaped
/// row in any other canon table is never served as an error code — and
/// a malformed row is skipped, never fatal (later rows stay reachable).
/// The quoted `failure` text is the LAST field, so commas inside it are
/// safe; the rows-stay-escape-free assumption is pinned by this crate's
/// tests at the projector seam.
#[must_use]
pub fn error_codes() -> Vec<ErrorCodeRow> {
    let mut out = Vec::new();
    let mut in_section = false;
    for line in canon().lines() {
        if line.starts_with("error_codes:") {
            in_section = true;
            continue;
        }
        if in_section && !line.trim().is_empty() && !line.starts_with([' ', '#']) {
            break; // next top-level key — the section is closed
        }
        if !in_section {
            continue;
        }
        let Some(rest) = line.trim_start().strip_prefix("- { code:") else {
            continue;
        };
        let Some((code, tail)) = rest.split_once(',') else {
            continue;
        };
        let (Some(category), Some(transient), Some(failure)) = (
            bare_field(tail, "category:"),
            quoted_field(tail, "transient:"),
            quoted_field(tail, "failure:"),
        ) else {
            continue;
        };
        out.push(ErrorCodeRow {
            code: code.trim(),
            category,
            transient,
            failure,
        });
    }
    out
}

/// The canon's `outcome_transitions` table, typed (spec 13 · W5) — THE
/// one parser for the normative `(class × cause)` law. The runtime's
/// Rust table parity-tests against THIS (never a private re-derivation),
/// and future consumers (the trace validator · the assertion layer)
/// read the same accessor. Output-only: constructed solely by
/// [`outcome_transitions`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OutcomeTransitions {
    /// The trace format the table binds to (`trace_format: 2`).
    pub trace_format: u32,
    /// The four terminal classes, in canon order.
    pub classes: Vec<&'static str>,
    /// class → its legal causes (canon order · the 10 rows).
    pub legal: Vec<(&'static str, Vec<&'static str>)>,
    /// class → its payload fields (`?`-suffixed = optional).
    pub payload: Vec<(&'static str, Vec<&'static str>)>,
}

/// Parse the canon's `outcome_transitions:` section (spec 13 · the ONE
/// machine-readable transition table). Same anchored-scan discipline as
/// [`error_codes`]: the scan is bound to the section, a malformed line
/// is skipped, and `None` = the section is absent or incomplete (a
/// drifted pack — the integrity tests fail before any user sees it).
#[must_use]
pub fn outcome_transitions() -> Option<OutcomeTransitions> {
    let mut in_section = false;
    // Which nested map the ≥4-indent rows belong to (`legal` · `payload`).
    let mut nest: Option<&str> = None;
    let mut trace_format: Option<u32> = None;
    let mut classes: Vec<&'static str> = Vec::new();
    let mut legal: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    let mut payload: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
    for line in canon().lines() {
        if line.starts_with("outcome_transitions:") {
            in_section = true;
            continue;
        }
        if in_section && !line.trim().is_empty() && !line.starts_with([' ', '#']) {
            break; // next top-level key — the section is closed
        }
        if !in_section {
            continue;
        }
        let item = line.trim_start();
        if item.is_empty() || item.starts_with('#') {
            continue;
        }
        let indent = line.len() - item.len();
        let Some((key, value)) = item.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if indent == 2 {
            match key {
                "trace_format" => trace_format = value.parse().ok(),
                "classes" => classes = flow_list(value),
                "legal" | "payload" => nest = Some(key),
                _ => nest = None,
            }
        } else if indent >= 4 {
            match nest {
                Some("legal") => legal.push((key, flow_list(value))),
                Some("payload") => payload.push((key, flow_list(value))),
                _ => {}
            }
        }
    }
    let trace_format = trace_format?;
    (!classes.is_empty() && !legal.is_empty() && !payload.is_empty()).then_some(
        OutcomeTransitions {
            trace_format,
            classes,
            legal,
            payload,
        },
    )
}

/// A YAML flow list `[a, b, "c?"]` → its items, quotes stripped
/// (the `?` optional-marker survives — it is data, not quoting).
fn flow_list(value: &'static str) -> Vec<&'static str> {
    let inner = value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or("");
    inner
        .split(',')
        .map(|item| item.trim().trim_matches('"'))
        .filter(|item| !item.is_empty())
        .collect()
}

/// `key: value` where the value runs to the next `,` or `}` (unquoted).
fn bare_field(tail: &'static str, key: &str) -> Option<&'static str> {
    let start = tail.find(key)? + key.len();
    let rest = &tail[start..];
    let end = rest.find([',', '}']).unwrap_or(rest.len());
    let value = rest[..end].trim();
    (!value.is_empty()).then_some(value)
}

/// `key: "value"` — the quoted form (commas inside stay intact).
fn quoted_field(tail: &'static str, key: &str) -> Option<&'static str> {
    let start = tail.find(key)? + key.len();
    let rest = tail[start..].trim_start();
    let inner = rest.strip_prefix('"')?;
    let end = inner.find('"')?;
    Some(&inner[..end])
}

/// The workflow JSON Schema — pipe it to editors
/// (`nika schema > workflow.schema.json`).
#[must_use]
pub fn schema_json() -> &'static str {
    file_str("schemas/workflow.schema.json").unwrap_or("")
}

/// The embedded QUICKSTART.
#[must_use]
pub fn quickstart() -> &'static str {
    file_str("QUICKSTART.md").unwrap_or("")
}

/// Bare `hello` is the 01-hello lesson — one file, one model, three doors
/// (`nika new hello` · `nika new 01-hello` · `nika try hello` / `01-hello`).
fn canonical_example_slug(slug: &str) -> &str {
    let slug = slug.strip_suffix(".nika.yaml").unwrap_or(slug);
    match slug {
        "hello" => "01-hello",
        other => other,
    }
}

/// One example by slug (`01-hello` or `showcase/t1-standup-digest` —
/// the `.nika.yaml` suffix is optional). `hello` is an alias of `01-hello`.
#[must_use]
pub fn example(slug: &str) -> Option<&'static str> {
    let slug = canonical_example_slug(slug);
    file_str(&format!("examples/{slug}.nika.yaml"))
        .or_else(|| file_str(&format!("examples/showcase/{slug}.nika.yaml")))
}

/// The five first-run jobs (UX-1). Bare `nika try` should show these;
/// the rest of the corpus lives behind `try --all`.
///
/// hello · brief · image · fetch · notify — in that order. Numbered
/// lessons that need a host toolchain (`03-exec-pipeline`) and git-repo
/// jobs (`standup-digest`) stay off this shelf so a weekend first-run
/// is five green rehearsals, not an opaque sandbox red.
#[must_use]
pub fn first_shelf() -> &'static [&'static str] {
    &[
        "01-hello",
        "ceo-monday-brief",
        "og-images",
        "05-fetch-chain",
        "release-notes",
    ]
}

/// Every file under `examples/fixtures/` — (path relative to
/// `examples/fixtures/`, raw bytes; photos are binary). The ingredients
/// a take (`nika new <slug>`) delivers beside a recipe that reads them (gauntlet
/// 2026-07-31: the recipe without its ingredients was the one
/// rage-quit — its own header taught an offline run that died on
/// NIKA-BUILTIN-READ-001).
#[must_use]
pub fn example_fixture_files() -> Vec<(&'static str, &'static [u8])> {
    fn walk(dir: &Dir<'static>, out: &mut Vec<(&'static str, &'static [u8])>) {
        for entry in dir.entries() {
            match entry {
                include_dir::DirEntry::File(f) => {
                    if let Some(tail) = f
                        .path()
                        .to_str()
                        .and_then(|p| p.strip_prefix("examples/fixtures/"))
                    {
                        out.push((tail, f.contents()));
                    }
                }
                include_dir::DirEntry::Dir(d) => walk(d, out),
            }
        }
    }
    let mut out = Vec::new();
    if let Some(dir) = PACK.get_dir("examples/fixtures") {
        walk(dir, &mut out);
    }
    out.sort_by_key(|(p, _)| *p);
    out
}

/// Every example slug (foundation first, then `showcase/<slug>`), sorted.
#[must_use]
pub fn example_slugs() -> Vec<String> {
    let mut out: Vec<String> = PACK
        .get_dir("examples")
        .map(collect_yaml_slugs)
        .unwrap_or_default();
    out.sort();
    out
}

fn collect_yaml_slugs(dir: &Dir<'static>) -> Vec<String> {
    let mut out = Vec::new();
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::File(f) => {
                if let Some(name) = f.path().to_str()
                    && let Some(slug) = name
                        .strip_prefix("examples/")
                        .and_then(|n| n.strip_suffix(".nika.yaml"))
                {
                    out.push(slug.to_owned());
                }
            }
            include_dir::DirEntry::Dir(d) => out.extend(collect_yaml_slugs(d)),
        }
    }
    out
}

/// One instantiable template by name (`chain` · `fanout` · …).
#[must_use]
pub fn template(name: &str) -> Option<&'static str> {
    let name = name.strip_suffix(".nika.yaml").unwrap_or(name);
    file_str(&format!("templates/{name}.nika.yaml"))
}

/// Every template name, sorted.
#[must_use]
pub fn template_names() -> Vec<String> {
    let mut out: Vec<String> = PACK
        .get_dir("templates")
        .map(|d| {
            d.files()
                .filter_map(|f| f.path().file_name()?.to_str())
                .filter_map(|n| n.strip_suffix(".nika.yaml"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// A spec or stdlib page by path (`spec/02-verbs.md` · `stdlib/builtins-v0.1.md`).
///
/// Lookups are literal keys into the compile-time snapshot — only paths
/// listed by [`doc_paths`] resolve; anything else (including traversal
/// attempts) returns `None`. There is no filesystem at runtime.
#[must_use]
pub fn doc(path: &str) -> Option<&'static str> {
    file_str(path)
}

/// Every doc page path (`spec/*.md` + `stdlib/*.md`), sorted.
#[must_use]
pub fn doc_paths() -> Vec<String> {
    let mut out = Vec::new();
    for dir_name in ["spec", "stdlib"] {
        if let Some(d) = PACK.get_dir(dir_name) {
            out.extend(
                d.files()
                    .filter_map(|f| f.path().to_str())
                    .map(str::to_owned),
            );
        }
    }
    out.sort();
    out
}

/// The lean text of a workflow — everything from the `nika:` envelope
/// line down, with trailing newlines trimmed. The manifest hash is
/// `sha256(lean + "\n")` — one canonical trailing newline, mirroring the
/// spec projector's python `lean()` (which keeps exactly one). Every
/// public surface (docs · website) renders this same range.
#[must_use]
pub fn lean(yaml_text: &str) -> &str {
    for (offset, line) in yaml_text.split_inclusive('\n').scan(0usize, |acc, l| {
        let start = *acc;
        *acc += l.len();
        Some((start, l))
    }) {
        if line.starts_with("nika:") {
            return yaml_text[offset..].trim_end_matches('\n');
        }
    }
    yaml_text
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn table() -> OutcomeTransitions {
        outcome_transitions().expect("the vendored canon carries outcome_transitions (spec 13)")
    }

    #[test]
    fn outcome_table_has_the_four_classes_and_ten_rows() {
        let t = table();
        assert_eq!(t.classes, ["success", "failure", "skipped", "cancelled"]);
        let rows: usize = t.legal.iter().map(|(_, causes)| causes.len()).sum();
        assert_eq!(rows, 10, "spec 13 · the table is exactly 10 rows");
        // Every legal/payload key IS a class, and every class has both.
        for (key, _) in t.legal.iter().chain(&t.payload) {
            assert!(t.classes.contains(key), "unknown class {key}");
        }
        assert_eq!(t.legal.len(), t.classes.len());
        assert_eq!(t.payload.len(), t.classes.len());
    }

    #[test]
    fn outcome_table_rows_match_spec_13() {
        let t = table();
        let causes = |class: &str| -> &[&str] {
            t.legal
                .iter()
                .find(|(k, _)| *k == class)
                .map(|(_, v)| v.as_slice())
                .expect("class present")
        };
        assert_eq!(causes("success"), ["normal", "recovered"]);
        assert_eq!(
            causes("failure"),
            ["verb_error", "timeout", "retry_exhausted"]
        );
        assert_eq!(causes("skipped"), ["gate", "error_skip"]);
        assert_eq!(causes("cancelled"), ["upstream", "operator", "budget"]);
    }

    #[test]
    fn outcome_payload_law_fields_parse_with_optional_markers() {
        let t = table();
        let fields = |class: &str| -> &[&str] {
            t.payload
                .iter()
                .find(|(k, _)| *k == class)
                .map(|(_, v)| v.as_slice())
                .expect("class present")
        };
        // The `?` marker survives parsing (quoted in YAML · data here).
        assert_eq!(fields("success"), ["value", "attempts", "recovered_from?"]);
        assert_eq!(fields("failure"), ["error", "attempts"]);
        assert_eq!(fields("skipped"), ["error?"]);
        assert_eq!(fields("cancelled"), ["reason"]);
    }

    #[test]
    fn outcome_table_binds_trace_format_2() {
        assert_eq!(table().trace_format, 2, "spec 13 · trace_format: 2");
    }
}
