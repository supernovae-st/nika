// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::process::Command;

fn main() {
    // Git short hash
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=NIKA_GIT_HASH={hash}");

    // Build timestamp (Unix seconds)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    println!("cargo:rustc-env=NIKA_BUILD_TIMESTAMP={now}");

    // Channel: set by CI (NIKA_BUILD_CHANNEL=release) or default to "dev"
    let channel = std::env::var("NIKA_BUILD_CHANNEL").unwrap_or_else(|_| "dev".into());
    println!("cargo:rustc-env=NIKA_BUILD_CHANNEL={channel}");

    // Rerun triggers
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads/");
    println!("cargo:rerun-if-env-changed=NIKA_BUILD_CHANNEL");
}
