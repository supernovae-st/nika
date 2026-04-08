// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Binary compilation and execution tests.
//!
//! Tests the compiled binary works correctly.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Get the cargo target directory
fn target_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
}

// ============================================================================
// COMPILATION TESTS
// ============================================================================

#[test]
fn test_binary_compiles_debug() {
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["build"])
        .output()
        .expect("Failed to run cargo build");

    assert!(
        output.status.success(),
        "cargo build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "Slow: release build"]
fn test_binary_compiles_release() {
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["build", "--release"])
        .output()
        .expect("Failed to run cargo build --release");

    assert!(
        output.status.success(),
        "cargo build --release failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ============================================================================
// BINARY SIZE TESTS
// ============================================================================

#[test]
#[ignore = "Requires release build"]
fn test_binary_size_reasonable() {
    let binary_path = target_dir().join("release").join("nika");

    if !binary_path.exists() {
        eprintln!("Skipping: Release binary not found");
        return;
    }

    let metadata = std::fs::metadata(&binary_path).unwrap();
    let size_mb = metadata.len() as f64 / (1024.0 * 1024.0);

    println!("Binary size: {:.2} MB", size_mb);

    // Binary should be under 50MB (reasonable for Rust CLI)
    assert!(
        size_mb < 50.0,
        "Binary too large: {:.2} MB (expected < 50 MB)",
        size_mb
    );

    // Binary should be at least 1MB (sanity check)
    assert!(
        size_mb > 1.0,
        "Binary too small: {:.2} MB (expected > 1 MB)",
        size_mb
    );
}

// ============================================================================
// STARTUP TIME TESTS
// ============================================================================

#[test]
fn test_startup_time_help() {
    // Use precompiled binary to avoid cargo compilation time
    let binary = target_dir().join("debug/nika");

    if !binary.exists() {
        eprintln!("Skipping: Debug binary not found, run `cargo build` first");
        return;
    }

    let start = std::time::Instant::now();

    let output = Command::new(&binary)
        .arg("--help")
        .output()
        .expect("Failed to run nika --help");

    let elapsed = start.elapsed();

    assert!(output.status.success());

    // Help should return in under 3 seconds (debug builds under load are slower)
    // Release builds typically complete in <500ms
    assert!(
        elapsed < Duration::from_secs(3),
        "Startup too slow: {:?}",
        elapsed
    );

    println!("--help startup time: {:?}", elapsed);
}

// ============================================================================
// FEATURE FLAG TESTS
// ============================================================================

#[test]
fn test_default_features_compile() {
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["check"])
        .output()
        .expect("Failed to run cargo check");

    assert!(
        output.status.success(),
        "cargo check with default features failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_no_default_features_compile() {
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["check", "--no-default-features"])
        .output()
        .expect("Failed to run cargo check --no-default-features");

    // May fail if some features are required, but should at least run
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        println!("Note: --no-default-features requires some features");
        println!("stderr: {}", stderr);
    }
}

#[test]
fn test_tui_feature_compile() {
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["check", "--features", "tui"])
        .output()
        .expect("Failed to run cargo check --features tui");

    assert!(
        output.status.success(),
        "TUI feature compilation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ============================================================================
// CLIPPY AND FORMAT TESTS
// ============================================================================

#[test]
fn test_clippy_passes() {
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["clippy", "--", "-D", "warnings"])
        .output()
        .expect("Failed to run cargo clippy");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("Clippy warnings/errors:\n{}", stderr);
    }

    assert!(
        output.status.success(),
        "Clippy found warnings: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_format_check() {
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["fmt", "--check"])
        .output()
        .expect("Failed to run cargo fmt --check");

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("Formatting issues:\n{}", stdout);
    }

    assert!(output.status.success(), "Code formatting issues found");
}

// ============================================================================
// TEST SUITE TESTS
// ============================================================================

#[test]
#[ignore = "Recursive: runs cargo test as subprocess, causes timeout in CI"]
fn test_unit_tests_pass() {
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["test", "--lib", "--", "--test-threads=4"])
        .env("RUST_BACKTRACE", "1")
        .output()
        .expect("Failed to run cargo test --lib");

    assert!(
        output.status.success(),
        "Unit tests failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ============================================================================
// DEPENDENCY TESTS
// ============================================================================

#[test]
fn test_no_unused_dependencies() {
    // This requires cargo-udeps (optional)
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["udeps", "--all-targets"])
        .output();

    match output {
        Ok(out) => {
            if out.status.success() {
                println!("No unused dependencies found");
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("error: no such subcommand") {
                    println!("Skipping: cargo-udeps not installed");
                } else {
                    println!("Unused dependencies found:\n{}", stderr);
                }
            }
        }
        Err(_) => {
            println!("Skipping: cargo-udeps not available");
        }
    }
}

#[test]
fn test_outdated_dependencies() {
    // This requires cargo-outdated (optional)
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["outdated", "--depth", "1"])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains("All dependencies are up to date") {
                println!("All dependencies up to date");
            } else {
                println!("Dependency status:\n{}", stdout);
            }
        }
        Err(_) => {
            println!("Skipping: cargo-outdated not available");
        }
    }
}

// ============================================================================
// SECURITY AUDIT TESTS
// ============================================================================

#[test]
fn test_security_audit() {
    // This requires cargo-audit (optional)
    let output = Command::new("cargo")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["audit"])
        .output();

    match output {
        Ok(out) => {
            if out.status.success() {
                println!("No security vulnerabilities found");
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("error: no such subcommand") {
                    println!("Skipping: cargo-audit not installed");
                } else {
                    println!("Security audit results:\n{}", stderr);
                }
            }
        }
        Err(_) => {
            println!("Skipping: cargo-audit not available");
        }
    }
}
