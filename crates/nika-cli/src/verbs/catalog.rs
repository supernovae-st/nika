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
//! (sovereign · zero key), then cloud with the doctrine head order
//! (mistral · anthropic · openai), then the rest by id. The `zero
//! network` claim is EARNED per run (P0-20): printed only when every
//! local engine's EFFECTIVE endpoint resolves to loopback — an operator
//! override (`NIKA_<ID>_BASE_URL` · `OLLAMA_HOST`) pointing at a LAN box
//! drops the claim and gets its endpoint + locus NAMED on its row. The
//! `--json` machine payload keeps catalog order — functional surface,
//! exempt by design.

use nika_catalog::export::{CatalogExport, ProviderExport, catalog_export};
use nika_cli_host::machine_truth::MachineTruth;
use nika_providers::ProviderRegistry;
use nika_providers::probe::ExecutionLocus;

use crate::display::chrome;
use crate::display::theme::{Role, Theme};
use crate::verbs::VerbOutput;

/// One local engine's EFFECTIVE execution facts (P0-20) — the endpoint a
/// run would hit and where that endpoint executes. « local » is a
/// protocol, never a topology: an operator override (`NIKA_<ID>_BASE_URL`
/// · `OLLAMA_HOST`) pointing at the LAN box must be NAMED here, not
/// laundered under « zero network ».
struct LocalLocus {
    id: String,
    endpoint: String,
    locus: ExecutionLocus,
}

/// The local engines' loci, derived from the SAME env composition a run
/// uses (overrides included) — injected into [`human_listing`] so the
/// render stays pure and testable.
fn local_loci(registry: &ProviderRegistry) -> Vec<LocalLocus> {
    registry
        .profiles()
        .iter()
        .filter(|p| !p.requires_key && p.id != "mock")
        .map(|p| {
            let endpoint = registry.effective_base_url(p.id).unwrap_or(p.base_url);
            LocalLocus {
                id: p.id.to_owned(),
                endpoint: endpoint.to_owned(),
                locus: ExecutionLocus::classify(Some(endpoint), p.base_url),
            }
        })
        .collect()
}

/// The catalog projection with the resolvability fact ON every row
/// (#1184).
///
/// `catalog_export()` alone claims nothing about what this build can
/// reach; both surfaces below serialize to a reader who then CHOOSES a
/// provider, so both chain the adapter layer's own id list. The pin
/// lives in `tests/catalog_family.rs` — a surface that drops the chain
/// fails there rather than teaching an unreachable vendor.
fn resolvable_export() -> CatalogExport {
    catalog_export().with_resolvable(&nika_providers::CANONICAL_IDS)
}

/// `nika catalog` — human listing, or the `--json` machine projection.
#[must_use]
pub fn run(json: bool, theme: Theme) -> VerbOutput {
    let export = resolvable_export();
    if json {
        return match serde_json::to_string_pretty(&export) {
            Ok(payload) => VerbOutput::ok(payload),
            Err(e) => VerbOutput::env(format!("catalog projection failed: {e}")),
        };
    }
    // The same env composition a run uses — the listing classifies the
    // endpoints the runtime would ACTUALLY hit, overrides included.
    let registry = ProviderRegistry::without_http(nika_runtime::compose::config_from_env());
    let truth = MachineTruth::from_registry(&registry);
    VerbOutput::ok(human_listing(&export, theme, &local_loci(&registry), truth))
}

/// The two header lines — the counts, each NAMING its facet.
///
/// Every number names what it counts (RAMS-12 · A-06): « catalog
/// entries » here, « wired in this build » for what welcome counts,
/// « take a key » for the doctor's cloud rows — three facets of one
/// machine, never three contradictions. One derivation serves all three
/// (`MachineTruth`), and `machine_truth_surfaces.rs` pins the renders.
fn listing_header(theme: Theme, truth: MachineTruth, models: usize) -> String {
    let facets = theme.paint(
        Role::Dim,
        &format!(
            "  {wired} wired in this build · {slots} take a key — nika doctor shows their state",
            wired = truth.wired,
            slots = truth.cloud_key_slots,
        ),
    );
    format!(
        "nika catalog — {n} catalog entries · {models} models (embedded · offline)\n{facets}\n",
        n = truth.catalog_entries,
    )
}

