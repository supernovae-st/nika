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
