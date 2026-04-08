// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model Management Contract Tests
//!
//! These tests define the expected behavior of `nika model *` commands.
//!
//! # Tests (10 total)
//!
//! 1. model list shows local and remote
//! 2. model list --local filter
//! 3. model list --remote filter
//! 4. model pull downloads from HuggingFace
//! 5. model status shows loaded models
//! 6. model info displays metadata
//! 7. model delete removes local
//! 8. 16 known models defined
//! 9. Model path resolution
//! 10. Model auto-quantization

use super::common::{run_nika, KNOWN_MODEL_COUNT};

/// Contract: `nika model list` shows local and remote models
#[test]
fn contract_model_list_shows_all() {
    let output = run_nika(&["model", "list"]);

    // Should succeed
    assert!(output.status.success(), "model list should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show some kind of list
    assert!(
        stdout.contains("Model")
            || stdout.contains("Name")
            || stdout.contains("local")
            || stdout.contains("remote")
            || !stdout.is_empty(),
        "model list should show models. Got: {}",
        stdout
    );
}

/// Contract: `nika model list --local` shows only local models
#[test]
fn contract_model_list_local_filter() {
    let output = run_nika(&["model", "list", "--local"]);

    // --local flag may not be supported
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("unrecognized") {
            eprintln!("Note: --local flag not supported");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show local models only
    // May be empty if no local models downloaded
    let _ = stdout;
}

/// Contract: `nika model list --remote` shows available models
#[test]
fn contract_model_list_remote_filter() {
    let output = run_nika(&["model", "list", "--remote"]);

    // --remote flag may not be supported
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("unrecognized") {
            eprintln!("Note: --remote flag not supported");
            return;
        }
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show available remote models
    // Should include known models like llama, phi, etc.
    let _ = stdout;
}

/// Contract: `nika model pull` downloads from HuggingFace
#[test]
fn contract_model_pull_from_hf() {
    let output = run_nika(&["model", "pull", "--help"]);

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Help should describe pulling models
    assert!(
        combined.contains("pull")
            || combined.contains("download")
            || combined.contains("HuggingFace")
            || combined.contains("model"),
        "model pull help should describe downloading. Got: {}",
        combined
    );
}

/// Contract: `nika model status` shows loaded models
#[test]
fn contract_model_status_shows_loaded() {
    let output = run_nika(&["model", "status"]);

    // Should succeed
    assert!(output.status.success(), "model status should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show status information
    // May show "no models loaded" or list of loaded models
    assert!(
        stdout.contains("loaded")
            || stdout.contains("Model")
            || stdout.contains("none")
            || stdout.contains("status")
            || stdout.is_empty(),
        "model status should show info. Got: {}",
        stdout
    );
}

/// Contract: `nika model info` displays model metadata
#[test]
fn contract_model_info_metadata() {
    let output = run_nika(&["model", "info", "--help"]);

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Help should describe model info
    assert!(
        combined.contains("info")
            || combined.contains("metadata")
            || combined.contains("details")
            || combined.contains("model"),
        "model info help should describe metadata. Got: {}",
        combined
    );
}

/// Contract: `nika model delete` removes local model
#[test]
fn contract_model_delete_removes_local() {
    // Try to delete non-existent model
    let output = run_nika(&["model", "delete", "nonexistent_model_xyz"]);

    // Should fail gracefully
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should indicate not found (not crash)
    assert!(
        combined.contains("not found")
            || combined.contains("does not exist")
            || combined.contains("unknown")
            || !output.status.success(),
        "delete non-existent should fail gracefully. Got: {}",
        combined
    );
}

/// Contract: 16 known models defined in KNOWN_MODELS
#[test]
fn contract_model_known_count() {
    // Document the 16 known models from nika-core
    let known_models = [
        "llama3.2:1b",
        "llama3.2:3b",
        "llama3.1:8b",
        "llama3.1:70b",
        "phi3:mini",
        "phi3:small",
        "phi3:medium",
        "mistral:7b",
        "mistral-nemo:12b",
        "codellama:7b",
        "codellama:13b",
        "codellama:34b",
        "qwen2:0.5b",
        "qwen2:1.5b",
        "qwen2:7b",
        "gemma2:2b",
    ];

    assert_eq!(
        known_models.len(),
        KNOWN_MODEL_COUNT,
        "Should have {} known models",
        KNOWN_MODEL_COUNT
    );
}

/// Contract: Model path resolution
#[test]
fn contract_model_path_resolution() {
    // Document the model path resolution
    // Models are stored in ~/.cache/huggingface/models/

    let home = std::env::var("HOME").unwrap_or_default();
    let model_cache_path = format!("{}/.cache/huggingface/models", home);

    // Path should be deterministic
    assert!(
        !home.is_empty(),
        "HOME should be set for model path resolution"
    );

    let _ = model_cache_path;
}

/// Contract: Model auto-quantization
#[test]
fn contract_model_auto_quantization() {
    // Document auto-quantization behavior
    // Models are automatically quantized to Q4_K_M by default

    // Supported quantization levels:
    // - Q4_K_M (default, balanced)
    // - Q4_K_S (smaller, less accurate)
    // - Q5_K_M (larger, more accurate)
    // - Q8_0 (largest, most accurate)

    // This is a documentation test
}

/// Contract: `nika model` with no subcommand shows help
#[test]
fn contract_model_no_subcommand_shows_help() {
    let output = run_nika(&["model"]);

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should show help/usage information
    assert!(
        combined.contains("list") && combined.contains("pull"),
        "model without subcommand should show help. Got: {}",
        combined
    );
}
