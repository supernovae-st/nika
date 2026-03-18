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
    let store_root = workspace_root.join(".nika").join("media").join("store");

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
            return Err(NikaError::ConfigError {
                reason: format!(
                    "Media store is locked by a running workflow ({}). \
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
}
