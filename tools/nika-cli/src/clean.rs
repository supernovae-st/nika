// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika clean` — umbrella cleanup command for project runtime state.
//!
//! Removes traces, media orphans, and cache in one command.
//! Wraps: `trace clean`, `media clean`, `cache clear`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use nika_engine::display::StatusIcon;
use nika_engine::error::NikaError;

/// Options for the clean command.
#[derive(Debug, Clone)]
pub struct CleanOptions {
    /// Preview only, don't actually delete
    pub dry_run: bool,
    /// Also remove serve.db + sessions/
    pub all: bool,
    /// Suppress output
    pub quiet: bool,
}

/// Result of a single clean category.
#[derive(Debug, Default)]
pub struct CleanResult {
    pub category: String,
    pub files_removed: usize,
    pub bytes_freed: u64,
}

/// Result of the full clean operation.
#[derive(Debug, Default)]
pub struct CleanReport {
    pub results: Vec<CleanResult>,
    pub dry_run: bool,
}

impl CleanReport {
    pub fn total_bytes(&self) -> u64 {
        self.results.iter().map(|r| r.bytes_freed).sum()
    }

    pub fn total_files(&self) -> usize {
        self.results.iter().map(|r| r.files_removed).sum()
    }
}

/// Run the full clean operation on a .nika/ directory.
pub fn run_clean(nika_dir: &Path, opts: &CleanOptions) -> Result<CleanReport, NikaError> {
    let mut report = CleanReport {
        dry_run: opts.dry_run,
        ..Default::default()
    };

    // 1. Traces
    let traces_dir = nika_dir.join("traces");
    if traces_dir.exists() {
        let result = clean_directory(&traces_dir, None, opts.dry_run, "Traces")?;
        report.results.push(result);
    }

    // 2. Cache
    let cache_dir = nika_dir.join("cache");
    if cache_dir.exists() {
        let result = clean_directory(&cache_dir, None, opts.dry_run, "Cache")?;
        report.results.push(result);
    }

    // 3. Media orphans (older than 1 hour)
    let media_dir = nika_dir.join("media").join("store");
    if media_dir.exists() {
        let one_hour = Duration::from_secs(3600);
        let result = clean_directory(&media_dir, Some(one_hour), opts.dry_run, "Media")?;
        report.results.push(result);
    }

    // 4. --all: also remove serve.db + sessions/
    if opts.all {
        let serve_db = nika_dir.join("serve.db");
        if serve_db.exists() {
            let size = fs::metadata(&serve_db).map(|m| m.len()).unwrap_or(0);
            if !opts.dry_run {
                fs::remove_file(&serve_db).ok();
            }
            report.results.push(CleanResult {
                category: "Serve DB".to_string(),
                files_removed: 1,
                bytes_freed: size,
            });
        }

        let sessions_dir = nika_dir.join("sessions");
        if sessions_dir.exists() {
            let result = clean_directory(&sessions_dir, None, opts.dry_run, "Sessions")?;
            report.results.push(result);
        }
    }

    Ok(report)
}

/// Clean files in a directory, optionally filtering by age.
fn clean_directory(
    dir: &Path,
    min_age: Option<Duration>,
    dry_run: bool,
    category: &str,
) -> Result<CleanResult, NikaError> {
    let mut result = CleanResult {
        category: category.to_string(),
        ..Default::default()
    };

    let now = SystemTime::now();
    let entries = collect_files_recursive(dir)?;

    for path in entries {
        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        // Skip if too recent
        if let Some(min_age) = min_age {
            if let Ok(modified) = meta.modified() {
                if let Ok(age) = now.duration_since(modified) {
                    if age < min_age {
                        continue;
                    }
                }
            }
        }

        result.bytes_freed += meta.len();
        result.files_removed += 1;

        if !dry_run {
            fs::remove_file(&path).ok();
        }
    }

    Ok(result)
}

/// Recursively collect all files in a directory.
fn collect_files_recursive(dir: &Path) -> Result<Vec<PathBuf>, NikaError> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_files_recursive(&path)?);
        } else {
            files.push(path);
        }
    }

    Ok(files)
}

