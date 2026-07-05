// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika catalog` — the embedded provider/model catalog on the wire.
//!
//! Until this verb the typed catalog (providers · models · capabilities ·
//! env-var names) was compiled into the binary but exported nowhere —
//! every editor/agent consumer re-bundled a copy and drifted. `--json`
//! emits the versioned projection (`catalog_version: 1`, built by
//! `nika-catalog::export`); the bare form prints the human teaching
//! listing.
//!
//! Teaching-surface ordering (the operator lock): LOCAL providers first
//! (sovereign · zero key · zero network), then cloud with the doctrine
//! head order (mistral · anthropic · openai), then the rest by id. The
//! `--json` machine payload keeps catalog order — functional surface,
//! exempt by design.

use nika_catalog::export::{CatalogExport, ProviderExport, catalog_export};

use crate::verbs::VerbOutput;

/// `nika catalog` — human listing, or the `--json` machine projection.
#[must_use]
pub fn run(json: bool) -> VerbOutput {
    let export = catalog_export();
    if json {
        return match serde_json::to_string_pretty(&export) {
            Ok(payload) => VerbOutput::ok(payload),
            Err(e) => VerbOutput::env(format!("catalog projection failed: {e}")),
        };
    }
    VerbOutput::ok(human_listing(&export))
}

/// The human teaching listing — sections LOCAL then CLOUD, doctrine order.
fn human_listing(export: &CatalogExport) -> String {
    let providers = export.providers.len();
    let models: usize = export.providers.iter().map(|p| p.models.len()).sum();

    let mut local: Vec<&ProviderExport> = export.providers.iter().filter(|p| p.local).collect();
    local.sort_by_key(|p| p.id);
    let mut cloud: Vec<&ProviderExport> = export.providers.iter().filter(|p| !p.local).collect();
    cloud.sort_by(|a, b| cloud_rank(a.id).cmp(&cloud_rank(b.id)));

    let mut out =
        format!("nika catalog — {providers} providers · {models} models (embedded · offline)\n");
    out.push_str("\nLOCAL (zero key · zero network)\n");
    for p in local {
        out.push_str(&provider_line(p));
    }
    // The runtime-resolvable engines the catalog DATA does not carry yet
    // (the local five today) — taught in the LOCAL block so the sovereign
    // path leads, derived LIVE from the provider profiles (zero drift).
    let catalog_ids: std::collections::BTreeSet<&str> =
        export.providers.iter().map(|p| p.id).collect();
    let runtime_only: Vec<&str> = nika_providers::CANONICAL_IDS
        .into_iter()
        .filter(|id| !catalog_ids.contains(id))
        .collect();
    if !runtime_only.is_empty() {
        use std::fmt::Write as _;
        let _ = writeln!(
            out,
            "  + runtime engines · {} (openai-compat · zero key)",
            runtime_only.join(" · "),
        );
    }
    out.push_str("\nCLOUD (key named by env var · the value is never read here)\n");
    for p in cloud {
        out.push_str(&provider_line(p));
    }
    out.push_str(
        "\nnika catalog --json → the machine projection (models · capabilities · context windows)",
    );
    out
}

/// Cloud head order: mistral (EU · open-weight) → anthropic → openai →
/// everyone else alphabetically.
fn cloud_rank(id: &str) -> (u8, &str) {
    let head = match id {
        "mistral" => 0,
        "anthropic" => 1,
        "openai" => 2,
        _ => 3,
    };
    (head, id)
}

/// One provider line of the human listing.
fn provider_line(p: &ProviderExport) -> String {
    let key = if p.requires_key { p.env_var } else { "no key" };
    format!("  {:<14} {:>2} models · {key}\n", p.id, p.models.len())
}

#[cfg(test)]
#[allow(clippy::panic)] // formatted assertion messages (the nika-mcp tests precedent)
mod tests {
    use super::*;
    use crate::verbs::exit;

    #[test]
    fn json_surface_is_the_versioned_catalog() {
        let out = run(true);
        assert_eq!(out.code, exit::OK);
        let value: serde_json::Value =
            serde_json::from_str(&out.text).expect("--json emits parseable JSON");
        assert_eq!(value["catalog_version"], 1, "the locked v1 wire marker");
        let providers = value["providers"].as_array().expect("providers array");
        assert!(!providers.is_empty(), "the embedded catalog is never empty");
        let first = providers[0].as_object().expect("provider object");
        for key in ["id", "env_var", "models", "local", "tags"] {
            assert!(first.contains_key(key), "provider entry missing `{key}`");
        }
    }

    #[test]
    fn human_surface_is_local_first_with_the_doctrine_cloud_order() {
        let out = run(false);
        assert_eq!(out.code, exit::OK);
        let text = out.text;
        let local = text.find("LOCAL").expect("a LOCAL section");
        let cloud = text.find("CLOUD").expect("a CLOUD section");
        assert!(local < cloud, "local providers lead the teaching surface");
        // The doctrine head order within CLOUD: mistral → anthropic → openai.
        let pos = |id: &str| {
            text.find(&format!("\n  {id}"))
                .unwrap_or_else(|| panic!("provider `{id}` missing from the listing"))
        };
        assert!(pos("mistral") < pos("anthropic"), "mistral leads anthropic");
        assert!(pos("anthropic") < pos("openai"), "anthropic leads openai");
        assert!(
            text.contains("--json"),
            "the human surface teaches the machine surface",
        );
        // The sovereign path leads: the runtime-resolvable local engines
        // (absent from the catalog DATA today) are taught in the LOCAL
        // block, before any cloud row — derived live, never hardcoded.
        let ollama = text
            .find("ollama")
            .expect("the runtime local engines are taught");
        assert!(
            ollama < cloud,
            "runtime local engines belong to the LOCAL block, before CLOUD",
        );
    }

    #[test]
    fn human_listing_counts_agree_with_the_projection() {
        let export = catalog_export();
        let text = human_listing(&export);
        let providers = export.providers.len();
        let models: usize = export.providers.iter().map(|p| p.models.len()).sum();
        assert!(
            text.contains(&format!("{providers} providers")),
            "the header states the provider count",
        );
        assert!(
            text.contains(&format!("{models} models")),
            "the header states the model count",
        );
    }
}
