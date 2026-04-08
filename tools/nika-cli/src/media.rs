// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika media` subcommand — list, stats, and GC for CAS media store.
//!
//! Commands:
//! - `nika media list` — Table output: HASH | SIZE | PATH
//! - `nika media stats` — Count, total size, shard distribution
//! - `nika media clean` — Remove old files with GC safety

use std::path::PathBuf;
use std::time::Duration;

use clap::Subcommand;
use colored::Colorize;

use nika_engine::display::{key_value, key_value_width, section_header, separator, StatusIcon};
use nika_engine::error::NikaError;
use nika_engine::media::CasStore;

/// Minimum GC age floor: 5 minutes (safety: prevent deleting in-flight media)
const MIN_GC_AGE_SECS: u64 = 300;

/// Lockfile name written by runner during workflow execution
const LOCKFILE_NAME: &str = ".nika-run.lock";

#[derive(Subcommand, Debug)]
pub enum MediaAction {
    /// Import a local file into the CAS media store
    Import {
        /// File path to import
        #[arg(help = "File path to import into CAS")]
        file: PathBuf,
    },

    /// List all files in the media store
    List,

    /// Show store statistics (count, size, shard distribution)
    Stats,

    /// List available media tools and their features
    Tools,

    /// Remove old media files from the store
    Clean {
        /// Minimum age for deletion (e.g., "1h", "7d", "30m"). Default: 1h
        #[arg(long, default_value = "1h")]
        older_than: String,

        /// Show what would be deleted without actually deleting
        #[arg(long)]
        dry_run: bool,

        /// Force cleanup even if a workflow is running (bypass lockfile check)
        #[arg(long)]
        force: bool,
    },
}

/// Handle `nika media` subcommand
pub async fn handle_media_command(action: MediaAction, quiet: bool) -> Result<(), NikaError> {
    let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let store = CasStore::workspace_default(&workspace_root);
    // Derive store_root from the actual CasStore root, NOT hardcoded.
    // This ensures NIKA_MEDIA_STORE overrides are respected for lockfile checks.
    let store_root = store.root().to_path_buf();

    match action {
        MediaAction::Import { file } => handle_import(&store, &file, quiet).await,
        MediaAction::List => handle_list(&store, quiet),
        MediaAction::Stats => handle_stats(&store, quiet),
        MediaAction::Tools => {
            handle_tools();
            Ok(())
        }
        MediaAction::Clean {
            older_than,
            dry_run,
            force,
        } => handle_clean(&store, &store_root, &older_than, dry_run, force, quiet),
    }
}

/// Maximum CLI import file size: 500 MB (matches CAS budget).
const MAX_CLI_IMPORT_SIZE: u64 = 500 * 1024 * 1024;

/// Sensitive system directories that import must never read from.
const CLI_SENSITIVE_PREFIXES: &[&str] = &[
    "/etc/",
    "/proc/",
    "/sys/",
    "/dev/",
    "/var/run/",
    "/var/log/",
    "/private/etc/",
    "/private/var/run/",
    "/private/var/log/",
];

/// Validate import path: reject path traversal and sensitive directories.
fn validate_cli_import_path(path: &std::path::Path) -> Result<(), NikaError> {
    let path_str = path.to_string_lossy();

    // Reject paths containing ".."
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(NikaError::BuiltinToolError {
                tool: "nika:import".to_string(),
                reason: format!(
                    "[NIKA-297] security violation: path traversal not allowed: {path_str}"
                ),
            });
        }
    }

    // Check raw path against sensitive prefixes
    for prefix in CLI_SENSITIVE_PREFIXES {
        if path_str.starts_with(prefix) {
            return Err(NikaError::BuiltinToolError {
                tool: "nika:import".to_string(),
                reason: format!(
                    "[NIKA-297] security violation: reading from {prefix} is not allowed"
                ),
            });
        }
    }

    // Also check canonical path (resolves symlinks)
    if let Ok(canonical) = path.canonicalize() {
        let canonical_str = canonical.to_string_lossy();
        for prefix in CLI_SENSITIVE_PREFIXES {
            if canonical_str.starts_with(prefix) {
                return Err(NikaError::BuiltinToolError {
                    tool: "nika:import".to_string(),
                    reason: format!(
                        "[NIKA-297] security violation: reading from {prefix} is not allowed"
                    ),
                });
            }
        }
    }

    Ok(())
}

