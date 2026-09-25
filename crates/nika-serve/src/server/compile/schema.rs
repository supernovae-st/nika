// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `OpenAPI` fragments of the compile door, kept as data beside the handler.
//!
//! `openapi.json` holds the generation-1 contract every server publishes: the path, the request
//! and the outcome. `openapi-native.json` is the generation-2 contract a server that seats native
//! authoring adds to its live document, as an RFC 7386 merge patch over that whole document. Each
//! bound and word in them is pinned by this module's tests to the constant the handler enforces,
//! so the published contract cannot drift from the refusal.

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

/// RFC 7386: an object merges key by key, `null` removes a key, anything else replaces.
fn merge(target: &mut Value, patch: Value) {
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

    use super::super::{
        MAX_COMPILE_ANSWER_KEY_BYTES, MAX_COMPILE_ANSWERS, MAX_COMPILE_BODY_BYTES,
        MAX_COMPILE_LITERAL_BYTES, MAX_COMPILE_NAME_BYTES, MAX_COMPILE_SOURCE_BYTES,
        MAX_COMPILE_TEXT_BYTES,
    };
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
    fn every_generation_one_bound_and_word_is_the_enforced_constant() {
        let request = request();
        let properties = &request["properties"];
        assert_eq!(properties["compile_version"]["const"], COMPILE_WIRE_VERSION);
        assert_eq!(properties["cognition"]["const"], DETERMINISTIC_ONLY);
        assert_eq!(properties["intent"]["maxLength"], MAX_COMPILE_TEXT_BYTES);
        assert_eq!(
            properties["workflow_id"]["maxLength"],
            MAX_COMPILE_NAME_BYTES
        );
        assert_eq!(properties["source"]["maxLength"], MAX_COMPILE_SOURCE_BYTES);
        let change = &properties["change"]["properties"];
        assert_eq!(change["text"]["maxLength"], MAX_COMPILE_TEXT_BYTES);
        let constant = &change["set_constant"]["properties"];
        assert_eq!(constant["name"]["maxLength"], MAX_COMPILE_NAME_BYTES);
        let literal = MAX_COMPILE_LITERAL_BYTES.to_string();
        assert!(
            constant["value"]["description"]
                .as_str()
                .unwrap()
                .contains(&literal)
        );
        let answers = &properties["answers"];
        assert_eq!(answers["maxProperties"], MAX_COMPILE_ANSWERS);
        assert_eq!(
            answers["propertyNames"]["maxLength"],
            MAX_COMPILE_ANSWER_KEY_BYTES
        );
        assert!(answers["description"].as_str().unwrap().contains(&literal));
        let described = request["description"].as_str().unwrap();
        assert!(described.contains(&MAX_COMPILE_BODY_BYTES.to_string()));
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
    fn every_generation_two_bound_and_word_is_the_enforced_constant() {
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
        for field in ["intent", "original_intent"] {
            assert_eq!(
                request[field]["maxLength"], MAX_COMPILE_TEXT_BYTES,
                "{field}"
            );
        }
        assert_eq!(request["workflow_id"]["maxLength"], MAX_COMPILE_NAME_BYTES);
        assert_eq!(request["source"]["maxLength"], MAX_COMPILE_SOURCE_BYTES);
        let token = format!("^[0-9a-f]{{{}}}$", super::super::v2::TOKEN_HEX);
        assert_eq!(request["replay_token"]["pattern"], token.as_str());
        let limits = &request["limits"]["properties"];
        assert_eq!(limits["repairs"]["maximum"], 5);
        assert_eq!(limits["max_tokens"]["maximum"], 32_768);
        assert_eq!(limits["call_timeout_ms"]["maximum"], 600_000);
        assert_eq!(limits["deadline_ms"]["maximum"], 3_600_000);
        let described = schemas["CompileRequestV2"]["description"].as_str().unwrap();
        assert!(described.contains(&MAX_COMPILE_BODY_BYTES.to_string()));
        // The replay store's per-round bound, where the operation and the header state it.
        let kept = super::super::replay::MAX_ENTRY_BYTES.to_string();
        let document = native(json!({}));
        let post = &document["paths"]["/v1/compile"]["post"];
        assert!(post["description"].as_str().unwrap().contains(&kept));
        let header = &post["responses"]["200"]["headers"]["Nika-Compile-Replay"];
        assert!(header["description"].as_str().unwrap().contains(&kept));
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
