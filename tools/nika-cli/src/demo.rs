// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika demo` and agent preset display handlers.

use colored::Colorize;
use nika_engine::error::NikaError;

/// Run the built-in 8-task DAG demo — Nika's manifesto as a workflow.
///
/// Diamond pattern: start → [write, connect, track] → [build, run] → launch → celebrate
/// 8 tasks, 4 layers, fan-out + fan-in, no API key needed.
pub async fn run_demo(
    quiet: bool,
    detail: nika_engine::display::DetailLevel,
) -> Result<(), NikaError> {
    const DEMO_YAML: &str = r#"schema: "nika/workflow@0.12"
workflow: hello-nika
description: "Welcome to Nika — this is running live right now"

tasks:
  - id: start
    exec: "echo 'Hey! This is Nika — a real DAG running live.'"

  - id: write
    depends_on: [start]
    exec: "echo 'Write YAML. Nika resolves deps and runs it.'"

  - id: connect
    depends_on: [start]
    exec: "echo '7 providers: Claude, GPT, Gemini, Mistral, Groq, xAI, local.'"

  - id: track
    depends_on: [start]
    exec: "echo 'Every token counted. Every cent tracked.'"

  - id: build
    depends_on: [write, connect]
    exec: "echo 'DAG, parallel exec, MCP tools, media pipeline.'"

  - id: run
    depends_on: [connect, track]
    exec: "echo 'Headless CLI, TUI, or embed as a library.'"

  - id: launch
    depends_on: [build, run]
    exec: "echo 'One file. Any AI. Ship it.'"

  - id: celebrate
    depends_on: [launch]
    exec: "echo 'Welcome aboard, captain.'"
"#;

    println!();
    println!(
        "  \u{1f98b} {}  {}",
        format!("nika v{}", env!("CARGO_PKG_VERSION")).bold(),
        "live demo".dimmed()
    );
    println!();
    println!("  {}", "This is a real workflow running live.".dimmed());
    println!(
        "  {}",
        "No API key. No setup. Just a YAML file and a DAG.".dimmed()
    );

    // Show the DAG visualization before running
    {
        use nika_engine::display::{render_dag, DagTask, DagTaskStatus};
        use std::collections::HashMap;

        let names = [
            "start",
            "write",
            "connect",
            "track",
            "build",
            "run",
            "launch",
            "celebrate",
        ];
        let dag_tasks: Vec<DagTask> = names
            .iter()
            .map(|id| DagTask {
                id: (*id).into(),
                verb: "exec".into(),
                status: DagTaskStatus::Pending,
                meta: None,
                tags: vec![],
            })
            .collect();

        let mut deps: HashMap<String, Vec<String>> = HashMap::new();
        deps.insert("write".into(), vec!["start".into()]);
        deps.insert("connect".into(), vec!["start".into()]);
        deps.insert("track".into(), vec!["start".into()]);
        deps.insert("build".into(), vec!["write".into(), "connect".into()]);
        deps.insert("run".into(), vec!["connect".into(), "track".into()]);
        deps.insert("launch".into(), vec!["build".into(), "run".into()]);
        deps.insert("celebrate".into(), vec!["launch".into()]);

        render_dag(&dag_tasks, &deps);
    }

    // Write temp file, run, clean up
    let tmp = std::env::temp_dir().join("nika-demo.nika.yaml");
    tokio::fs::write(&tmp, DEMO_YAML).await?;

    let result = crate::run::run_workflow(
        &tmp.display().to_string(),
        None,
        None,
        &[],
        None,
        false,
        None,
        None,
        None,
        true, // skip cost confirm for demo
        quiet,
        detail,
        false, // demo always uses live renderer
        "deny",
        false,
    )
    .await;

    let _ = tokio::fs::remove_file(&tmp).await;

    result?;

    println!();
    println!(
        "  {} {}",
        "Next:".cyan().bold(),
        "nika new hello --verb exec".bold(),
    );
    println!(
        "  {}",
        "Create your first workflow. It's just a YAML file.".dimmed()
    );
    println!();

    Ok(())
}

/// Display built-in agent presets in a table format.
pub fn print_agent_presets() {
    use nika_engine::runtime::resolver::default_presets;

    let presets = default_presets();
    let mut names: Vec<&String> = presets.keys().collect();
    names.sort();

    println!("\n {} {}\n", "⟡".cyan(), "Built-in Agent Presets".bold());
    println!(
        "  {:<12} {:<28} {:<6} {}",
        "NAME".dimmed(),
        "MODEL".dimmed(),
        "TEMP".dimmed(),
        "DESCRIPTION".dimmed(),
    );
    println!("  {}", "─".repeat(80).dimmed());

    for name in &names {
        let agent = &presets[*name];
        let model = agent.model.as_deref().unwrap_or("default");
        let temp = agent
            .temperature
            .map(|t| format!("{:.1}", t))
            .unwrap_or_else(|| "—".to_string());
        // First sentence of system prompt as description
        let desc = agent.system.split('.').next().unwrap_or(&agent.system);
        println!(
            "  {:<12} {:<28} {:<6} {}",
            name.cyan(),
            model.dimmed(),
            temp,
            desc,
        );
    }

    println!(
        "\n  {} Use with: {} or {}",
        "→".dimmed(),
        "agent: <name>".cyan(),
        "preset: <name>".cyan(),
    );
    println!(
        "  {} Override in workflow {} block\n",
        "→".dimmed(),
        "agents:".cyan(),
    );
}
