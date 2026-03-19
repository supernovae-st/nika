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

use nika::error::NikaError;
use nika::media::CasStore;

/// Minimum GC age floor: 5 minutes (safety: prevent deleting in-flight media)
const MIN_GC_AGE_SECS: u64 = 300;

/// Lockfile name written by runner during workflow execution
const LOCKFILE_NAME: &str = ".nika-run.lock";

#[derive(Subcommand, Debug)]
pub enum MediaAction {
    /// List all files in the media store
    List,

    /// Show store statistics (count, size, shard distribution)
    Stats,

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
        MediaAction::List => handle_list(&store, quiet),
        MediaAction::Stats => handle_stats(&store, quiet),
        MediaAction::Clean {
            older_than,
            dry_run,
            force,
        } => handle_clean(&store, &store_root, &older_than, dry_run, force, quiet),
    }
}

fn handle_list(store: &CasStore, quiet: bool) -> Result<(), NikaError> {
    let entries = store.list();

    if entries.is_empty() {
        if !quiet {
            println!("{}", "Media store is empty.".dimmed());
        }
        return Ok(());
    }

    if !quiet {
        println!(
            "{:<68}  {:>10}  {}",
            "HASH".bold(),
            "SIZE".bold(),
            "PATH".bold()
        );
    }

    for entry in &entries {
        println!(
            "{:<68}  {:>10}  {}",
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
    let mut shards: std::collections::BTreeMap<String, (usize, u64)> = std::collections::BTreeMap::new();
    for entry in &entries {
        // Extract shard prefix (first 2 chars of hash after "blake3:")
        let shard = entry
            .hash
            .strip_prefix("blake3:")
            .map(|h: &str| h[..2.min(h.len())].to_string())
            .unwrap_or_else(|| "??".to_string());
        let counter = shards.entry(shard).or_insert((0, 0));
        counter.0 += 1;
        counter.1 += entry.size;
    }

    if quiet {
        println!("{count}");
        return Ok(());
    }

    println!("{}", "Media Store Statistics".bold());
    println!("  Files:      {}", count);
    println!("  Total size: {}", format_bytes(total_size));
    println!("  Shards:     {}", shards.len());

    if !shards.is_empty() {
        println!("\n{}", "Shard Distribution:".bold());
        for (shard, (shard_count, shard_size)) in &shards {
            println!(
                "  {}/  {:>4} files  {:>10}",
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
        reason: format!("Invalid duration '{}': {}. Examples: 1h, 30m, 7d", older_than, e),
    })?;

    // Enforce minimum GC age (5 minutes)
    let duration = if duration.as_secs() < MIN_GC_AGE_SECS {
        if !quiet {
            println!(
                "{} Minimum GC age is 5 minutes, using 5m instead of '{}'",
                "⚠".yellow(),
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
                                    "would delete:".yellow(),
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
                "\n{} Would delete {} file(s), freeing {}",
                "dry-run:".cyan().bold(),
                would_delete,
                format_bytes(would_free)
            );
        }
    } else {
        let result = store.clean_older_than(duration);
        // TODO(PR2): Emit EventKind::MediaCleanup here. The CLI has no EventLog
        // because it runs outside the workflow runner context. Options: accept an
        // optional EventLog param, write one-shot NDJSON to trace dir, or defer
        // until `nika media clean` integrates with the TUI. The MediaCleanup
        // variant exists in EventKind but is never emitted.
        if !quiet {
            println!(
                "{} Removed {} file(s), freed {}",
                "✓".green(),
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
        format!("{} B", bytes)
    }
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
        assert!(humantime::parse_duration("1h").is_ok());
        assert!(humantime::parse_duration("30m").is_ok());
        assert!(humantime::parse_duration("7d").is_ok());
        assert!(humantime::parse_duration("5m").is_ok());
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
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stats_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let result = handle_stats(&store, true);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_clean_dry_run_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());
        let result = handle_clean(&store, dir.path(), "1h", true, false, true);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_clean_lockfile_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let store = CasStore::new(dir.path());

        // Create lockfile
        let lockfile = dir.path().join(LOCKFILE_NAME);
        std::fs::write(&lockfile, "locked").unwrap();

        let result = handle_clean(&store, dir.path(), "1h", false, false, true);
        assert!(result.is_err());

        // With --force should work
        let result = handle_clean(&store, dir.path(), "1h", true, true, true);
        assert!(result.is_ok());
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
        assert!(result.is_ok());

        let entries = store.list();
        assert_eq!(entries.len(), 3, "expected 3 entries, got {}", entries.len());
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
        assert!(result.is_ok());

        let entries = store.list();
        assert_eq!(entries.len(), 2);

        let total_size: u64 = entries.iter().map(|e| e.size).sum();
        assert_eq!(
            total_size,
            (blob_a.len() + blob_b.len()) as u64,
            "total size mismatch"
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
        assert!(clean.is_ok());

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
        assert!(clean.is_ok());

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
        assert!(clean.is_ok());

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
        assert!(result.is_ok());
    }

}