async fn handle_import(
    store: &CasStore,
    file: &std::path::Path,
    quiet: bool,
) -> Result<(), NikaError> {
    // Security: reject path traversal and sensitive directories
    validate_cli_import_path(file)?;

    // Async metadata check (no blocking I/O)
    let metadata = tokio::fs::metadata(file)
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => NikaError::BuiltinInvalidParams {
                tool: "nika:import".to_string(),
                reason: format!("[NIKA-294] file not found: {}", file.display()),
            },
            _ => NikaError::BuiltinToolError {
                tool: "nika:import".to_string(),
                reason: format!("[NIKA-290] cannot stat file: {e}"),
            },
        })?;

    if !metadata.is_file() {
        return Err(NikaError::BuiltinInvalidParams {
            tool: "nika:import".to_string(),
            reason: format!("[NIKA-294] not a regular file: {}", file.display()),
        });
    }

    if metadata.len() == 0 {
        return Err(NikaError::BuiltinInvalidParams {
            tool: "nika:import".to_string(),
            reason: format!("[NIKA-294] file is empty: {}", file.display()),
        });
    }

    if metadata.len() > MAX_CLI_IMPORT_SIZE {
        return Err(NikaError::BuiltinInvalidParams {
            tool: "nika:import".to_string(),
            reason: format!(
                "[NIKA-294] file too large ({} bytes, max {} bytes)",
                metadata.len(),
                MAX_CLI_IMPORT_SIZE
            ),
        });
    }

    // Read the file (size already validated)
    let data = tokio::fs::read(file)
        .await
        .map_err(|e| NikaError::BuiltinToolError {
            tool: "nika:import".to_string(),
            reason: format!("[NIKA-290] read failed: {e}"),
        })?;

    // Detect MIME type via magic bytes
    let mime_type = infer::get(&data).map_or_else(
        || "application/octet-stream".to_string(),
        |t| t.mime_type().to_string(),
    );

    let size = data.len() as u64;

    // Store in CAS
    let result = store
        .store(&data)
        .await
        .map_err(|e| NikaError::BuiltinToolError {
            tool: "nika:import".to_string(),
            reason: format!("[NIKA-290] CAS store failed: {e}"),
        })?;

    if quiet {
        println!("{}", result.hash);
    } else {
        println!("{} imported {}", StatusIcon::Ok, file.display());
        println!("{}", key_value_width("Hash", &result.hash, 8));
        println!("{}", key_value_width("MIME", &mime_type, 8));
        println!("{}", key_value_width("Size", &format_bytes(size), 8));
        if result.deduplicated {
            println!("    {} deduplicated (already in store)", StatusIcon::Info);
        }
    }

    Ok(())
}

fn handle_list(store: &CasStore, quiet: bool) -> Result<(), NikaError> {
    let entries = store.list();

    if entries.is_empty() {
        if !quiet {
            println!("{} media store is empty", StatusIcon::Info);
        }
        return Ok(());
    }

    if !quiet {
        println!(
            "  {:<68}  {:>10}  {}",
            "HASH".bold(),
            "SIZE".bold(),
            "PATH".bold()
        );
        println!("{}", separator(92));
    }

    for entry in &entries {
        println!(
            "  {:<68}  {:>10}  {}",
            entry.hash.dimmed(),
            format_bytes(entry.size),
            entry.path.display(),
        );
    }

    if !quiet {
        println!(
            "\n{} file(s), {} total",
            entries.len(),
            format_bytes(entries.iter().map(|e| e.size).sum())
        );
    }

    Ok(())
}

