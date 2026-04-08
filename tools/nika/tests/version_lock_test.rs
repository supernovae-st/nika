// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Version Lock Tests
//!
//! These tests ensure Nika NEVER reaches version 1.0.0.
//! This is a deliberate design decision, not a bug.
//! Enforced by: tests/version_lock_test.rs + ci.yml (check job).

use std::fs;
use std::path::Path;

/// Find Cargo.toml - handles both workspace root and direct paths
fn find_cargo_toml() -> String {
    let possible_paths = [
        "Cargo.toml",
        "../Cargo.toml",
        "../../Cargo.toml",
        "tools/nika/Cargo.toml",
    ];

    for path in possible_paths {
        if Path::new(path).exists() {
            // Read and check if it has a version field (not workspace)
            if let Ok(content) = fs::read_to_string(path) {
                if content.contains("version = \"") && !content.contains("version.workspace") {
                    return path.to_string();
                }
            }
        }
    }

    // Default fallback
    "Cargo.toml".to_string()
}

fn extract_version(content: &str) -> Option<String> {
    content
        .lines()
        .find(|line| line.starts_with("version = "))
        .map(|line| {
            line.trim_start_matches("version = ")
                .trim_matches('"')
                .to_string()
        })
}

#[test]
fn version_must_be_zero_major() {
    let cargo_path = find_cargo_toml();
    let content =
        fs::read_to_string(&cargo_path).unwrap_or_else(|_| panic!("Failed to read {}", cargo_path));

    let version =
        extract_version(&content).unwrap_or_else(|| panic!("No version found in {}", cargo_path));

    assert!(
        version.starts_with("0."),
        "\n\
        ╔═══════════════════════════════════════════════════════════════════════════════╗\n\
        ║  🏴‍☠️ CAPTAIN'S ORDERS VIOLATION!                                               ║\n\
        ╠═══════════════════════════════════════════════════════════════════════════════╣\n\
        ║                                                                               ║\n\
        ║  The Captain has decreed: Nika shall NEVER be version 1.0.0 or higher.       ║\n\
        ║  This is the way of the cosmic pirates.                                      ║\n\
        ║                                                                               ║\n\
        ║  Found: {}                                                                     \n\
        ║  Expected: 0.x.x (forever)                                                   ║\n\
        ║                                                                               ║\n\
        ╚═══════════════════════════════════════════════════════════════════════════════╝\n",
        version
    );
}

#[test]
fn version_follows_semver() {
    let cargo_path = find_cargo_toml();
    let content =
        fs::read_to_string(&cargo_path).unwrap_or_else(|_| panic!("Failed to read {}", cargo_path));

    let version =
        extract_version(&content).unwrap_or_else(|| panic!("No version found in {}", cargo_path));

    let parts: Vec<&str> = version.split('.').collect();

    assert_eq!(
        parts.len(),
        3,
        "Version must be MAJOR.MINOR.PATCH format, got: {}",
        version
    );

    for (i, part) in parts.iter().enumerate() {
        let part_name = match i {
            0 => "MAJOR",
            1 => "MINOR",
            2 => "PATCH",
            _ => "UNKNOWN",
        };
        part.parse::<u32>().unwrap_or_else(|_| {
            panic!(
                "Version {} part '{}' must be a number in: {}",
                part_name, part, version
            )
        });
    }
}

#[test]
fn version_not_zero_zero_zero() {
    let cargo_path = find_cargo_toml();
    let content =
        fs::read_to_string(&cargo_path).unwrap_or_else(|_| panic!("Failed to read {}", cargo_path));

    let version =
        extract_version(&content).unwrap_or_else(|| panic!("No version found in {}", cargo_path));

    assert_ne!(
        version, "0.0.0",
        "Version 0.0.0 is not allowed (must be at least 0.0.1)"
    );
}

#[test]
fn version_minor_within_bounds() {
    let cargo_path = find_cargo_toml();
    let content =
        fs::read_to_string(&cargo_path).unwrap_or_else(|_| panic!("Failed to read {}", cargo_path));

    let version =
        extract_version(&content).unwrap_or_else(|| panic!("No version found in {}", cargo_path));

    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        let minor: u32 = parts[1].parse().unwrap_or(0);
        assert!(
            minor < 100,
            "Minor version {} seems unusually high. Are we sure about this?",
            minor
        );
    }
}
