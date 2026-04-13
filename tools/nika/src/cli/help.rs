// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Custom help system — beautiful grouped CLI help with cosmic theme.
//!
//! Replaces clap's flat command list with categorised, colored output.
//! Sections are derived from `help_heading` attributes on the `Commands` enum,
//! so new commands appear automatically without editing this file.
//! Supports deep-dive topics via `nika help <topic>`.

use colored::Colorize;

// ═══════════════════════════════════════════════════════════════════════════
// BANNER
// ═══════════════════════════════════════════════════════════════════════════

/// Box width constant — shared between banner and separators.
const HELP_WIDTH: usize = 58;

fn print_banner() {
    let version = env!("CARGO_PKG_VERSION");
    let inner = HELP_WIDTH - 4; // 4 = 2 border + 2 padding
    let bar = "\u{2500}".repeat(inner + 2);

    let l1_right = format!("v{version}");
    // "✧ N I K A" = 9 display columns (✧ is 1 col but 3 UTF-8 bytes)
    let l1_pad = inner - 9 - l1_right.len();

    let l2 = "Semantic YAML workflow engine for AI tasks";
    let l2_pad = inner - l2.len();

    let l3 = "5 verbs \u{00B7} 9 providers \u{00B7} 30+ builtin tools";
    let l3_pad = inner - l3.len();

    println!();
    println!(
        "  {}{}{}",
        "\u{256D}".dimmed(),
        bar.dimmed(),
        "\u{256E}".dimmed()
    );

    print!("  {} ", "\u{2502}".dimmed());
    print!("{} {}", "\u{2727}".magenta().bold(), "N I K A".bold());
    println!(
        "{}{} {}",
        " ".repeat(l1_pad),
        l1_right.dimmed(),
        "\u{2502}".dimmed()
    );

    print!("  {} ", "\u{2502}".dimmed());
    println!("{l2}{} {}", " ".repeat(l2_pad), "\u{2502}".dimmed());

    print!("  {} ", "\u{2502}".dimmed());
    print!("{}", l3.dimmed());
    println!("{} {}", " ".repeat(l3_pad), "\u{2502}".dimmed());

    println!(
        "  {}{}{}",
        "\u{2570}".dimmed(),
        bar.dimmed(),
        "\u{256F}".dimmed()
    );
    println!();
}

// ═══════════════════════════════════════════════════════════════════════════
// STATIC METADATA
// ═══════════════════════════════════════════════════════════════════════════

/// Display order for command sections (matches help_heading values in Commands enum).
/// Sections not in this list are silently skipped.
const SECTION_ORDER: &[&str] = &[
    "WORKFLOWS",
    "5 VERBS",
    "INTERACTIVE",
    "MODELS & PROVIDERS",
    "LEARNING",
    "PROJECT",
    "SYSTEM",
];

/// Short curated description for each command (fits the 32-char help column).
/// Falls back to a truncated clap about for unknown commands.
fn get_short_desc(name: &str) -> Option<&'static str> {
    match name {
        "run" => Some("Run a workflow file"),
        "check" => Some("Validate syntax & DAG"),
        "new" => Some("Create a new workflow"),
        "workflow" => Some("Edit, graph, add-task"),
        "infer" => Some("Call an LLM directly"),
        "fetch" => Some("Fetch URL with extraction"),
        "invoke" => Some("Builtin or MCP tool"),
        "agent" => Some("Multi-turn AI agent"),
        "provider" => Some("Manage API keys"),
        "mcp" => Some("MCP server connections"),
        "model" => Some("LLM models (cloud + local)"),
        "showcase" => Some("Browse 115+ showcase workflows"),
        "init" => Some("Initialize .nika/ project"),
        "config" => Some("Manage configuration"),
        "media" => Some("Media store management"),
        "doctor" => Some("System health check"),
        "cache" => Some("LLM response cache"),
        "job" => Some("Background jobs"),
        "daemon" => Some("Background daemon"),
        "setup" => Some("API key setup wizard"),
        "features" => Some("Compiled feature flags"),
        "completion" => Some("Shell completions"),
        "trace" => Some("Execution traces"),
        "schema" => Some("Schema versions & migrations"),
        _ => None,
    }
}