/// The human teaching listing — sections LOCAL then CLOUD, doctrine order.
fn human_listing(
    export: &CatalogExport,
    theme: Theme,
    loci: &[LocalLocus],
    truth: MachineTruth,
) -> String {
    let models: usize = export.providers.iter().map(|p| p.models.len()).sum();

    // Three sections, because the GROUP is the marker. A per-row
    // « catalog only » suffix was correct and unreadable: 22 of 38 rows
    // repeated the same 30 characters, so the fact a reader needed had
    // to be found by scanning for its ABSENCE. LOCAL and CLOUD now hold
    // only what this build resolves, in the doctrine order; everything
    // the engine has no adapter for sits under its own head, where one
    // sentence covers all of them.
    let mut local: Vec<&ProviderExport> = export
        .providers
        .iter()
        .filter(|p| p.local && p.resolves)
        .collect();
    local.sort_by_key(|p| p.id);
    let mut cloud: Vec<&ProviderExport> = export
        .providers
        .iter()
        .filter(|p| !p.local && p.resolves)
        .collect();
    cloud.sort_by(|a, b| cloud_rank(a.id).cmp(&cloud_rank(b.id)));
    let mut catalog_only: Vec<&ProviderExport> =
        export.providers.iter().filter(|p| !p.resolves).collect();
    catalog_only.sort_by_key(|p| p.id);

    let mut out = listing_header(theme, truth, models);
    out.push('\n');
    // « zero network » is EARNED: only when every local engine resolves
    // to loopback. One override off-loopback and the claim falls.
    let zero_network = loci.iter().all(|l| l.locus == ExecutionLocus::Loopback);
    out.push_str(&chrome::rail_head(
        theme,
        if zero_network {
            "LOCAL (zero key · zero network)"
        } else {
            "LOCAL (zero key)"
        },
    ));
    out.push('\n');
    for p in local {
        out.push_str(&provider_line(p, theme, loci.iter().find(|l| l.id == p.id)));
    }
    // The runtime-resolvable engines the catalog DATA does not carry yet
    // (the local five today) — taught in the LOCAL block so the sovereign
    // path leads, derived LIVE from the provider profiles (zero drift).
    let catalog_ids: std::collections::BTreeSet<&str> =
        export.providers.iter().map(|p| p.id).collect();
    let runtime_only: Vec<String> = nika_providers::CANONICAL_IDS
        .into_iter()
        .filter(|id| !catalog_ids.contains(id))
        .map(|id| match loci.iter().find(|l| l.id == id) {
            // An override-pointed engine is NAMED with its endpoint and
            // locus — the ink the honest header saved goes here.
            Some(l) if l.locus != ExecutionLocus::Loopback => format!(
                "{id} → {} ({})",
                crate::verbs::doctor::redact_userinfo(&l.endpoint),
                l.locus.label()
            ),
            _ => (*id).to_owned(),
        })
        .collect();
    if !runtime_only.is_empty() {
        use std::fmt::Write as _;
        let _ = writeln!(
            out,
            "  + runtime engines · {} (openai-compat · zero key)",
            runtime_only.join(" · "),
        );
    }
    out.push('\n');
    out.push_str(&chrome::rail_head(
        theme,
        "CLOUD (key named by env var · the value is never read here)",
    ));
    out.push('\n');
    for p in cloud {
        out.push_str(&provider_line(p, theme, None));
    }
    if !catalog_only.is_empty() {
        out.push('\n');
        out.push_str(&chrome::rail_head(
            theme,
            "CATALOG ONLY (no adapter in this build · nika check refuses these)",
        ));
        out.push('\n');
        for p in catalog_only {
            out.push_str(&provider_line(p, theme, None));
        }
    }
    out.push_str(
        "\nnika catalog --json → the machine projection (models · capabilities · context windows · resolves)",
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

/// One provider line of the human listing. A local engine whose
/// EFFECTIVE endpoint sits off-loopback (an operator override) is NAMED
/// with endpoint + locus — the ink the honest header saved (P0-20).
///
/// The row carries no resolvability marker: its SECTION does (#1184).
/// An aggregate in the header does not survive the scan a user actually
/// performs — but neither does the same suffix repeated on 22 of 38
/// rows, which is what marking each one produced.
fn provider_line(p: &ProviderExport, theme: Theme, locus: Option<&LocalLocus>) -> String {
    // B-6b (the gauntlet's Marta): `native` sits under the « zero key ·
    // zero network » banner, but in-process GGUF inference needs the
    // file ON DISK first — the row names the prerequisite and its door,
    // never a bare « no key » that dead-ends at run.
    //
    // The call to action is gated on `resolves`: `native` does not
    // resolve in this build, so « nika model pull » invited a user
    // through a door that refuses at check. An unreachable row never
    // carries a verb.
    let key = if p.requires_key {
        p.env_var
    } else if p.id == "native" && p.resolves {
        "no key · needs a local .gguf (nika model pull)"
    } else {
        "no key"
    };
    let locus_note = match locus {
        Some(l) if l.locus != ExecutionLocus::Loopback => format!(
            " → {} ({})",
            crate::verbs::doctor::redact_userinfo(&l.endpoint),
            l.locus.label()
        ),
        _ => String::new(),
    };
    format!(
        "{}\n",
        chrome::rail_line(
            theme,
            &format!(
                " {}{}{}",
                theme.paint(Role::Strong, &format!("{:<14}", p.id)),
                theme.paint(Role::Dim, &format!(" {:>2} models · {key}", p.models.len()),),
                theme.paint(Role::Dim, &locus_note),
            ),
        )
    )
}

#[cfg(test)]
#[allow(clippy::panic)] // formatted assertion messages (the nika-mcp tests precedent)
mod tests {
    use super::*;
    use crate::verbs::exit;

    const PLAIN: Theme = Theme::new(false, false, false);

    #[test]
    fn json_surface_is_the_versioned_catalog() {
        let out = run(true, PLAIN);
        assert_eq!(out.code, exit::OK);
        let value: serde_json::Value =
            serde_json::from_str(&out.text).expect("--json emits parseable JSON");
        assert_eq!(value["catalog_version"], 1, "the locked v1 wire marker");
        let providers = value["providers"].as_array().expect("providers array");
        assert!(!providers.is_empty(), "the embedded catalog is never empty");
        let first = providers[0].as_object().expect("provider object");
        for key in ["id", "env_var", "models", "local", "tags", "resolves"] {
            assert!(first.contains_key(key), "provider entry missing `{key}`");
        }
    }

    /// #1184 · the machine surface must let a consumer recover the
    /// distinction the human header states. A payload where every row
    /// reads `resolves: false` is `catalog_export()` un-chained — the
    /// exact drop this field exists to make impossible.
    #[test]
    fn json_marks_the_adapter_set_row_by_row() {
        let out = run(true, PLAIN);
        let value: serde_json::Value = serde_json::from_str(&out.text).expect("parseable JSON");
        let providers = value["providers"].as_array().expect("providers array");
        let resolving: Vec<&str> = providers
            .iter()
            .filter(|p| p["resolves"] == serde_json::Value::Bool(true))
            .filter_map(|p| p["id"].as_str())
            .collect();
        assert!(
            !resolving.is_empty(),
            "a payload with zero resolving rows is the un-chained projection",
        );
        for id in &resolving {
            assert!(
                nika_providers::CANONICAL_IDS.contains(id),
                "`{id}` is marked resolving but no adapter carries it",
            );
        }
        for id in nika_providers::CANONICAL_IDS {
            if providers.iter().any(|p| p["id"] == id) {
                assert!(
                    resolving.contains(&id),
                    "`{id}` has an adapter but its row is unmarked",
                );
            }
        }
    }

    /// The row a user scans carries the fact — and an unreachable row
    /// never carries a call to action (`native` invited « nika model
    /// pull » into a door that refuses at check).
    #[test]
    fn a_non_resolving_row_sits_below_the_head_and_offers_no_verb() {
        let text = run(false, PLAIN).text;
        let export = resolvable_export();
        let head = text
            .find("CATALOG ONLY")
            .expect("the listing separates what runs from what it merely knows");
        for p in &export.providers {
            let at = text
                .find(&format!("\u{2502}  {:<14}", p.id))
                .unwrap_or_else(|| panic!("provider `{}` missing from the listing", p.id));
            assert_eq!(
                at < head,
                p.resolves,
                "provider `{}` sits in the wrong block (resolves={})",
                p.id,
                p.resolves,
            );
        }
        // An unreachable row carries no call to action: `native` invited
        // « nika model pull » into a door that refuses at check.
        let below = &text[head..];
        assert!(
            !below.contains("nika model pull"),
            "a CATALOG ONLY row still invites a verb:\n{below}",
        );
    }

    #[test]
    fn human_surface_is_local_first_with_the_doctrine_cloud_order() {
        let out = run(false, PLAIN);
        assert_eq!(out.code, exit::OK);
        let text = out.text;
        let local = text.find("LOCAL").expect("a LOCAL section");
        let cloud = text.find("CLOUD").expect("a CLOUD section");
        assert!(local < cloud, "local providers lead the teaching surface");
        // The doctrine head order within CLOUD: mistral → anthropic → openai.
        let pos = |id: &str| {
            // The rail grammar: `│  <id>` opens every provider row.
            text.find(&format!("\n\u{2502}  {id}"))
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

    /// A hand-built truth with three DISTINCT numbers — the render test
    /// stays pure (injected facts, no env), and distinct values prove
    /// each rendered number reads its own facet, never a neighbour's.
    fn distinct_truth(export: &CatalogExport) -> MachineTruth {
        MachineTruth {
            catalog_entries: export.providers.len(),
            wired: 7,
            cloud_key_slots: 3,
        }
    }

    #[test]
    fn human_listing_counts_agree_with_the_projection() {
        let export = resolvable_export();
        let truth = distinct_truth(&export);
        let text = human_listing(&export, PLAIN, &loopback_loci(), truth);
        let models: usize = export.providers.iter().map(|p| p.models.len()).sum();
        assert!(
            text.contains(&format!("{} catalog entries", truth.catalog_entries)),
            "the header NAMES its facet (RAMS-12):\n{text}",
        );
        assert!(
            text.contains(&format!("{models} models")),
            "the header states the model count",
        );
        assert!(
            text.contains("7 wired in this build"),
            "the wired facet is named under the header:\n{text}",
        );
        assert!(
            text.contains("3 take a key"),
            "the key-slot facet is named under the header:\n{text}",
        );
    }

    /// Every local engine on its loopback seed — the shape the real
    /// registry yields with zero operator override.
    fn loopback_loci() -> Vec<LocalLocus> {
        ["ollama", "lmstudio", "llamacpp", "localai", "vllm"]
            .into_iter()
            .map(|id| LocalLocus {
                id: id.to_owned(),
                endpoint: "http://127.0.0.1:1".to_owned(),
                locus: ExecutionLocus::Loopback,
            })
            .collect()
    }

    #[test]
    fn zero_network_is_earned_by_loopback_only() {
        let export = resolvable_export();
        // All locals on loopback → the sovereign claim holds.
        let text = human_listing(&export, PLAIN, &loopback_loci(), distinct_truth(&export));
        assert!(
            text.contains("LOCAL (zero key · zero network)"),
            "loopback keeps the claim:\n{text}"
        );
        // What `NIKA_OLLAMA_BASE_URL=http://192.168.1.50:11434` becomes
        // through the compose ladder — INJECTED as locus facts (the
        // house hermetic pattern: no `set_var` race). The claim must
        // fall, and the endpoint + locus must be NAMED (P0-20).
        let mut loci = loopback_loci();
        loci[0] = LocalLocus {
            id: "ollama".to_owned(),
            endpoint: "http://192.168.1.50:11434".to_owned(),
            locus: ExecutionLocus::Lan,
        };
        let text = human_listing(&export, PLAIN, &loci, distinct_truth(&export));
        assert!(
            !text.contains("zero network"),
            "a LAN engine never launders as « zero network »:\n{text}"
        );
        assert!(
            text.contains("192.168.1.50:11434"),
            "the effective endpoint is named:\n{text}"
        );
        assert!(text.contains("(lan)"), "the locus is named:\n{text}");
        // …while the un-overridden engines keep their bare ids.
        assert!(text.contains("lmstudio"), "{text}");
    }
}
