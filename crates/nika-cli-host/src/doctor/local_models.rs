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

    /// The `llamacpp` row after a sweep that saw `status`, whose route
    /// answered as an API (`reachable`), or `None` for no HTTP answer.
    fn swept(status: Option<u16>, reachable: Option<bool>) -> ProviderProbe {
        let mut row = nika_providers::probe::collect_access_probes_env(
            nika_providers::ProvidersConfig::new(),
        )
        .into_iter()
        .find(|p| p.id == "llamacpp")
        .expect("local row");
        let mut listing = nika_providers::probe::ModelListing::new("observed");
        listing.http_status = status;
        listing.protocol_compatible = status.map(|_| false);
        row.readiness.model_listing = Some(listing);
        row.readiness.reachable = reachable;
        row
    }

    /// #1626 · `ok ping llamacpp -- listening on 127.0.0.1:8080` over a web
    /// app: an open port is a candidate, never a model — but only a page
    /// that is not an API drops the ✔ (review round 1: Nika's own sidecar
    /// has no list route, a key-protected engine refuses an unkeyed list, a
    /// slow one times out, and each is still the engine answering).
    #[test]
    fn only_a_page_that_is_not_an_api_drops_the_ping() {
        let open = (
            "llamacpp".to_owned(),
            "127.0.0.1:8080".to_owned(),
            super::super::PingState::Reachable(0),
        );
        let row = |provider: &ProviderProbe| super::super::ping_finding(&open, Some(provider));
        let page = row(&swept(Some(200), Some(false)));
        assert_eq!(page.level, Level::Warn);
        assert_eq!(
            page.detail,
            "llamacpp — 127.0.0.1:8080 accepts connections (0ms) but answers a page, not an \
             API (HTTP 200, not JSON) — not a model server"
        );
        for (status, reachable, detail) in [
            (
                Some(404),
                Some(true),
                "llamacpp — listening on 127.0.0.1:8080 (0ms) · no /v1/models route",
            ),
            (
                Some(401),
                Some(true),
                "llamacpp — listening on 127.0.0.1:8080 (0ms) · key-protected",
            ),
            (
                Some(403),
                Some(true),
                "llamacpp — listening on 127.0.0.1:8080 (0ms) · key-protected",
            ),
            (
                Some(200),
                Some(true),
                "llamacpp — listening on 127.0.0.1:8080 (0ms)",
            ),
            (None, None, "llamacpp — listening on 127.0.0.1:8080 (0ms)"),
        ] {
            let shown = row(&swept(status, reachable));
            assert_eq!(shown.level, Level::Ok, "{status:?}");
            assert_eq!(shown.detail, detail, "{status:?}");
        }
        let media = (
            "image-local".to_owned(),
            "localhost:8080".to_owned(),
            super::super::PingState::Reachable(0),
        );
        assert_eq!(super::super::ping_finding(&media, None).level, Level::Ok);
    }
}
