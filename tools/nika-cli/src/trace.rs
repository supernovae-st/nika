// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Trace subcommand handler

use clap::Subcommand;
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

use nika_engine::display::{key_value, section_header, separator, StatusIcon};
use nika_engine::error::NikaError;
use nika_engine::Event;

#[derive(Subcommand)]
pub enum TraceAction {
    /// List all traces
    List {
        /// Show only last N traces
        #[arg(short, long)]
        limit: Option<usize>,
    },

    /// Show details of a trace
    Show {
        /// Generation ID or partial match
        id: String,
    },

    /// Export trace to file
    Export {
        /// Generation ID
        id: String,
        /// Output format (json, yaml)
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Delete old traces
    Clean {
        /// Keep only last N traces
        #[arg(short, long, default_value = "10")]
        keep: usize,
    },

    /// Search across workflow records
    Search {
        /// Search query
        query: String,
        /// Maximum results to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Filter by workflow name
        #[arg(short, long)]
        workflow: Option<String>,
        /// Filter by date (ISO 8601)
        #[arg(long)]
        since: Option<String>,
    },
}

pub fn handle_trace_command(action: TraceAction, _quiet: bool) -> Result<(), NikaError> {
    match action {
        TraceAction::List { limit } => {
            let traces = nika_engine::list_traces()?;
            let traces = match limit {
                Some(n) => traces.into_iter().take(n).collect::<Vec<_>>(),
                None => traces,
            };

            println!("{}", section_header(&format!("Traces ({})", traces.len())));
            println!(
                "  {:<30} {:>10} {:>20}",
                "GENERATION ID".bold(),
                "SIZE".bold(),
                "CREATED".bold()
            );
            println!("{}", separator(62));

            for trace in traces {
                let size = if trace.size_bytes > 1024 * 1024 {
                    format!("{:.1}MB", trace.size_bytes as f64 / 1024.0 / 1024.0)
                } else if trace.size_bytes > 1024 {
                    format!("{:.1}KB", trace.size_bytes as f64 / 1024.0)
                } else {
                    format!("{}B", trace.size_bytes)
                };

                let created = trace
                    .created
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| {
                        chrono::DateTime::from_timestamp(d.as_secs() as i64, 0).map_or_else(
                            || "unknown".to_string(),
                            |dt| dt.format("%Y-%m-%d %H:%M").to_string(),
                        )
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                println!("  {:<30} {:>10} {:>20}", trace.generation_id, size, created);
            }
            Ok(())
        }

        TraceAction::Show { id } => {
            let traces = nika_engine::list_traces()?;
            let trace = traces
                .iter()
                .find(|t| t.generation_id.contains(&id))
                .ok_or_else(|| NikaError::ValidationError {
                    reason: format!("No trace matching '{id}'"),
                })?;

            let content = fs::read_to_string(&trace.path)?;
            let events: Vec<Event> = content
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();

            println!("{}", section_header("Trace Details"));
            println!("{}", key_value("ID", &trace.generation_id));
            println!("{}", key_value("Events", &events.len().to_string()));
            println!(
                "{}",
                key_value("Size", &format!("{} bytes", trace.size_bytes))
            );
            println!();

            for event in events {
                println!("[{:>6}ms] {:?}", event.timestamp_ms, event.kind);
            }
            Ok(())
        }

        TraceAction::Export { id, format, output } => {
            let traces = nika_engine::list_traces()?;
            let trace = traces
                .iter()
                .find(|t| t.generation_id.contains(&id))
                .ok_or_else(|| NikaError::ValidationError {
                    reason: format!("No trace matching '{id}'"),
                })?;

            let content = fs::read_to_string(&trace.path)?;
            let events: Vec<Event> = content
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect();

            let exported = match format.as_str() {
                "json" => serde_json::to_string_pretty(&events)?,
                "yaml" => nika_engine::serde_yaml::to_string(&events).map_err(|e| {
                    NikaError::SerializationError {
                        details: e.to_string(),
                    }
                })?,
                other => {
                    return Err(NikaError::ValidationError {
                        reason: format!("Unknown format: {other}. Use 'json' or 'yaml'"),
                    })
                }
            };

            match output {
                Some(path) => {
                    fs::write(&path, &exported)?;
                    println!(
                        "{} exported {} events to {}",
                        StatusIcon::Ok,
                        events.len(),
                        path.display()
                    );
                }
                None => println!("{exported}"),
            }
            Ok(())
        }

        TraceAction::Clean { keep } => {
            let traces = nika_engine::list_traces()?;
            let to_delete: Vec<_> = traces.into_iter().skip(keep).collect();
            let count = to_delete.len();

            for trace in to_delete {
                fs::remove_file(&trace.path)?;
            }

            println!("{} deleted {count} old traces, kept {keep}", StatusIcon::Ok);
            Ok(())
        }

        TraceAction::Search {
            query,
            limit,
            workflow,
            since,
        } => {
            let records_dir = PathBuf::from(".nika/records");
            if !records_dir.exists() {
                println!("No records found");
                return Ok(());
            }

            let query_lower = query.to_lowercase();

            let mut results: Vec<serde_json::Value> = Vec::new();

            let entries = fs::read_dir(&records_dir)?;

            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("ndjson") {
                    continue;
                }

                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                for line in content.lines() {
                    let record: serde_json::Value = match serde_json::from_str(line) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Filter by query (case-insensitive substring match in summary)
                    let summary = record.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                    if !summary.to_lowercase().contains(&query_lower) {
                        continue;
                    }

                    // Filter by workflow name
                    if let Some(ref wf) = workflow {
                        let record_wf = record
                            .get("workflow")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if record_wf != wf {
                            continue;
                        }
                    }

                    // Filter by since date
                    if let Some(ref since_str) = since {
                        let record_date = record.get("date").and_then(|v| v.as_str()).unwrap_or("");
                        if record_date < since_str.as_str() {
                            continue;
                        }
                    }

                    results.push(record);
                }
            }

            // Sort by confidence descending
            results.sort_by(|a, b| {
                let ca = a.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let cb = b.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
                cb.partial_cmp(&ca).unwrap_or(std::cmp::Ordering::Equal)
            });

            // Limit results
            results.truncate(limit);

            println!(
                "{}",
                format!("Records matching \"{}\" ({} results)", query, results.len()).bold()
            );
            println!(
                "  {:<22} {:<13} {:<12} {}",
                "WORKFLOW".bold(),
                "TASK".bold(),
                "CONFIDENCE".bold(),
                "DATE".bold()
            );
            println!("{}", separator(62));

            for record in &results {
                let wf = record
                    .get("workflow")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-");
                let task = record.get("task").and_then(|v| v.as_str()).unwrap_or("-");
                let confidence = record
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let date = record.get("date").and_then(|v| v.as_str()).unwrap_or("-");
                let summary = record.get("summary").and_then(|v| v.as_str()).unwrap_or("");

                println!(
                    "  {:<22} {:<13} {:<12.2} {}",
                    wf.cyan(),
                    task,
                    confidence,
                    date
                );

                // Show truncated summary as preview
                let preview: String = summary.chars().take(60).collect();
                let ellipsis = if summary.len() > 60 { "..." } else { "" };
                println!("    {}{}", preview.dimmed(), ellipsis.dimmed());
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_action_list_variants_exist() {
        let _ = TraceAction::List { limit: Some(10) };
        let _ = TraceAction::Show {
            id: "abc".to_string(),
        };
        let _ = TraceAction::Export {
            id: "abc".to_string(),
            format: "json".to_string(),
            output: None,
        };
        let _ = TraceAction::Clean { keep: 5 };
    }

    #[test]
    fn section_header_used_in_list() {
        let header = section_header("Traces (3)");
        assert!(header.contains("Traces (3)"));
        assert!(header.contains("─"));
    }

    #[test]
    fn key_value_used_in_show() {
        let kv = key_value("ID", "abc123");
        assert!(kv.contains("ID:"));
        assert!(kv.contains("abc123"));
    }

    #[test]
    fn separator_used_in_list() {
        let sep = separator(62);
        assert!(sep.contains("─"));
    }

    #[test]
    fn trace_action_search_variant_exists() {
        let _ = TraceAction::Search {
            query: "test".to_string(),
            limit: 10,
            workflow: None,
            since: None,
        };
    }
}
