// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Resolved bindings — re-exported from nika-core (S23-A3).
//!
//! The resolver — and the bulk of its unit tests — now live in
//! `nika_core::binding::resolve`. This module exists only to:
//!
//! 1. keep the `crate::binding::resolve::{LazyBinding, ResolvedBindings}`
//!    path alive for intra-engine callers (28 sites);
//! 2. host the engine-side integration tests that need `RunContext`,
//!    `TaskResult`, the vault, or the media pipeline — i.e. anything
//!    the L0 core cannot pull in without breaking layering.
//!
//! After S24-A4, the re-export block for private helpers
//! (`split_path`, `resolve_entry`, …) is gone: every test that
//! reached into those internals now lives in nika-core itself and
//! sees them via `use super::*`.

pub use nika_core::binding::resolve::{LazyBinding, ResolvedBindings};

#[cfg(test)]
mod tests {

    use super::*;
    use crate::binding::{BindingEntry, BindingSpec, BindingType, WithEntry, WithSpec};
    use crate::binding::types::BindingPath;
    use crate::store::{RunContext, TaskResult};
    use nika_core::binding::transform::TransformExpr;
    use serde_json::json;
    use serial_test::serial;
    use std::sync::Arc;
    use std::time::Duration;

    // ═══════════════════════════════════════════════════════════════
    // Basic tests (common API)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn get_resolved_re_resolves_on_each_call() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"counter": 1}), Duration::from_secs(1)),
        );

        let mut spec = BindingSpec::default();
        spec.insert("lazy".to_string(), BindingEntry::new_lazy("task.counter"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        // First call
        let result1 = bindings.get_resolved("lazy", &store).unwrap();
        assert_eq!(result1, json!(1));

        // Update store
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"counter": 2}), Duration::from_secs(1)),
        );

        // Second call - should reflect new value (lazy bindings don't cache)
        let result2 = bindings.get_resolved("lazy", &store).unwrap();
        assert_eq!(result2, json!(2));
    }
    #[test]
    #[serial]
    fn with_spec_env_existing_var() {
        // Use a known env var
        std::env::set_var("NIKA_TEST_VAR_8A", "test_value_8a");

        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        let mut spec = WithSpec::default();
        spec.insert(
            "my_var".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_TEST_VAR_8A").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("my_var"), Some(&json!("test_value_8a")));

        std::env::remove_var("NIKA_TEST_VAR_8A");
    }
    #[test]
    fn with_spec_lazy_re_resolves() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"counter": 1}), Duration::from_secs(1)),
        );

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$step1.counter").unwrap());
        entry.lazy = true;
        spec.insert("counter".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();

        let v1 = bindings.get_resolved("counter", &store).unwrap();
        assert_eq!(v1, json!(1));

        // Update store
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"counter": 42}), Duration::from_secs(1)),
        );

        let v2 = bindings.get_resolved("counter", &store).unwrap();
        assert_eq!(v2, json!(42));
    }
    #[test]
    #[serial]
    fn with_spec_mixed_sources() {
        use rustc_hash::FxHashMap;

        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Task output
        store.insert(
            Arc::from("step1"),
            TaskResult::success(json!({"result": "task_val"}), Duration::from_secs(1)),
        );

        // Inputs
        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({"type": "string", "default": "AI"}),
        );
        store.set_inputs(inputs);

        // Context file
        {
            use crate::store::LoadedContext;
            let mut ctx = LoadedContext::new();
            ctx.files.insert("brand".to_string(), json!("Brand Text"));
            store.set_context(ctx);
        }

        // Env
        std::env::set_var("NIKA_TEST_MIXED_8A", "env_val");

        let mut spec = WithSpec::default();
        spec.insert(
            "from_task".to_string(),
            WithEntry::simple(BindingPath::parse("$step1.result").unwrap()),
        );
        spec.insert(
            "from_input".to_string(),
            WithEntry::simple(BindingPath::parse("$inputs.topic").unwrap()),
        );
        spec.insert(
            "from_context".to_string(),
            WithEntry::simple(BindingPath::parse("$context.files.brand").unwrap()),
        );
        spec.insert(
            "from_env".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_TEST_MIXED_8A").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();

        assert_eq!(bindings.get("from_task"), Some(&json!("task_val")));
        assert_eq!(bindings.get("from_input"), Some(&json!("AI")));
        assert_eq!(bindings.get("from_context"), Some(&json!("Brand Text")));
        assert_eq!(bindings.get("from_env"), Some(&json!("env_val")));

        std::env::remove_var("NIKA_TEST_MIXED_8A");
    }
    #[test]
    fn env_binding_allows_secret_pattern_vars() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Set env vars with secret patterns (KEY, TOKEN, AUTH)
        std::env::set_var("NIKA_TEST_ELEVENLABS_API_KEY", "sk-test-123");
        std::env::set_var("NIKA_TEST_MY_SECRET_TOKEN", "tok-456");
        std::env::set_var("NIKA_TEST_CUSTOM_AUTH", "auth-789");

        let mut spec = WithSpec::default();
        spec.insert(
            "api_key".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_TEST_ELEVENLABS_API_KEY").unwrap()),
        );
        spec.insert(
            "token".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_TEST_MY_SECRET_TOKEN").unwrap()),
        );
        spec.insert(
            "auth".to_string(),
            WithEntry::simple(BindingPath::parse("$env.NIKA_TEST_CUSTOM_AUTH").unwrap()),
        );

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();

        // All secret-pattern vars should now be accessible (BUG-001 fix)
        assert_eq!(bindings.get("api_key"), Some(&json!("sk-test-123")));
        assert_eq!(bindings.get("token"), Some(&json!("tok-456")));
        assert_eq!(bindings.get("auth"), Some(&json!("auth-789")));

        std::env::remove_var("NIKA_TEST_ELEVENLABS_API_KEY");
        std::env::remove_var("NIKA_TEST_MY_SECRET_TOKEN");
        std::env::remove_var("NIKA_TEST_CUSTOM_AUTH");
    }
    #[test]
    fn env_binding_blocks_restricted_vars() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Set restricted env vars to known values
        std::env::set_var("SSH_AUTH_SOCK", "/tmp/test-agent.sock");
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "super-secret");

        let mut spec = WithSpec::default();
        // SSH_AUTH_SOCK — process/system internal
        let mut entry_ssh = WithEntry::simple(BindingPath::parse("$env.SSH_AUTH_SOCK").unwrap());
        entry_ssh.transform = Some(TransformExpr::parse("default(\"blocked\")").unwrap());
        spec.insert("ssh".to_string(), entry_ssh);
        // NIKA_VAULT_PASSPHRASE — nika internal
        let mut entry_vault =
            WithEntry::simple(BindingPath::parse("$env.NIKA_VAULT_PASSPHRASE").unwrap());
        entry_vault.transform = Some(TransformExpr::parse("default(\"blocked\")").unwrap());
        spec.insert("vault".to_string(), entry_vault);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();

        // Both should resolve to the default, not the actual value
        assert_eq!(bindings.get("ssh"), Some(&json!("blocked")));
        assert_eq!(bindings.get("vault"), Some(&json!("blocked")));

        std::env::remove_var("SSH_AUTH_SOCK");
        std::env::remove_var("NIKA_VAULT_PASSPHRASE");
    }
    #[test]
    fn env_binding_blocks_case_sensitive() {
        // Blocklist uses uppercase comparison
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        std::env::set_var("LD_PRELOAD", "/tmp/evil.so");

        let mut spec = WithSpec::default();
        let mut entry = WithEntry::simple(BindingPath::parse("$env.LD_PRELOAD").unwrap());
        entry.transform = Some(TransformExpr::parse("default(\"safe\")").unwrap());
        spec.insert("ld".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("ld"), Some(&json!("safe")));

        std::env::remove_var("LD_PRELOAD");
    }
    /// Helper: populate a RunContext with a "gen" task that has media refs
    /// and a "thumb" task that has thumbnail JSON output.
    fn store_with_media_chain() -> RunContext {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Task "gen": produces an image with media refs in the side-channel
        let gen_media = vec![crate::media::MediaRef {
            hash: "blake3:abc123def456".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 1048576,
            path: std::path::PathBuf::from("/tmp/cas/ab/c123def456"),
            extension: "png".to_string(),
            created_by: "gen".to_string(),
            metadata: {
                let mut m = serde_json::Map::new();
                m.insert("width".to_string(), json!(1024));
                m.insert("height".to_string(), json!(768));
                m
            },
        }];
        store.insert(
            Arc::from("gen"),
            TaskResult::success(json!({"prompt": "a sunset photo"}), Duration::from_secs(3))
                .with_media(gen_media),
        );

        // Task "thumb": invoke result stored as JSON-string output
        store.insert(
            Arc::from("thumb"),
            TaskResult::success_str(
                r#"{"hash":"blake3:thumb_999","mime_type":"image/png","size_bytes":2048,"metadata":{"width":256,"height":192}}"#,
                Duration::from_millis(100),
            ),
        );

        store
    }
    #[test]
    fn binding_spec_resolves_invoke_output_hash() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // with: { thumb_hash: $thumb.hash }
        spec.insert("thumb_hash".to_string(), BindingEntry::new("thumb.hash"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("thumb_hash", &store).unwrap();
        assert_eq!(value, json!("blake3:thumb_999"));
    }
    #[test]
    fn binding_spec_resolves_invoke_output_nested_metadata() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // with: { thumb_width: $thumb.metadata.width }
        spec.insert(
            "thumb_width".to_string(),
            BindingEntry::new("thumb.metadata.width"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("thumb_width", &store).unwrap();
        assert_eq!(value, json!(256));
    }
    #[test]
    fn binding_spec_resolves_invoke_output_mime_type() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        spec.insert(
            "thumb_mime".to_string(),
            BindingEntry::new("thumb.mime_type"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("thumb_mime", &store).unwrap();
        assert_eq!(value, json!("image/png"));
    }
    #[test]
    fn binding_spec_resolves_media_ref_hash() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // with: { gen_hash: $gen.media[0].hash }
        spec.insert(
            "gen_hash".to_string(),
            BindingEntry::new("gen.media[0].hash"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("gen_hash", &store).unwrap();
        assert_eq!(value, json!("blake3:abc123def456"));
    }
    #[test]
    fn binding_spec_resolves_media_ref_enriched_width() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // with: { gen_width: $gen.media[0].metadata.width }
        spec.insert(
            "gen_width".to_string(),
            BindingEntry::new("gen.media[0].metadata.width"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("gen_width", &store).unwrap();
        assert_eq!(value, json!(1024));
    }
    #[test]
    fn binding_spec_resolves_media_ref_mime_type() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        spec.insert(
            "gen_mime".to_string(),
            BindingEntry::new("gen.media[0].mime_type"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("gen_mime", &store).unwrap();
        assert_eq!(value, json!("image/png"));
    }
    #[test]
    fn binding_spec_resolves_media_full_array() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // with: { all_media: $gen.media }
        spec.insert("all_media".to_string(), BindingEntry::new("gen.media"));

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();
        let value = bindings.get_resolved("all_media", &store).unwrap();
        let arr = value.as_array().expect("media should be an array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["hash"], "blake3:abc123def456");
    }
    #[test]
    fn binding_spec_chained_gen_media_and_thumb_output() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();

        // Bind both gen.media[0].hash and thumb.hash in one spec
        spec.insert(
            "source_hash".to_string(),
            BindingEntry::new("gen.media[0].hash"),
        );
        spec.insert("thumb_hash".to_string(), BindingEntry::new("thumb.hash"));
        spec.insert(
            "thumb_width".to_string(),
            BindingEntry::new("thumb.metadata.width"),
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        // Verify all three bindings resolve correctly
        assert_eq!(
            bindings.get_resolved("source_hash", &store).unwrap(),
            json!("blake3:abc123def456"),
            "gen.media[0].hash should resolve via media side-channel"
        );
        assert_eq!(
            bindings.get_resolved("thumb_hash", &store).unwrap(),
            json!("blake3:thumb_999"),
            "thumb.hash should resolve via JSON-string auto-parse"
        );
        assert_eq!(
            bindings.get_resolved("thumb_width", &store).unwrap(),
            json!(256),
            "thumb.metadata.width should resolve via nested JSON-string traversal"
        );
    }
    #[test]
    fn binding_spec_lazy_media_ref_resolves_on_access() {
        use crate::binding::BindingEntry;

        let store = store_with_media_chain();
        let mut spec = BindingSpec::default();
        // Lazy binding: resolution deferred until get_resolved
        spec.insert(
            "lazy_hash".to_string(),
            BindingEntry {
                path: "gen.media[0].hash".to_string(),
                default: None,
                lazy: true,
            },
        );

        let bindings = ResolvedBindings::from_binding_spec(Some(&spec), &store).unwrap();

        // Should be pending initially
        assert!(bindings.is_lazy("lazy_hash"));

        // But still resolves correctly via get_resolved
        let value = bindings.get_resolved("lazy_hash", &store).unwrap();
        assert_eq!(value, json!("blake3:abc123def456"));
    }
    #[test]
    #[serial]
    fn vault_binding_resolves_value() {
        // Set up a vault with credentials in a temp dir
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let dir = tempfile::TempDir::new().unwrap();
        let vault = nika_vault::NikaVault::new(dir.path());

        let mut fields = std::collections::BTreeMap::new();
        fields.insert("api_key".to_string(), "sk_live_test123".to_string());
        fields.insert("secret".to_string(), "whsec_test456".to_string());
        vault.set_credential("stripe", fields, None, None).unwrap();

        // Attach vault to RunContext
        let mut store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.set_vault(Arc::new(vault));

        // Build WithSpec with $vault.stripe.secret
        let path = BindingPath::parse("$vault.stripe.secret").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("stripe_secret".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("stripe_secret"), Some(&json!("whsec_test456")));
    }
    #[test]
    #[serial]
    fn vault_binding_not_found_errors() {
        // Vault has no "nonexistent" service
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let dir = tempfile::TempDir::new().unwrap();
        let vault = nika_vault::NikaVault::new(dir.path());

        let mut store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.set_vault(Arc::new(vault));

        let path = BindingPath::parse("$vault.nonexistent.key").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("missing".to_string(), entry);

        // Should error because value is None with no default
        let result = ResolvedBindings::from_with_spec(Some(&spec), &store);
        assert!(result.is_err(), "Should error on missing vault credential");
    }
    #[test]
    #[serial]
    fn vault_binding_with_default() {
        // When vault credential is missing, the ?? default should apply
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let dir = tempfile::TempDir::new().unwrap();
        let vault = nika_vault::NikaVault::new(dir.path());

        let mut store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.set_vault(Arc::new(vault));

        let path = BindingPath::parse("$vault.missing_service.api_key").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: Some(json!("fallback-key")),
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("key".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("key"), Some(&json!("fallback-key")));
    }
    #[test]
    #[serial]
    fn vault_binding_is_redacted_in_traces() {
        // Vault-sourced bindings must be redacted in to_value_redacted()
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let dir = tempfile::TempDir::new().unwrap();
        let vault = nika_vault::NikaVault::new(dir.path());
        vault.set("anthropic", "sk-ant-secret").unwrap();

        let mut store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.set_vault(Arc::new(vault));

        let path = BindingPath::parse("$vault.anthropic.key").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("api_key".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();

        // The redacted version should not contain the secret
        let redacted = bindings.to_value_redacted();
        let redacted_map = redacted.as_object().unwrap();
        let val = redacted_map.get("api_key").unwrap();
        assert_eq!(
            val,
            &json!("[REDACTED:$env]"),
            "Vault-sourced binding should be redacted"
        );
    }
    #[test]
    #[serial]
    fn vault_binding_simple_key_via_key_field() {
        // A simple Key("sk-ant-test") is accessible as $vault.anthropic.key
        std::env::set_var("NIKA_VAULT_PASSPHRASE", "test-only");
        let dir = tempfile::TempDir::new().unwrap();
        let vault = nika_vault::NikaVault::new(dir.path());
        vault.set("anthropic", "sk-ant-test-12345").unwrap();

        let mut store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.set_vault(Arc::new(vault));

        let path = BindingPath::parse("$vault.anthropic.key").unwrap();
        let entry = WithEntry {
            source: path,
            binding_type: BindingType::Any,
            default: None,
            lazy: false,
            transform: None,
        };
        let mut spec = WithSpec::default();
        spec.insert("api_key".to_string(), entry);

        let bindings = ResolvedBindings::from_with_spec(Some(&spec), &store).unwrap();
        assert_eq!(bindings.get("api_key"), Some(&json!("sk-ant-test-12345")));
    }
}