/// One-line example for each command (shown in the right column).
fn get_example(name: &str) -> &'static str {
    match name {
        "run" => "nika run flow.nika.yaml",
        "check" => "nika check flow.nika.yaml",
        "new" => "nika new my-flow --verb infer",
        "workflow" => "nika workflow graph f.nika.yaml",
        "infer" => "nika infer \"hello\"",
        "fetch" => "nika fetch url --extract article",
        "invoke" => "nika invoke nika:thumbnail img.jpg",
        "agent" => "nika agent \"Research AI\"",
        "provider" => "nika provider list",
        "mcp" => "nika mcp list",
        "model" => "nika model list",
        "showcase" => "nika showcase list",
        "init" => "nika init",
        "config" => "nika config list",
        "media" => "nika media stats",
        "doctor" => "nika doctor --fix",
        "cache" => "nika cache stats",
        "job" => "nika job list",
        "daemon" => "nika daemon status",
        "setup" => "nika setup",
        "features" => "nika features",
        "completion" => "nika completion zsh",
        "trace" => "nika trace list",
        "schema" => "nika schema version",
        _ => "",
    }
}

/// Cosmic icon + color for verb commands. Returns None for regular commands.
fn get_icon(name: &str) -> Option<(&'static str, &'static str)> {
    match name {
        "infer" => Some(("\u{2727}", "magenta")),
        "fetch" => Some(("\u{2604}", "cyan")),
        "invoke" => Some(("\u{229B}", "green")),
        "agent" => Some(("\u{274B}", "red")),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MAIN HELP
// ═══════════════════════════════════════════════════════════════════════════

/// Print the full custom help (called from `nika` with no args or `nika help`).
///
/// Sections are derived dynamically from clap's `help_heading` attributes —
/// new commands added to the `Commands` enum appear automatically.
pub fn print_help(app: &clap::Command) {
    print_banner();

    // ── Quick Start ────────────────────────────────────────────────────
    section("QUICK START");
    quick("nika infer \"Explain quantum computing\"", "LLM call");
    quick("nika run workflow.nika.yaml", "Execute workflow");
    quick("nika help verbs", "Learn the 5 verbs");
    sep();

    // ── Command sections (derived from clap headings) ─────────────────
    for &heading in SECTION_ORDER {
        let section_cmds: Vec<&clap::Command> = app
            .get_subcommands()
            .filter(|s| !s.is_hide_set() && s.get_next_help_heading() == Some(heading))
            .collect();
        if section_cmds.is_empty() {
            continue;
        }
        section(heading);
        for sub in section_cmds {
            print_cmd_line(sub);
        }
        sep();
    }

    // ── Deep Dive ────────────────────────────────────────────────────────
    section("DEEP DIVE");
    topic("nika help verbs", "The 5 semantic verbs explained");
    topic("nika help providers", "All 9 providers with status");
    topic("nika help templates", "Template syntax & 52 transforms");
    topic("nika help examples", "Common workflow patterns");
    sep();

    // ── Flags ────────────────────────────────────────────────────────────
    section("FLAGS");
    flag("-v, --verbose", "Increase verbosity (-v, -vv, -vvv)");
    flag("-q, --quiet", "Suppress non-error output");
    flag("--color <MODE>", "auto | always | never");
    flag("--detail <LVL>", "max | default | min | json");
    flag("--no-live", "Classic append-only output");
    println!();

    // ── Footer ───────────────────────────────────────────────────────────
    println!(
        "  {}",
        "https://github.com/supernovae-st/nika".cyan().underline()
    );
    println!();
}

/// Render one command line inside a section.
fn print_cmd_line(sub: &clap::Command) {
    let name = sub.get_name();
    let desc = get_short_desc(name)
        .map(|s| s.to_string())
        .unwrap_or_else(|| truncate_about(sub));
    let alias = sub.get_visible_aliases().next();
    let example = get_example(name);
    if let Some((icon, color)) = get_icon(name) {
        verb_cmd(name, alias, icon, color, &desc, example);
    } else {
        cmd(name, alias, &desc, example);
    }
}

/// Fallback: extract a short description from clap's about text.
fn truncate_about(sub: &clap::Command) -> String {
    let raw = sub.get_about().map(|s| s.to_string()).unwrap_or_default();
    let first = raw.lines().next().unwrap_or("").to_string();
    // Strip parenthetical or em-dash suffixes common in clap about strings
    let trimmed = first
        .split_once(" (")
        .map(|(a, _)| a.to_string())
        .or_else(|| first.split_once(" \u{2014} ").map(|(a, _)| a.to_string()))
        .unwrap_or(first);
    if trimmed.len() > 32 {
        format!("{}…", &trimmed[..31])
    } else {
        trimmed
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TOPIC HELP
// ═══════════════════════════════════════════════════════════════════════════

/// Print deep-dive help for a specific topic.
/// Returns true if topic was found, false otherwise.
pub fn print_topic(topic: &str) -> bool {
    match topic.to_lowercase().as_str() {
        "verbs" | "verb" => {
            topic_verbs();
            true
        }
        "providers" | "provider" => {
            topic_providers();
            true
        }
        "templates" | "template" | "bindings" | "transforms" => {
            topic_templates();
            true
        }
        "examples" | "example" | "patterns" => {
            topic_examples();
            true
        }
        _ => false,
    }
}

fn topic_verbs() {
    print_banner();
    section("THE 5 SEMANTIC VERBS");
    println!();

    verb_topic(
        "\u{2727}",
        "magenta",
        "infer:",
        "LLM text generation",
        &[
            "Sends a prompt to any cloud or local model.",
            "Supports vision (content: array), structured output, extended thinking.",
        ],
        &[
            ("$", "nika infer \"Explain quantum computing\""),
            ("$", "cat file.txt | nika infer \"Summarize\" --stdin"),
            (
                "$",
                "nika infer \"Extract\" --from-example '{\"names\":[\"\"]}'",
            ),
        ],
    );

    verb_topic(
        "\u{2388}",
        "yellow",
        "exec:",
        "Shell command execution",
        &[
            "Runs commands in a subprocess. Use shell: true for pipes/redirects.",
            "Commands validated against security blocklist (NIKA-053).",
        ],
        &[
            ("yaml", "exec: \"npm run build\""),
            (
                "yaml",
                "exec: { command: \"cat data | jq '.items'\", shell: true }",
            ),
        ],
    );

    verb_topic(
        "\u{2604}",
        "cyan",
        "fetch:",
        "HTTP requests with smart extraction",
        &[
            "9 extract modes: markdown, article, text, selector, metadata,",
            "links, jsonpath, feed, llm_txt. Also response: full | binary.",
        ],
        &[
            ("$", "nika fetch https://blog.com --extract article"),
            (
                "$",
                "nika fetch https://api.x.com/data --extract jsonpath --selector \"$.items\"",
            ),
        ],
    );

    verb_topic(
        "\u{229B}",
        "green",
        "invoke:",
        "MCP tool calls & 30+ builtin tools",
        &[
            "30+ builtin tools (nika:*) + any MCP server tool.",
            "Double-colon separator for MCP: server::tool_name",
        ],
        &[
            ("$", "nika invoke nika:dimensions photo.jpg"),
            ("$", "nika invoke --list"),
        ],
    );

    verb_topic(
        "\u{274B}",
        "red",
        "agent:",
        "Multi-turn agentic loops",
        &[
            "AI agent with tool access. Completion: explicit | natural | pattern.",
            "Guardrails: length, schema, regex, llm. Cost/time limits.",
        ],
        &[(
            "$",
            "nika agent \"Research AI workflows\" --tool web_search --turns 5",
        )],
    );
}

fn topic_providers() {
    print_banner();
    section("LLM PROVIDERS (9 total)");
    println!();

    let providers = [
        (
            "anthropic",
            "ANTHROPIC_API_KEY",
            "Claude \u{2014} reasoning, code, recommended",
        ),
        (
            "openai",
            "OPENAI_API_KEY",
            "GPT \u{2014} versatile, large ecosystem",
        ),
        (
            "mistral",
            "MISTRAL_API_KEY",
            "Mistral \u{2014} multilingual, EU sovereign",
        ),
        ("groq", "GROQ_API_KEY", "Groq \u{2014} ultra-fast inference"),
        (
            "deepseek",
            "DEEPSEEK_API_KEY",
            "DeepSeek \u{2014} budget reasoning",
        ),
        (
            "gemini",
            "GEMINI_API_KEY",
            "Gemini \u{2014} large context, multimodal",
        ),
        ("xai", "XAI_API_KEY", "Grok \u{2014} real-time knowledge"),
    ];

    println!("  {}", "CLOUD".bold());
    for (name, env_var, desc) in &providers {
        let has_key = std::env::var(env_var).is_ok_and(|v| !v.trim().is_empty());
        let icon = if has_key {
            "\u{2713}".green().bold().to_string()
        } else {
            "\u{2717}".red().bold().to_string()
        };
        println!(
            "    {} {:<12} {:<42} {}",
            icon,
            name.cyan(),
            desc,
            env_var.dimmed()
        );
    }
    println!();

    println!("  {}", "LOCAL".bold());
    let has_native = cfg!(feature = "native-inference");
    let native_icon = if has_native {
        "\u{2713}".green().bold().to_string()
    } else {
        "\u{2298}".dimmed().to_string()
    };
    println!(
        "    {} {:<12} {:<42} {}",
        native_icon,
        "native".cyan(),
        "GGUF models via mistral.rs",
        if has_native {
            "enabled"
        } else {
            "needs --features native-inference"
        }
        .dimmed()
    );
    println!();

    println!("  {}", "TEST".bold());
    println!(
        "    {} {:<12} {:<42} {}",
        "\u{2713}".green().bold(),
        "mock".cyan(),
        "Deterministic, no API calls",
        "always available".dimmed()
    );
    println!();

    println!(
        "  {} {}",
        "\u{2192}".dimmed(),
        "nika provider list".dimmed()
    );
    println!(
        "  {} {}",
        "\u{2192}".dimmed(),
        "nika keys set <name>".dimmed()
    );
    println!("  {} {}", "\u{2192}".dimmed(), "nika model list".dimmed());
    println!();
}

fn topic_templates() {
    print_banner();
    section("TEMPLATE SYNTAX & TRANSFORMS");
    println!();

    println!("  {}", "BINDINGS".bold());
    binding("with: { alias: $task_id }", "Bind task output");
    binding("with: { temp: $weather.data.temp }", "Path access");
    binding("with: { val: $task.path ?? \"fallback\" }", "Default value");
    binding("with: { key: $env.API_KEY }", "Environment variable");
    binding("{{with.alias}}", "Use in templates");
    binding("{{inputs.param}}", "Workflow inputs");
    binding("{{context.readme}}", "File context");
    println!();

    println!("  {} (52 available)", "PIPE TRANSFORMS".bold());
    transform(
        "String:",
        "upper, lower, trim, trim_start, trim_end, length, to_string",
    );
    transform(
        "Array:",
        "first, last, flatten, reverse, sort, unique, compact, keys, values",
    );
    transform("Numeric:", "to_number, round, abs, ceil, floor");
    transform("Type:", "to_bool, to_json, parse_json, type_of");
    transform(
        "Param:",
        "join(sep), split(sep), default(val), slice(start, end)",
    );
    transform(
        "Query:",
        "pluck(f), where(f, v), pick(f1, f2), omit(f1, f2), sort_by(f), group_by(f), merge, regex(pat)",
    );
    transform("String test:", "starts_with(s), ends_with(s), contains(s)");
    transform(
        "URL:",
        "url_host, url_path, url_without_query, url_normalize",
    );
    transform(
        "Encoding:",
        "base64_encode, base64_decode, content_hash, unique_urls",
    );
    transform("JQ:", "jq(expr) — full jq stdlib via jaq-core");
    transform("System:", "shell (escape for safe interpolation)");
    println!();
    println!(
        "    {}",
        "{{with.items | flatten | unique | join(\", \")}}".cyan()
    );
    println!(
        "    {}",
        "{{with.result | default(\"none\") | upper}}".cyan()
    );
    println!();

    println!("  {}", "NULL SAFETY".bold());
    println!(
        "    {}",
        "19 transforms fail on null. Always guard with default():".dimmed()
    );
    println!(
        "    {}",
        "{{with.result | default(\"none\") | upper}}".cyan()
    );
    println!();
}

fn topic_examples() {
    print_banner();
    section("COMMON PATTERNS");
    println!();

    example(
        "Quick LLM call",
        &["nika infer \"Explain quantum computing\""],
    );
    example(
        "Pipeline from stdin",
        &["cat document.txt | nika infer \"Summarize this\" --stdin"],
    );
    example(
        "Structured output",
        &["nika infer \"List 3 cities\" --from-example '{\"cities\":[\"Paris\"]}'"],
    );
    example(
        "Web scraping",
        &["nika fetch https://blog.com --extract article"],
    );
    example(
        "Workflow execution",
        &["nika run research.nika.yaml -i topic=\"AI safety\""],
    );
    example(
        "Dry run (validate without executing)",
        &["nika run workflow.nika.yaml --dry-run"],
    );
    example(
        "Mock testing (no API keys)",
        &["nika run workflow.nika.yaml --provider mock"],
    );
    example(
        "Multi-turn agent",
        &["nika agent \"Research AI safety\" --tool web_search --turns 10"],
    );
    example(
        "Shell completion setup",
        &[
            "nika completion zsh > ~/.zfunc/_nika",
            "nika completion bash > ~/.local/share/bash-completion/completions/nika",
        ],
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// FORMATTING HELPERS
// ═══════════════════════════════════════════════════════════════════════════

fn section(title: &str) {
    println!("  {}", title.bold().magenta());
}

fn sep() {
    println!("  {}", "\u{2500}".repeat(HELP_WIDTH - 2).dimmed()); // ─
}

fn quick(command: &str, desc: &str) {
    println!("    {} {}  {}", "$".dimmed(), command.cyan(), desc.dimmed());
}

fn cmd(name: &str, alias: Option<&str>, desc: &str, example: &str) {
    let alias_str = match alias {
        Some(a) => format!("({})", a),
        None => String::new(),
    };
    println!(
        "    {:<10} {:<8} {:<32} {}",
        name.green(),
        alias_str.dimmed(),
        desc,
        example.dimmed()
    );
}

fn verb_cmd(name: &str, alias: Option<&str>, icon: &str, color: &str, desc: &str, example: &str) {
    let alias_str = match alias {
        Some(a) => format!("({})", a),
        None => String::new(),
    };
    let colored_icon = match color {
        "magenta" => icon.magenta().bold().to_string(),
        "yellow" => icon.yellow().bold().to_string(),
        "cyan" => icon.cyan().bold().to_string(),
        "green" => icon.green().bold().to_string(),
        "red" => icon.red().bold().to_string(),
        _ => icon.white().bold().to_string(),
    };
    println!(
        "    {:<10} {:<6} {} {:<30} {}",
        name.green(),
        alias_str.dimmed(),
        colored_icon,
        desc,
        example.dimmed()
    );
}

fn verb_topic(
    icon: &str,
    color: &str,
    name: &str,
    title: &str,
    desc_lines: &[&str],
    examples: &[(&str, &str)],
) {
    let colored_icon = match color {
        "magenta" => icon.magenta().bold().to_string(),
        "yellow" => icon.yellow().bold().to_string(),
        "cyan" => icon.cyan().bold().to_string(),
        "green" => icon.green().bold().to_string(),
        "red" => icon.red().bold().to_string(),
        _ => icon.white().bold().to_string(),
    };
    println!("  {} {}  {title}", colored_icon, name.green().bold());
    for line in desc_lines {
        println!("     {}", line.dimmed());
    }
    println!();
    for (prefix, cmd) in examples {
        println!("     {}  {}", prefix.dimmed(), cmd.cyan());
    }
    println!();
}

fn topic(command: &str, desc: &str) {
    println!("    {:<34} {}", command.cyan(), desc.dimmed());
}

fn flag(name: &str, desc: &str) {
    println!("    {:<20} {}", name.green(), desc);
}

fn binding(syntax: &str, desc: &str) {
    println!("    {:<46} {}", syntax.cyan(), desc.dimmed());
}

fn transform(category: &str, items: &str) {
    println!("    {} {items}", category.yellow());
}

fn example(title: &str, commands: &[&str]) {
    println!("  {}:", title.bold());
    for cmd in commands {
        println!("    {}  {}", "$".dimmed(), cmd.cyan());
    }
    println!();
}

// ═══════════════════════════════════════════════════════════════════════════
// COSMIC EASTER EGG
// ═══════════════════════════════════════════════════════════════════════════

/// Hidden easter egg: `nika cosmic`
pub fn print_cosmic() {
    println!();
    println!("  {}", "\u{2500}".repeat(50).dimmed());
    println!();
    println!("         {}", "T H E   N I K A   E N G I N E".bold());
    println!();
    println!(
        "    {}  {}",
        "\u{2727}".magenta().bold(),
        "infer    \u{2014}  speak to the stars".magenta()
    );
    println!(
        "    {}  {}",
        "\u{2388}".yellow().bold(),
        "exec     \u{2014}  command the void".yellow()
    );
    println!(
        "    {}  {}",
        "\u{2604}".cyan().bold(),
        "fetch    \u{2014}  catch falling light".cyan()
    );
    println!(
        "    {}  {}",
        "\u{229B}".green().bold(),
        "invoke   \u{2014}  summon the tools".green()
    );
    println!(
        "    {}  {}",
        "\u{274B}".red().bold(),
        "agent    \u{2014}  free will unbound".red()
    );
    println!();
    println!("    {}", "In YAML, we write our destiny.".dimmed());
    println!("    {}", "In DAGs, we find our truth.".dimmed());
    println!("    {}", "In workflows, we build tomorrow.".dimmed());
    println!();
    println!("    {}", "Now go forth, captain. The cosmos awaits.".bold());
    println!();
    println!("  {}", "\u{2500}".repeat(50).dimmed());
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_help_doesnt_panic() {
        // Minimal app — sections will be empty, but banner/static content renders.
        let app = clap::Command::new("nika-test");
        print_help(&app);
    }

    #[test]
    fn print_topic_verbs() {
        assert!(print_topic("verbs"));
        assert!(print_topic("verb"));
    }

    #[test]
    fn print_topic_providers() {
        assert!(print_topic("providers"));
        assert!(print_topic("provider"));
    }

    #[test]
    fn print_topic_templates() {
        assert!(print_topic("templates"));
        assert!(print_topic("template"));
        assert!(print_topic("bindings"));
        assert!(print_topic("transforms"));
    }

    #[test]
    fn print_topic_examples() {
        assert!(print_topic("examples"));
        assert!(print_topic("example"));
        assert!(print_topic("patterns"));
    }

    #[test]
    fn print_topic_unknown_returns_false() {
        assert!(!print_topic("nonexistent"));
        assert!(!print_topic(""));
    }

    #[test]
    fn section_order_has_expected_sections() {
        assert!(SECTION_ORDER.contains(&"WORKFLOWS"));
        assert!(SECTION_ORDER.contains(&"5 VERBS"));
        assert!(SECTION_ORDER.contains(&"SYSTEM"));
        assert!(!SECTION_ORDER.contains(&"HELP")); // meta — excluded
    }

    #[test]
    fn get_short_desc_known_commands() {
        assert!(get_short_desc("run").is_some());
        assert!(get_short_desc("infer").is_some());
        assert!(get_short_desc("schema").is_some());
    }

    #[test]
    fn get_example_known_commands() {
        assert!(!get_example("run").is_empty());
        assert!(!get_example("infer").is_empty());
        assert!(get_example("unknown-cmd").is_empty());
    }

    #[test]
    fn get_icon_verb_commands() {
        assert!(get_icon("infer").is_some());
        assert!(get_icon("fetch").is_some());
        assert!(get_icon("invoke").is_some());
        assert!(get_icon("agent").is_some());
        assert!(get_icon("run").is_none()); // regular command
    }
}