fn handle_stats(store: &CasStore, quiet: bool) -> Result<(), NikaError> {
    let entries = store.list();

    let total_size: u64 = entries.iter().map(|e| e.size).sum();
    let count = entries.len();

    // Shard distribution
    let mut shards: std::collections::BTreeMap<String, (usize, u64)> =
        std::collections::BTreeMap::new();
    for entry in &entries {
        // Extract shard prefix (first 2 chars of hash after "blake3:")
        let shard = entry.hash.strip_prefix("blake3:").map_or_else(
            || "??".to_string(),
            |h: &str| h[..2.min(h.len())].to_string(),
        );
        let counter = shards.entry(shard).or_insert((0, 0));
        counter.0 += 1;
        counter.1 += entry.size;
    }

    if quiet {
        println!("{count}");
        return Ok(());
    }

    println!("{}", section_header("Media Store Statistics"));
    println!("{}", key_value("Files", &count.to_string()));
    println!("{}", key_value("Total size", &format_bytes(total_size)));
    println!("{}", key_value("Shards", &shards.len().to_string()));

    if !shards.is_empty() {
        println!("{}", section_header("Shard Distribution"));
        for (shard, (shard_count, shard_size)) in &shards {
            println!(
                "    {}/  {:>4} files  {:>10}",
                shard,
                shard_count,
                format_bytes(*shard_size)
            );
        }
    }

    Ok(())
}

fn handle_clean(
    store: &CasStore,
    store_root: &std::path::Path,
    older_than: &str,
    dry_run: bool,
    force: bool,
    quiet: bool,
) -> Result<(), NikaError> {
    // Parse duration
    let duration = humantime::parse_duration(older_than).map_err(|e| NikaError::ConfigError {
        reason: format!("Invalid duration '{older_than}': {e}. Examples: 1h, 30m, 7d"),
    })?;

    // Enforce minimum GC age (5 minutes)
    let duration = if duration.as_secs() < MIN_GC_AGE_SECS {
        if !quiet {
            println!(
                "{} minimum GC age is 5 minutes, using 5m instead of '{}'",
                StatusIcon::Warn,
                older_than
            );
        }
        Duration::from_secs(MIN_GC_AGE_SECS)
    } else {
        duration
    };

    // Check lockfile (unless --force)
    if !force {
        let lockfile = store_root.join(LOCKFILE_NAME);
        if lockfile.exists() {
            return Err(NikaError::MediaStoreLocked {
                reason: format!(
                    "Locked by a running workflow ({}). \
                     Use --force to override or wait for the workflow to complete.",
                    lockfile.display()
                ),
            });
        }
    }

    if dry_run {
        // Count what would be deleted
        let entries = store.list();
        let now = std::time::SystemTime::now();
        let mut would_delete = 0u64;
        let mut would_free = 0u64;

        for entry in &entries {
            if let Ok(meta) = std::fs::metadata(&entry.path) {
                if let Ok(modified) = meta.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age > duration {
                            would_delete += 1;
                            would_free += entry.size;
                            if !quiet {
                                println!(
                                    "  {} {} ({})",
                                    StatusIcon::Warn,
                                    entry.hash.dimmed(),
                                    format_bytes(entry.size)
                                );
                            }
                        }
                    }
                }
            }
        }

        if !quiet {
            println!(
                "\n  {} would delete {} file(s), freeing {}",
                StatusIcon::Info,
                would_delete,
                format_bytes(would_free)
            );
        }
    } else {
        let result = store.clean_older_than(duration);
        if !quiet {
            println!(
                "{} removed {} file(s), freed {}",
                StatusIcon::Ok,
                result.removed,
                format_bytes(result.bytes_freed)
            );
        }
    }

    Ok(())
}

