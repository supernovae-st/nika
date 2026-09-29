// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Protocol observations remain separate from a listening port and live inference.
use super::{Finding, Level, ProviderProbe};

pub(super) fn finding(p: &ProviderProbe) -> Option<Finding> {
    let listing = p.readiness.model_listing.as_ref()?;
    let (level, detail) = match listing.available() {
        Some(true) => (
            Level::Ok,
            format!(
                "{} — compatible model list · {} advertised models · inference not tested",
                p.id,
                listing.models.len()
            ),
        ),
        Some(false) => (
            Level::Warn,
            format!("{} — compatible model list · no model advertised", p.id),
        ),
        None => (
            Level::Warn,
            format!(
                "{} — protocol/model availability unverified · {}",
                p.id,
                listing.failure.as_deref().unwrap_or("no usable model list")
            ),
        ),
    };
    Some(Finding {
        level,
        label: "models".into(),
        detail,
        fix: None,
    })
}

pub(super) fn json(raw: String, rows: &[ProviderProbe]) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return raw;
    };
    value["local_model_probes"] = serde_json::json!(rows.iter().filter_map(|p| {
        p.readiness.model_listing.as_ref().map(|listing| serde_json::json!({"provider": p.id, "observation": listing, "inference_tested": false}))
    }).collect::<Vec<_>>());
    format!("{value:#}")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_transport_failure_does_not_print_ready_or_claim_no_models() {
        let rows = nika_providers::probe::collect_access_probes_env(
            nika_providers::ProvidersConfig::new(),
        );
        let mut local = rows
            .into_iter()
            .find(|p| p.id == "ollama")
            .expect("local row");
        assert!(finding(&local).is_none());
        local.readiness.model_listing = Some(nika_providers::probe::ModelListing::new(
            "model-list transport did not return a usable response",
        ));
        let shown = finding(&local).expect("observation");
        assert_eq!(shown.level, Level::Warn);
        assert!(shown.detail.contains("unverified"));
        assert!(!shown.detail.contains("no model advertised"));
        let doc: serde_json::Value =
            serde_json::from_str(&json("{}".into(), &[local])).expect("JSON");
        assert_eq!(doc["local_model_probes"][0]["inference_tested"], false);
        assert!(doc["local_model_probes"][0]["observation"]["protocol_compatible"].is_null());
    }
}
