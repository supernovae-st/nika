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

    /// #1626 · `ok ping llamacpp -- listening on 127.0.0.1:8080` over a web
    /// app: an open port is a candidate, never a model. The ping row is ✔
    /// only when the engine answered an OpenAI-shaped model list; a surface
    /// with no list to ask (a media plane) keeps the connect verdict.
    #[test]
    fn an_open_port_is_an_ok_ping_only_with_a_model_list() {
        use nika_providers::probe::ModelListing;
        let open = (
            "llamacpp".to_owned(),
            "127.0.0.1:8080".to_owned(),
            super::super::PingState::Reachable(0),
        );
        let mut html = ModelListing::new("model-list response is not compatible");
        html.protocol_compatible = Some(false);
        html.http_status = Some(200);
        let row = super::super::ping_finding(&open, Some(&html));
        assert_eq!(row.level, Level::Warn);
        assert_eq!(
            row.detail,
            "llamacpp — 127.0.0.1:8080 accepts connections (0ms) but answers no model list \
             (model-list response is not compatible) — not a model server"
        );
        let mut listed = ModelListing::new("unused");
        listed.protocol_compatible = Some(true);
        listed.failure = None;
        listed.models = vec!["fake-local-model".to_owned()];
        let row = super::super::ping_finding(&open, Some(&listed));
        assert_eq!(row.level, Level::Ok);
        assert_eq!(row.detail, "llamacpp — listening on 127.0.0.1:8080 (0ms)");
        let media = (
            "image-local".to_owned(),
            "localhost:8080".to_owned(),
            super::super::PingState::Reachable(0),
        );
        assert_eq!(super::super::ping_finding(&media, None).level, Level::Ok);
    }
}
