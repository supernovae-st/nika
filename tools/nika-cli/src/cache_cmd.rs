// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika cache` subcommand handler.

use clap::Subcommand;
use colored::Colorize;
use std::time::Duration;

use nika_daemon::{daemon_socket_path, DaemonClient, DaemonRequest, DaemonResponse};
use nika_engine::error::NikaError;

#[derive(Subcommand)]
pub enum CacheAction {
    /// Show cache statistics
    Stats,
    /// Clear all cached responses
    Clear,
}

pub async fn handle_cache_command(action: CacheAction, quiet: bool) -> Result<(), NikaError> {
    let client = DaemonClient::new(daemon_socket_path()).with_timeout(Duration::from_secs(5));

    if !client.socket_exists() {
        eprintln!(
            "{} daemon not running — start with: nika daemon start",
            "✗".red().bold()
        );
        return Ok(());
    }

    match action {
        CacheAction::Stats => {
            let resp = client
                .send(DaemonRequest::CacheStats)
                .await
                .map_err(cache_err)?;

            match resp {
                DaemonResponse::CacheStatsResult {
                    entries,
                    hits,
                    misses,
                    evictions,
                    total_tokens_saved,
                    total_cost_saved,
                } => {
                    let total = hits + misses;
                    let hit_rate = if total > 0 {
                        (hits as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    };

                    println!("{}", "LLM Response Cache".bold());
                    println!("  entries:      {}", entries);
                    println!(
                        "  hit rate:     {:.1}% ({} hits / {} misses)",
                        hit_rate, hits, misses
                    );
                    println!("  evictions:    {}", evictions);
                    println!("  tokens saved: {}", total_tokens_saved);
                    println!("  cost saved:   ${:.4}", total_cost_saved);
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }

        CacheAction::Clear => {
            let resp = client
                .send(DaemonRequest::CacheClear)
                .await
                .map_err(cache_err)?;

            match resp {
                DaemonResponse::Ok => {
                    if !quiet {
                        println!("{} cache cleared", "✓".green().bold());
                    }
                }
                DaemonResponse::Error { code, message } => {
                    eprintln!("{} [{code}] {message}", "✗".red().bold());
                }
                _ => eprintln!("{} unexpected response", "✗".red().bold()),
            }
        }
    }

    Ok(())
}

fn cache_err(e: nika_daemon::DaemonError) -> NikaError {
    NikaError::Execution(format!("cache: {e}"))
}
