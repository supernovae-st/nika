// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! System utilities for platform-specific information.
//!
//! Provides cross-platform functions for querying system resources
//! like RAM, without requiring external dependencies.
//!
//! # Example
//!
//! ```rust,ignore
//! use nika::util::system::{get_total_ram_gb, get_available_ram_gb, has_enough_ram_gb};
//!
//! let total = get_total_ram_gb();
//! println!("Total RAM: {:.1} GB", total);
//!
//! // Check if system has enough RAM for a 4GB model
//! if has_enough_ram_gb(4.0) {
//!     println!("Sufficient RAM available");
//! }
//! ```

/// Default RAM value (in GB) when detection fails.
const DEFAULT_RAM_GB: f64 = 8.0;

/// Headroom factor for "available" RAM calculations (80% of total).
const RAM_HEADROOM_FACTOR: f64 = 0.8;

/// Get total system RAM in gigabytes.
///
/// Uses platform-specific methods:
/// - macOS: `sysctl -n hw.memsize`
/// - Linux: `/proc/meminfo`
///
/// Returns 8.0 GB as a fallback on unknown platforms or detection failure.
///
/// # Example
///
/// ```rust,ignore
/// use nika::util::system::get_total_ram_gb;
///
/// let ram = get_total_ram_gb();
/// assert!(ram > 0.0);
/// ```
#[must_use]
pub fn get_total_ram_gb() -> f64 {
    get_total_ram_bytes()
        .map(|bytes| bytes as f64 / 1_000_000_000.0)
        .unwrap_or(DEFAULT_RAM_GB)
}

/// Get total system RAM in bytes.
///
/// Returns `None` if detection fails.
#[must_use]
pub fn get_total_ram_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
    }

    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|line| line.starts_with("MemTotal:"))
                    .and_then(|line| {
                        line.split_whitespace()
                            .nth(1)
                            .and_then(|kb| kb.parse::<u64>().ok())
                    })
            })
            // /proc/meminfo reports in kB, convert to bytes
            .map(|kb| kb * 1024)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Get available system RAM in gigabytes with headroom.
///
/// Returns 80% of total RAM to leave headroom for the OS and other processes.
/// This is useful for estimating how much RAM is safe to use for models.
///
/// Returns the value as u32 for convenience in model selection.
///
/// # Example
///
/// ```rust,ignore
/// use nika::util::system::get_available_ram_gb;
///
/// let available = get_available_ram_gb();
/// println!("Available RAM (with headroom): {} GB", available);
/// ```
#[must_use]
pub fn get_available_ram_gb() -> u32 {
    (get_total_ram_gb() * RAM_HEADROOM_FACTOR) as u32
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_total_ram_gb() {
        let ram = get_total_ram_gb();
        // Should return something reasonable (> 1GB on any modern system)
        assert!(ram > 1.0, "RAM should be > 1 GB, got {}", ram);
        // Should not exceed reasonable limits (< 1TB)
        assert!(ram < 1024.0, "RAM should be < 1 TB, got {}", ram);
    }

    #[test]
    fn test_get_total_ram_bytes() {
        #[allow(unused_variables)]
        let bytes = get_total_ram_bytes();
        // On macOS/Linux, this should succeed
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            assert!(bytes.is_some(), "RAM detection should succeed");
            let bytes = bytes.unwrap();
            assert!(bytes > 1_000_000_000, "RAM should be > 1 GB in bytes");
        }
    }

    #[test]
    fn test_get_available_ram_gb() {
        let available = get_available_ram_gb();
        // Should be less than total (80% factor applied)
        let total = get_total_ram_gb() as u32;
        assert!(
            available <= total,
            "Available {} should be <= total {}",
            available,
            total
        );
        // Should be at least 1 GB on any modern system
        assert!(available >= 1, "Available RAM should be >= 1 GB");
    }

    #[test]
    fn test_default_ram_constant() {
        // Verify the default is reasonable
        assert_eq!(DEFAULT_RAM_GB, 8.0);
    }

    #[test]
    fn test_headroom_factor() {
        // Verify headroom is 80%
        assert!((RAM_HEADROOM_FACTOR - 0.8).abs() < f64::EPSILON);
    }
}