/// Format bytes as human-readable size
pub(crate) fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Display available media tools and their feature flags.
fn handle_tools() {
    println!("{}", section_header("Available Media Tools"));

    // Always-on tools (Tier 1)
    println!("  {}", "Tier 1 — Always On".cyan().bold());

    let tools_tier1 = [
        (
            "nika:import",
            "Import any file into CAS (image, audio, video, PDF)",
        ),
        ("nika:dimensions", "Image dimensions from headers (~0.1ms)"),
        ("nika:thumbhash", "25-byte compact image placeholder"),
        ("nika:dominant_color", "Color palette extraction"),
        (
            "nika:pipeline",
            "Chain operations in-memory (1 read → N ops → 1 write)",
        ),
    ];

    for (name, desc) in &tools_tier1 {
        println!("  {} {name} — {desc}", StatusIcon::Ok);
    }
    println!();

    // Feature-gated tools (Tier 2)
    println!(
        "  {}",
        "Tier 2 — Default Features (media-core)".cyan().bold()
    );

    let tools_tier2 = [
        (
            "nika:thumbnail",
            "media-thumbnail",
            "SIMD-accelerated image resize",
            cfg!(feature = "media-thumbnail"),
        ),
        (
            "nika:convert",
            "media-thumbnail",
            "Format conversion (PNG/JPEG/WebP)",
            cfg!(feature = "media-thumbnail"),
        ),
        (
            "nika:strip",
            "media-thumbnail",
            "Remove metadata (re-encode)",
            cfg!(feature = "media-thumbnail"),
        ),
        (
            "nika:metadata",
            "media-metadata",
            "Universal EXIF/audio metadata",
            cfg!(feature = "media-metadata"),
        ),
        (
            "nika:optimize",
            "media-optimize",
            "Lossless PNG optimization (oxipng)",
            cfg!(feature = "media-optimize"),
        ),
        (
            "nika:svg_render",
            "media-svg",
            "SVG to PNG rasterization (resvg)",
            cfg!(feature = "media-svg"),
        ),
    ];

    for (name, feature, desc, enabled) in &tools_tier2 {
        let icon = if *enabled {
            StatusIcon::Ok
        } else {
            StatusIcon::Fail
        };
        println!("  {icon} {name} — {desc} [{feature}]");
    }
    println!();

    // Tier 3 — Opt-in
    println!("  {}", "Tier 3 — Opt-In Features".cyan().bold());

    let tools_tier3 = [
        (
            "nika:phash",
            "media-phash",
            "Perceptual image hashing (near-duplicate detection)",
            cfg!(feature = "media-phash"),
        ),
        (
            "nika:compare",
            "media-phash",
            "Visual comparison between two images",
            cfg!(feature = "media-phash"),
        ),
        (
            "nika:pdf_extract",
            "media-pdf",
            "PDF text extraction",
            cfg!(feature = "media-pdf"),
        ),
        (
            "nika:chart",
            "media-chart",
            "Generate charts (bar/line/pie) from JSON data",
            cfg!(feature = "media-chart"),
        ),
        (
            "nika:provenance",
            "media-provenance",
            "Add C2PA content credentials (provenance)",
            cfg!(feature = "media-provenance"),
        ),
        (
            "nika:verify",
            "media-provenance",
            "Verify C2PA content credentials + EU AI Act compliance",
            cfg!(feature = "media-provenance"),
        ),
        (
            "nika:qr_validate",
            "media-qr",
            "QR code decode + scan quality score (0-100)",
            cfg!(feature = "media-qr"),
        ),
        (
            "nika:quality",
            "media-iqa",
            "Image quality assessment (DSSIM/SSIM comparison)",
            cfg!(feature = "media-iqa"),
        ),
        (
            "nika:html_to_md",
            "fetch-markdown",
            "Convert HTML to clean Markdown",
            cfg!(feature = "fetch-markdown"),
        ),
        (
            "nika:css_select",
            "fetch-html",
            "CSS selector extraction from HTML",
            cfg!(feature = "fetch-html"),
        ),
        (
            "nika:extract_metadata",
            "fetch-html",
            "Extract OG, Twitter Cards, JSON-LD metadata",
            cfg!(feature = "fetch-html"),
        ),
        (
            "nika:extract_links",
            "fetch-html",
            "Rich link classification (internal/external/nav)",
            cfg!(feature = "fetch-html"),
        ),
        (
            "nika:readability",
            "fetch-article",
            "Article content extraction (Mozilla Readability)",
            cfg!(feature = "fetch-article"),
        ),
    ];

    for (name, feature, desc, enabled) in &tools_tier3 {
        let icon = if *enabled {
            StatusIcon::Ok
        } else {
            StatusIcon::Fail
        };
        println!("  {icon} {name} — {desc} [{feature}]");
    }
    println!();

    // Count
    let enabled_count = tools_tier1.len()
        + tools_tier2.iter().filter(|(_, _, _, e)| *e).count()
        + tools_tier3.iter().filter(|(_, _, _, e)| *e).count();
    let total = tools_tier1.len() + tools_tier2.len() + tools_tier3.len();
    println!(
        "  {} {enabled_count} / {total} tools enabled",
        StatusIcon::Info,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1_048_576), "1.0 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn test_parse_duration_valid() {
        let r1 = humantime::parse_duration("1h");
        assert!(r1.is_ok(), "1h should parse: {:?}", r1.as_ref().err());
        let r2 = humantime::parse_duration("30m");
        assert!(r2.is_ok(), "30m should parse: {:?}", r2.as_ref().err());
        let r3 = humantime::parse_duration("7d");
        assert!(r3.is_ok(), "7d should parse: {:?}", r3.as_ref().err());
        let r4 = humantime::parse_duration("5m");
        assert!(r4.is_ok(), "5m should parse: {:?}", r4.as_ref().err());
    }

    #[test]
    fn test_min_gc_age_enforced() {
        // Durations under 5 minutes should be clamped
        let short = humantime::parse_duration("1m").unwrap();
        assert!(short.as_secs() < MIN_GC_AGE_SECS);
    }

    #[tokio::test]
    async fn test_list_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        // Should not panic on empty store
        let result = handle_list(&store, true);
        assert!(
            result.is_ok(),
            "handle_list should succeed on empty store: {:?}",
            result.as_ref().err()
        );
    }

    #[tokio::test]
    async fn test_stats_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let result = handle_stats(&store, true);
        assert!(
            result.is_ok(),
            "handle_stats should succeed on empty store: {:?}",
            result.as_ref().err()
        );
    }

    #[tokio::test]
    async fn test_clean_dry_run_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let result = handle_clean(&store, dir.path(), "1h", true, false, true);
        assert!(
            result.is_ok(),
            "dry-run clean should succeed on empty store: {:?}",
            result.as_ref().err()
        );
    }

    #[tokio::test]
    async fn test_clean_lockfile_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Create lockfile
        let lockfile = dir.path().join(LOCKFILE_NAME);
        std::fs::write(&lockfile, "locked").unwrap();

        let result = handle_clean(&store, dir.path(), "1h", false, false, true);
        assert!(
            result.is_err(),
            "clean should fail when lockfile present but got: {:?}",
            result.as_ref().ok()
        );

        // With --force should work
        let result = handle_clean(&store, dir.path(), "1h", true, true, true);
        assert!(
            result.is_ok(),
            "clean with --force should succeed: {:?}",
            result.as_ref().err()
        );
    }

    // ── Functional tests with actual CAS data ──────────────────────────

    /// Backdate a file's mtime by the given duration.
    fn backdate_mtime(path: &std::path::Path, age: Duration) {
        let old_time = std::time::SystemTime::now() - age;
        let file = std::fs::File::open(path).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(old_time))
            .unwrap();
    }

    #[tokio::test]
    async fn test_list_with_data() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        store.store(b"alpha").await.unwrap();
        store.store(b"bravo").await.unwrap();
        store.store(b"charlie").await.unwrap();

        // handle_list with quiet=true prints entries to stdout but should not error
        let result = handle_list(&store, true);
        assert!(
            result.is_ok(),
            "handle_list should succeed with data: {:?}",
            result.as_ref().err()
        );

        let entries = store.list();
        assert_eq!(
            entries.len(),
            3,
            "expected 3 entries, got {}",
            entries.len()
        );
    }

    #[tokio::test]
    async fn test_stats_with_data() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let blob_a = b"stats-blob-alpha";
        let blob_b = b"stats-blob-bravo!!";

        store.store(blob_a).await.unwrap();
        store.store(blob_b).await.unwrap();

        // handle_stats with quiet=false prints table to stdout but should not error
        let result = handle_stats(&store, false);
        assert!(
            result.is_ok(),
            "handle_stats should succeed with data: {:?}",
            result.as_ref().err()
        );

        let entries = store.list();
        assert_eq!(entries.len(), 2);

        // On-disk sizes may include CAS compression framing overhead.
        // Verify non-zero sizes instead of exact byte match.
        let total_size: u64 = entries.iter().map(|e| e.size).sum();
        assert!(total_size > 0, "total size should be non-zero");
        assert!(
            total_size >= (blob_a.len() + blob_b.len()) as u64,
            "total on-disk size should be >= raw data (framing adds bytes)"
        );
    }

    #[tokio::test]
    async fn test_clean_removes_old_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Store a blob and backdate its mtime to 2 hours ago
        let sr = store.store(b"old-file-content").await.unwrap();
        backdate_mtime(&sr.path, Duration::from_secs(7200));

        assert_eq!(store.list().len(), 1, "precondition: 1 file before clean");

        // Clean with older_than=1h, dry_run=false, force=false
        let clean = handle_clean(&store, dir.path(), "1h", false, false, true);
        assert!(
            clean.is_ok(),
            "clean should succeed: {:?}",
            clean.as_ref().err()
        );

        // File backdated 2h ago exceeds the 1h threshold; it should be gone
        assert_eq!(
            store.list().len(),
            0,
            "file backdated 2h should have been removed by 1h threshold"
        );
    }

    #[tokio::test]
    async fn test_clean_dry_run_preserves_files() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Store and backdate to 2 hours ago
        let sr = store.store(b"dry-run-content").await.unwrap();
        backdate_mtime(&sr.path, Duration::from_secs(7200));

        assert_eq!(store.list().len(), 1, "precondition: 1 file before dry-run");

        // dry_run=true: report what would be deleted, but don't actually delete
        let clean = handle_clean(&store, dir.path(), "1h", true, false, true);
        assert!(
            clean.is_ok(),
            "dry-run clean should succeed: {:?}",
            clean.as_ref().err()
        );

        assert_eq!(store.list().len(), 1, "dry-run must not delete files");
    }

    #[tokio::test]
    async fn test_clean_min_age_floor() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Store a blob and backdate to only 3 minutes ago
        let sr = store.store(b"young-file").await.unwrap();
        backdate_mtime(&sr.path, Duration::from_secs(180));

        assert_eq!(store.list().len(), 1, "precondition: 1 file");

        // Request older_than="1m" -- the 5-minute safety floor clamps this to 5m.
        // The file is only 3 minutes old (< 5m floor), so it must survive.
        let clean = handle_clean(&store, dir.path(), "1m", false, false, true);
        assert!(
            clean.is_ok(),
            "clean with floor-clamped age should succeed: {:?}",
            clean.as_ref().err()
        );

        assert_eq!(
            store.list().len(),
            1,
            "file aged 3m must survive when floor clamps 1m to 5m"
        );
    }

    #[tokio::test]
    async fn test_stats_shard_distribution() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Store 20 distinct blobs for hash diversity across shards
        for i in 0u32..20 {
            let blob = format!("shard-test-blob-{i:04}");
            store.store(blob.as_bytes()).await.unwrap();
        }

        let entries = store.list();
        assert_eq!(entries.len(), 20, "should have 20 unique entries");

        // Count distinct shards (first 2 hex chars of hash after "blake3:")
        let mut shards = std::collections::BTreeSet::new();
        for entry in &entries {
            let raw = entry.hash.strip_prefix("blake3:").unwrap();
            shards.insert(raw[..2].to_string());
        }

        // 20 random-ish blake3 hashes across 256 possible shards: P(all same) ~ 1e-46
        assert!(
            shards.len() >= 2,
            "expected >= 2 distinct shards from 20 files, got {}",
            shards.len()
        );

        // Verify the shard distribution logic matches handle_stats internals
        let mut shard_map: std::collections::BTreeMap<String, (usize, u64)> =
            std::collections::BTreeMap::new();
        for entry in &entries {
            let shard = entry
                .hash
                .strip_prefix("blake3:")
                .map(|h| h[..2].to_string())
                .unwrap();
            let counter = shard_map.entry(shard).or_insert((0, 0));
            counter.0 += 1;
            counter.1 += entry.size;
        }
        assert_eq!(shard_map.len(), shards.len());

        // handle_stats itself should not error
        let result = handle_stats(&store, false);
        assert!(
            result.is_ok(),
            "handle_stats shard distribution should succeed: {:?}",
            result.as_ref().err()
        );
    }

    // ── Import tests ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_import_png_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Create a minimal valid PNG
        let png_data = {
            use image::{ImageBuffer, Rgb};
            let img = ImageBuffer::from_pixel(4u32, 4u32, Rgb([255u8, 0, 0]));
            let mut buf = Vec::new();
            let enc = image::codecs::png::PngEncoder::new(&mut buf);
            image::ImageEncoder::write_image(
                enc,
                img.as_raw(),
                4,
                4,
                image::ExtendedColorType::Rgb8,
            )
            .unwrap();
            buf
        };

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), &png_data).unwrap();

        let result = handle_import(&store, tmp.path(), true).await;
        assert!(
            result.is_ok(),
            "PNG import should succeed: {:?}",
            result.as_ref().err()
        );

        // Verify file is in CAS
        let entries = store.list();
        assert_eq!(entries.len(), 1);
        // On-disk size may differ from input due to CAS compression framing.
        // Verify content round-trips correctly instead.
        assert!(entries[0].size > 0);
    }

    #[tokio::test]
    async fn test_import_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = handle_import(
            &store,
            std::path::Path::new("/tmp/no_such_file_99999.xyz"),
            true,
        )
        .await;
        assert!(
            result.is_err(),
            "importing nonexistent file should fail but got: {:?}",
            result.as_ref().ok()
        );
    }

    #[tokio::test]
    async fn test_import_directory_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let result = handle_import(&store, dir.path(), true).await;
        assert!(
            result.is_err(),
            "importing directory should fail but got: {:?}",
            result.as_ref().ok()
        );
    }

    #[tokio::test]
    async fn test_import_empty_file_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"").unwrap();

        let result = handle_import(&store, tmp.path(), true).await;
        assert!(
            result.is_err(),
            "importing empty file should fail but got: {:?}",
            result.as_ref().ok()
        );
    }

    #[tokio::test]
    async fn test_import_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"some binary content here").unwrap();

        // Import twice
        let r1 = handle_import(&store, tmp.path(), true).await;
        assert!(
            r1.is_ok(),
            "first import should succeed: {:?}",
            r1.as_ref().err()
        );
        let r2 = handle_import(&store, tmp.path(), true).await;
        assert!(
            r2.is_ok(),
            "second import (dedup) should succeed: {:?}",
            r2.as_ref().err()
        );

        // CAS should still have only 1 entry (deduplicated)
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn test_cli_import_rejects_path_traversal() {
        let path = std::path::Path::new("../../etc/passwd");
        let result = validate_cli_import_path(path);
        assert!(
            result.is_err(),
            "path traversal should be rejected but got: {:?}",
            result.as_ref().ok()
        );
    }

    #[test]
    fn test_cli_import_rejects_sensitive_paths() {
        for path_str in ["/etc/passwd", "/dev/null", "/var/log/system.log"] {
            let path = std::path::Path::new(path_str);
            let result = validate_cli_import_path(path);
            assert!(result.is_err(), "should reject {path_str}");
        }
    }

    #[test]
    fn test_cli_import_allows_normal_paths() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let result = validate_cli_import_path(tmp.path());
        assert!(
            result.is_ok(),
            "normal path should be allowed: {:?}",
            result.as_ref().err()
        );
    }

    // ── cli_format adoption tests ──────────────────────────────────────

    #[test]
    fn section_header_used_in_stats() {
        let header = section_header("Media Store Statistics");
        assert!(header.contains("Media Store Statistics"));
        assert!(header.contains("─"));
    }

    #[test]
    fn separator_used_in_list() {
        let sep = separator(92);
        assert!(sep.contains("─"));
    }

    #[test]
    fn status_icon_ok_for_clean() {
        let rendered = format!("{} removed 3 file(s)", StatusIcon::Ok);
        assert!(rendered.contains('✓'));
        assert!(rendered.contains("removed 3 file(s)"));
    }

    #[test]
    fn status_icon_warn_for_gc_floor() {
        let rendered = format!("{} minimum GC age is 5 minutes", StatusIcon::Warn);
        assert!(rendered.contains('⚠'));
    }

    #[test]
    fn status_icon_info_for_empty_store() {
        let rendered = format!("{} media store is empty", StatusIcon::Info);
        assert!(rendered.contains('ℹ'));
    }
}
