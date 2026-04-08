// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Sprint 2 Item 3b — `check_path_readable` tests.
//!
//! Verifies the recon-block helper that gates `nika:read`, `nika:glob`,
//! `nika:grep` against sensitive files (.mcp.json, nika.toml, .env*,
//! *.nika.yaml) for tainted agents.

#![cfg(test)]

use std::path::PathBuf;

use nika_core::trust::TrustLevel;

use super::check_path_readable;
use crate::error::NikaError;

#[test]
fn check_path_readable_allows_trusted_caller_for_sensitive_path() {
    let path = PathBuf::from("nika.toml");
    let result = check_path_readable(&path, TrustLevel::Trusted, false);
    assert!(
        result.is_ok(),
        "trusted caller must always be allowed: {result:?}"
    );
}

#[test]
fn check_path_readable_allows_elevated_caller_for_sensitive_path() {
    let path = PathBuf::from("nika.toml");
    let result = check_path_readable(&path, TrustLevel::Untrusted, true);
    assert!(
        result.is_ok(),
        "trust: elevated must bypass the recon block: {result:?}"
    );
}

#[test]
fn check_path_readable_blocks_untrusted_from_reading_nika_toml() {
    let path = PathBuf::from("nika.toml");
    let result = check_path_readable(&path, TrustLevel::Untrusted, false);
    assert!(matches!(result, Err(NikaError::CapabilityDenied { .. })));
    if let Err(NikaError::CapabilityDenied { action, reason, .. }) = result {
        assert_eq!(action, "nika:read");
        assert!(reason.contains("nika.toml"));
    }
}

#[test]
fn check_path_readable_blocks_untrusted_from_reading_mcp_json() {
    let path = PathBuf::from(".mcp.json");
    let result = check_path_readable(&path, TrustLevel::Untrusted, false);
    assert!(matches!(result, Err(NikaError::CapabilityDenied { .. })));
}

#[test]
fn check_path_readable_blocks_untrusted_from_reading_dot_env() {
    for name in [".env", ".env.local", ".env.production", ".env.staging"] {
        let path = PathBuf::from(name);
        let result = check_path_readable(&path, TrustLevel::Untrusted, false);
        assert!(
            matches!(result, Err(NikaError::CapabilityDenied { .. })),
            "{name} must be blocked"
        );
    }
}

#[test]
fn check_path_readable_blocks_untrusted_from_reading_workflow_files() {
    let path = PathBuf::from("research.nika.yaml");
    let result = check_path_readable(&path, TrustLevel::Untrusted, false);
    assert!(matches!(result, Err(NikaError::CapabilityDenied { .. })));
}

#[test]
fn check_path_readable_allows_untrusted_for_arbitrary_safe_files() {
    for name in ["README.md", "src/main.rs", "Cargo.toml", "data.json"] {
        let path = PathBuf::from(name);
        let result = check_path_readable(&path, TrustLevel::Untrusted, false);
        assert!(
            result.is_ok(),
            "{name} must be allowed for tainted reads: {result:?}"
        );
    }
}

#[test]
fn check_path_readable_handles_model_tainted_same_as_untrusted() {
    let path = PathBuf::from("nika.toml");
    let result = check_path_readable(&path, TrustLevel::ModelTainted, false);
    assert!(
        matches!(result, Err(NikaError::CapabilityDenied { .. })),
        "ModelTainted must also block sensitive reads"
    );
}

#[test]
fn check_path_readable_resolves_symlinks_when_present() {
    use std::fs;
    use tempfile::TempDir;

    let dir = TempDir::new().expect("tempdir");
    let real = dir.path().join("nika.toml");
    fs::write(&real, "[project]\nname = \"test\"").expect("write real file");

    let bait = dir.path().join("innocent.txt");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real, &bait).expect("symlink");
    }
    #[cfg(not(unix))]
    {
        // No-op on non-unix; just write a file with the same content
        // — the symlink test only runs on unix.
        fs::copy(&real, &bait).expect("copy");
    }

    let result = check_path_readable(&bait, TrustLevel::Untrusted, false);
    #[cfg(unix)]
    assert!(
        matches!(result, Err(NikaError::CapabilityDenied { .. })),
        "symlink innocent.txt → nika.toml must be blocked: {result:?}"
    );
    #[cfg(not(unix))]
    let _ = result; // Skip on non-unix
}
