// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika tools` — discover builtin tools and their parameter schemas.

use clap::Subcommand;
use nika_engine::runtime::builtin::BuiltinToolRouter;

/// Builtin tool discovery and documentation.
#[derive(Subcommand)]
pub enum ToolsAction {
    /// List all builtin tools with descriptions
    #[command(visible_alias = "ls")]
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show parameter schema for a specific tool
    Info {
        /// Tool name (e.g., "sleep", "glob", "read" — without nika: prefix)
        name: String,
    },
}

pub fn handle_tools_command(action: ToolsAction) {
    // Create router with core tools only (no file/media context needed for listing)
    let router = BuiltinToolRouter::new();

    match action {
        ToolsAction::List { json } => list_tools(&router, json),
        ToolsAction::Info { name } => info_tool(&router, &name),
    }
}

fn list_tools(router: &BuiltinToolRouter, json: bool) {
    let mut names = router.tool_names();
    names.sort();

    // Also include file tools (not registered without context, but known)
    let file_tools = ["read", "write", "edit", "glob", "grep"];
    let file_descriptions = [
        "Read file with line numbers",
        "Create or overwrite file",
        "Modify file (old_string → new_string)",
        "Find files by glob pattern",
        "Search content with regex",
    ];

    if json {
        let mut entries: Vec<serde_json::Value> = names
            .iter()
            .map(|name| {
                let desc = router.get_tool(name).map(|t| t.description()).unwrap_or("");
                serde_json::json!({ "name": format!("nika:{}", name), "description": desc })
            })
            .collect();

        for (name, desc) in file_tools.iter().zip(file_descriptions.iter()) {
            if !names.contains(name) {
                entries.push(
                    serde_json::json!({ "name": format!("nika:{}", name), "description": desc }),
                );
            }
        }

        entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        println!("{}", serde_json::to_string_pretty(&entries).unwrap());
    } else {
        println!("Builtin tools (nika:*):\n");

        // Merge core + file tools
        let mut all: Vec<(String, String)> = names
            .iter()
            .map(|name| {
                let desc = router
                    .get_tool(name)
                    .map(|t| t.description().to_string())
                    .unwrap_or_default();
                (name.to_string(), desc)
            })
            .collect();

        for (name, desc) in file_tools.iter().zip(file_descriptions.iter()) {
            if !names.contains(name) {
                all.push((name.to_string(), desc.to_string()));
            }
        }

        all.sort_by(|a, b| a.0.cmp(&b.0));

        let max_name_len = all.iter().map(|(n, _)| n.len() + 5).max().unwrap_or(15);

        for (name, desc) in &all {
            let padded = format!("nika:{}", name);
            println!("  {:<width$}  {}", padded, desc, width = max_name_len);
        }

        println!(
            "\n{} tools available. Use `nika tools info <name>` for param schema.",
            all.len()
        );
    }
}

fn info_tool(router: &BuiltinToolRouter, name: &str) {
    // Strip nika: prefix if provided
    let clean_name = name.strip_prefix("nika:").unwrap_or(name);

    if let Some(tool) = router.get_tool(clean_name) {
        println!("nika:{}\n", clean_name);
        println!("  {}\n", tool.description());
        println!("Parameters:");
        let schema = tool.parameters_schema();
        println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    } else {
        // Check file tools (not registered without context)
        let file_tools = [
            (
                "read",
                "Read file with line numbers",
                r#"{"type":"object","properties":{"file_path":{"type":"string","description":"Absolute or relative path to read"}},"required":["file_path"],"additionalProperties":false}"#,
            ),
            (
                "write",
                "Create or overwrite file",
                r#"{"type":"object","properties":{"file_path":{"type":"string","description":"Path to write"},"content":{"type":"string","description":"File content"}},"required":["file_path","content"],"additionalProperties":false}"#,
            ),
            (
                "edit",
                "Modify file (old_string → new_string)",
                r#"{"type":"object","properties":{"file_path":{"type":"string","description":"Path to file"},"old_string":{"type":"string","description":"Text to find"},"new_string":{"type":"string","description":"Replacement text"}},"required":["file_path","old_string","new_string"],"additionalProperties":false}"#,
            ),
            (
                "glob",
                "Find files by glob pattern",
                r#"{"type":"object","properties":{"pattern":{"type":"string","description":"Glob pattern (e.g. **/*.yaml)"}},"required":["pattern"],"additionalProperties":false}"#,
            ),
            (
                "grep",
                "Search content with regex",
                r#"{"type":"object","properties":{"pattern":{"type":"string","description":"Regex pattern to search"},"path":{"type":"string","description":"Directory or file to search in"}},"required":["pattern"],"additionalProperties":false}"#,
            ),
        ];

        if let Some((_, desc, schema_str)) = file_tools.iter().find(|(n, _, _)| *n == clean_name) {
            println!("nika:{}\n", clean_name);
            println!("  {}\n", desc);
            println!("Parameters:");
            let schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();
            println!("{}", serde_json::to_string_pretty(&schema).unwrap());
        } else {
            eprintln!("Unknown tool: nika:{}", clean_name);
            eprintln!("Run `nika tools list` to see available tools.");
            std::process::exit(1);
        }
    }
}
