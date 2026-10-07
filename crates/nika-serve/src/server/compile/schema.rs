// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `OpenAPI` fragments of the compile door, kept as data beside the handler.
//!
//! `openapi.json` holds the generation-1 contract every server publishes: the path, the request
//! and the outcome. `openapi-native.json` is the generation-2 contract a server that seats native
//! authoring adds to its live document, as an RFC 7386 merge patch over that whole document. Each
//! wire type and transport boundary is checked beside the handler; configured operator limits
//! remain dynamic and are tested through the live door.

use serde_json::{Map, Value};

const FOUNDATION: &str = include_str!("openapi.json");
const NATIVE: &str = include_str!("openapi-native.json");

/// One named fragment of the generation-1 contract.
fn fragment(name: &str) -> Value {
    serde_json::from_str::<Value>(FOUNDATION)
        .ok()
        .and_then(|mut fragments| fragments.get_mut(name).map(Value::take))
        .unwrap_or(Value::Null)
}

pub(in crate::server) fn path() -> Value {
    fragment("path")
}

pub(in crate::server) fn request() -> Value {
    fragment("CompileRequest")
}

pub(in crate::server) fn outcome() -> Value {
    fragment("CompileOutcome")
}

/// The document of a server that seats native authoring: the default document with the
/// generation-2 contract merged in.
pub(in crate::server) fn native(mut document: Value) -> Value {
    if let Ok(patch) = serde_json::from_str(NATIVE) {
        merge(&mut document, patch);
    }
    document
}

/// RFC 7386: an object merges key by key, `null` removes a key, anything else replaces. The
/// served document's one merge: the native compile patch here and the cost-review door's patch.
pub(in crate::server) fn merge(target: &mut Value, patch: Value) {
    let Value::Object(patch) = patch else {
        *target = patch;
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    if let Value::Object(target) = target {
        for (key, value) in patch {
            if value.is_null() {
                target.remove(&key);
            } else {
                merge(target.entry(key).or_insert(Value::Null), value);
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use nika_onboard::compile::{AuthoringCognition, COMPILE_WIRE_VERSION};
    use serde_json::json;

    use super::*;

    const DETERMINISTIC_ONLY: &str = AuthoringCognition::DeterministicOnly.word();
    const EXPLICIT_PROVIDER: &str = AuthoringCognition::ExplicitProvider.word();

    fn native_schemas() -> Value {
        native(json!({}))["components"]["schemas"].take()
    }

    #[test]
    fn both_resources_are_json_objects() {
        for (name, text) in [
            ("openapi.json", FOUNDATION),
            ("openapi-native.json", NATIVE),
        ] {
            let value: Value = serde_json::from_str(text).expect(name);
            assert!(value.is_object(), "{name}");
        }
        for name in ["path", "CompileRequest", "CompileOutcome"] {
            assert!(fragment(name).is_object(), "{name}");
        }
    }

    #[test]
    fn generation_one_publishes_the_transport_boundary_and_its_words() {
        let request = request();
        let properties = &request["properties"];
        assert_eq!(properties["compile_version"]["const"], COMPILE_WIRE_VERSION);
        assert_eq!(properties["cognition"]["const"], DETERMINISTIC_ONLY);
        for field in ["intent", "workflow_id", "source"] {
            assert!(properties[field].get("maxLength").is_none());
        }
        let change = &properties["change"]["properties"];
        assert!(change["text"].get("maxLength").is_none());
        assert!(
            change["set_constant"]["properties"]["name"]
                .get("maxLength")
                .is_none()
        );
        let answers = &properties["answers"];
        assert!(answers.get("maxProperties").is_none());
        assert!(answers.get("propertyNames").is_none());
        let described = request["description"].as_str().unwrap();
        assert!(described.contains("configured HTTP body ceiling"));
        assert!(described.starts_with(&format!("Generation {COMPILE_WIRE_VERSION} ")));
        let outcome = outcome();
        assert_eq!(
            outcome["properties"]["compile_version"]["const"],
            COMPILE_WIRE_VERSION
        );
        assert_eq!(
            outcome["properties"]["provenance"]["properties"]["cognition"]["const"],
            DETERMINISTIC_ONLY
        );
    }

    #[test]
    fn generation_two_publishes_typed_limits_without_legacy_product_ceilings() {
        let schemas = native_schemas();
        let request = &schemas["CompileRequestV2"]["properties"];
        assert_eq!(
            request["compile_version"]["const"],
            super::super::v2::GENERATION
        );
        assert_eq!(
            request["cognition"]["enum"],
            json!([EXPLICIT_PROVIDER, DETERMINISTIC_ONLY])
        );
        for field in ["intent", "original_intent", "workflow_id", "source"] {
            assert!(request[field].get("maxLength").is_none(), "{field}");
        }
        let token = format!("^[0-9a-f]{{{}}}$", super::super::v2::TOKEN_HEX);
        assert_eq!(request["replay_token"]["pattern"], token.as_str());
        let limits = &request["limits"]["properties"];
        assert_eq!(limits["repairs"]["maximum"], u32::MAX);
        assert_eq!(limits["max_tokens"]["maximum"], u32::MAX);
        assert!(limits["call_timeout_ms"].get("maximum").is_none());
        assert!(limits["deadline_ms"].get("maximum").is_none());
        let described = schemas["CompileRequestV2"]["description"].as_str().unwrap();
        assert!(described.contains("configured HTTP body ceiling"));
        // The operation and header state default retention and the process boundary.
        let document = native(json!({}));
        let post = &document["paths"]["/v1/compile"]["post"];
        let header = &post["responses"]["200"]["headers"]["Nika-Compile-Replay"];
        for described in [post, header] {
            let description = described["description"].as_str().unwrap();
            assert!(description.contains("no implicit count, size or expiry limit"));
            assert!(description.contains("forgotten on restart"));
            assert!(description.contains("explicit"));
            assert!(!description.contains("2097152"));
        }
        let outcome = &schemas["CompileOutcomeV2"]["properties"];
        assert_eq!(
            outcome["compile_version"]["const"],
            super::super::v2::GENERATION
        );
        assert_eq!(
            outcome["provenance"]["properties"]["cognition"]["const"],
            EXPLICIT_PROVIDER
        );
    }

    #[test]
    fn a_merge_patch_merges_objects_removes_nulls_and_replaces_the_rest() {
        let mut target = json!({"a": {"b": 1, "c": [1, 2], "d": "kept"}, "e": 2});
        merge(
            &mut target,
            json!({"a": {"b": 3, "c": [9], "d": null, "f": {"g": true}}, "e": {"h": 1}}),
        );
        assert_eq!(
            target,
            json!({"a": {"b": 3, "c": [9], "f": {"g": true}}, "e": {"h": 1}})
        );
    }
}