/// Format bytes as human-readable string.
pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Print the clean report.
pub fn print_report(report: &CleanReport, quiet: bool) {
    if quiet {
        return;
    }

    let icon = if report.dry_run {
        StatusIcon::Info
    } else {
        StatusIcon::Ok
    };
    let verb = if report.dry_run {
        "Would remove"
    } else {
        "Removed"
    };

    println!();
    for r in &report.results {
        if r.files_removed > 0 {
            println!(
                "  {:9} {:<8} {} file(s){}",
                format!("{}:", r.category),
                verb.to_lowercase(),
                r.files_removed,
                if r.bytes_freed > 0 {
                    format!("  ({})", format_bytes(r.bytes_freed))
                } else {
                    String::new()
                }
            );
        } else {
            println!("  {:9} nothing to clean", format!("{}:", r.category));
        }
    }

    let total = report.total_bytes();
    if total > 0 {
        println!();
        println!(
            "  {} {} {}",
            icon,
            if report.dry_run {
                "Would free"
            } else {
                "Freed"
            },
            format_bytes(total)
        );
    } else {
        println!();
        println!("  {} Nothing to clean", StatusIcon::Info);
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // Test 31: clean runs all three categories
    #[test]
    fn clean_runs_all_three() {
        let temp = tempdir().unwrap();
        let nika_dir = temp.path().join(".nika");

        // Create trace files
        let traces = nika_dir.join("traces");
        fs::create_dir_all(&traces).unwrap();
        fs::write(traces.join("trace1.ndjson"), "data").unwrap();
        fs::write(traces.join("trace2.ndjson"), "more data").unwrap();

        // Create cache files
        let cache = nika_dir.join("cache");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("response.json"), "cached").unwrap();

        // Create media files (old enough to be cleaned)
        let media = nika_dir.join("media").join("store");
        fs::create_dir_all(&media).unwrap();
        let blob_path = media.join("abc123");
        fs::write(&blob_path, "blob data").unwrap();
        // Set modification time to 2 hours ago
        let two_hours_ago =
            filetime::FileTime::from_system_time(SystemTime::now() - Duration::from_secs(7200));
        filetime::set_file_mtime(&blob_path, two_hours_ago).unwrap();

        let opts = CleanOptions {
            dry_run: false,
            all: false,
            quiet: true,
        };
        let report = run_clean(&nika_dir, &opts).unwrap();

        assert!(report.total_files() > 0, "should have cleaned files");
        assert!(report.total_bytes() > 0, "should have freed bytes");
        // Check all 3 categories present
        let categories: Vec<&str> = report.results.iter().map(|r| r.category.as_str()).collect();
        assert!(categories.contains(&"Traces"), "should have Traces");
        assert!(categories.contains(&"Cache"), "should have Cache");
        assert!(categories.contains(&"Media"), "should have Media");
    }

    // Test 32: dry_run doesn't delete
    #[test]
    fn clean_dry_run_no_delete() {
        let temp = tempdir().unwrap();
        let nika_dir = temp.path().join(".nika");
        let traces = nika_dir.join("traces");
        fs::create_dir_all(&traces).unwrap();
        fs::write(traces.join("trace.ndjson"), "data").unwrap();

        let opts = CleanOptions {
            dry_run: true,
            all: false,
            quiet: true,
        };
        let report = run_clean(&nika_dir, &opts).unwrap();

        assert!(report.dry_run);
        assert!(report.total_files() > 0, "should report files");
        // File should still exist
        assert!(
            traces.join("trace.ndjson").exists(),
            "file should NOT be deleted in dry run"
        );
    }

    // Test 33: --all includes serve.db + sessions
    #[test]
    fn clean_all_includes_serve_db() {
        let temp = tempdir().unwrap();
        let nika_dir = temp.path().join(".nika");
        fs::create_dir_all(&nika_dir).unwrap();

        // Create serve.db
        fs::write(nika_dir.join("serve.db"), "sqlite data").unwrap();

        // Create sessions
        let sessions = nika_dir.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        fs::write(sessions.join("session1.json"), "state").unwrap();

        let opts = CleanOptions {
            dry_run: false,
            all: true,
            quiet: true,
        };
        let report = run_clean(&nika_dir, &opts).unwrap();

        let categories: Vec<&str> = report.results.iter().map(|r| r.category.as_str()).collect();
        assert!(categories.contains(&"Serve DB"), "should include Serve DB");
        assert!(categories.contains(&"Sessions"), "should include Sessions");
        assert!(
            !nika_dir.join("serve.db").exists(),
            "serve.db should be deleted"
        );
    }

    // Test 34: reports bytes freed per category
    #[test]
    fn clean_reports_bytes_freed() {
        let temp = tempdir().unwrap();
        let nika_dir = temp.path().join(".nika");
        let cache = nika_dir.join("cache");
        fs::create_dir_all(&cache).unwrap();

        let data = "x".repeat(1024); // 1KB
        fs::write(cache.join("file1.json"), &data).unwrap();
        fs::write(cache.join("file2.json"), &data).unwrap();

        let opts = CleanOptions {
            dry_run: false,
            all: false,
            quiet: true,
        };
        let report = run_clean(&nika_dir, &opts).unwrap();

        let cache_result = report
            .results
            .iter()
            .find(|r| r.category == "Cache")
            .expect("Cache category present");
        assert!(
            cache_result.bytes_freed >= 2048,
            "should report ~2KB freed, got {}",
            cache_result.bytes_freed
        );
        assert_eq!(cache_result.files_removed, 2, "should report 2 files");
    }
}
