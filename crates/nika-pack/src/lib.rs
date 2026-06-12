// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-pack` — the embedded language pack for the Nika diamond.
//!
//! Every spec version ships a **pack**: the spec prose, the JSON Schema,
//! the canonical examples, the instantiable templates and the manifest
//! that hashes them (nika-spec README §The examples pack). This crate
//! embeds the pack of the language version this binary speaks, so
//! `nika examples` / `nika docs` / `nika schema` / `nika new` work
//! offline and an installed binary always carries the artifacts of
//! *its* version.
//!
//! This crate sits at **L0** (pure data): no I/O, no async, no error
//! enum — every accessor is a total function over compile-time bytes.
//! The snapshot under `pack/` is vendored by `scripts/sync-pack.sh`
//! and verified against the manifest hashes by the integrity tests
//! (a tampered or drifted pack fails `cargo test`, not a user).
//!
//! ```rust
//! assert_eq!(nika_pack::pack_version(), "0.1.0-draft");
//! assert!(nika_pack::example("01-hello").is_some());
//! assert!(nika_pack::template("chain").is_some());
//! assert!(nika_pack::schema_json().contains("\"nika\""));
//! ```

use include_dir::{Dir, include_dir};

/// The vendored spec-pack snapshot (see `scripts/sync-pack.sh`).
static PACK: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/pack");

fn file_str(path: &str) -> Option<&'static str> {
    PACK.get_file(path).and_then(|f| f.contents_utf8())
}

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

/// One example by slug (`01-hello` or `showcase/t1-standup-digest` —
/// the `.nika.yaml` suffix is optional).
#[must_use]
pub fn example(slug: &str) -> Option<&'static str> {
    let slug = slug.strip_suffix(".nika.yaml").unwrap_or(slug);
    file_str(&format!("examples/{slug}.nika.yaml"))
        .or_else(|| file_str(&format!("examples/showcase/{slug}.nika.yaml")))
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
