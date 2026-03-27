//! Custom help system — beautiful grouped CLI help with cosmic theme.
//!
//! Replaces clap's flat command list with categorised, colored output.
//! Supports deep-dive topics via `nika help <topic>`.

use colored::Colorize;

// ═══════════════════════════════════════════════════════════════════════════
// BANNER
// ═══════════════════════════════════════════════════════════════════════════

fn print_banner() {
    let version = env!("CARGO_PKG_VERSION");
    println!();
    println!(
        "  {} {}                                          {}",
        "\u{2727}".magenta().bold(), // ✧
        "N I K A".bold(),
        format!("v{version}").dimmed()
    );
    println!(
        "  {}",
        "Semantic YAML workflow engine for AI tasks".dimmed()
    );
    println!("  {}", "\u{2501}".repeat(58).dimmed()); // ━
    println!();
}

// ═══════════════════════════════════════════════════════════════════════════
// MAIN HELP
// ═══════════════════════════════════════════════════════════════════════════

/// Print the full custom help (called from `nika` with no args or `nika help`).
pub fn print_help() {
    print_banner();

    // ── Workflows ────────────────────────────────────────────────────────
    section("WORKFLOWS");
    cmd(
        "run",
        Some("r"),
        "Run a workflow file",
        "nika run flow.nika.yaml",
    );
    cmd(
        "check",
        Some("v"),
        "Validate syntax & DAG",
        "nika check flow.nika.yaml",
    );
    cmd(
        "new",
        Some("n"),
        "Create a new workflow",
        "nika new my-flow --verb infer",
    );
    cmd(
        "workflow",
        Some("w"),
        "Edit, graph, add-task",
        "nika workflow graph flow.nika.yaml",
    );
    println!();

    // ── 5 Verbs ──────────────────────────────────────────────────────────
    section("5 VERBS \u{2014} Direct Use");
    verb_cmd(
        "infer",
        Some("i"),
        "\u{2727}",
        "magenta",
        "Call an LLM directly",
        "nika infer \"Explain AI\"",
    );
    verb_cmd(
        "fetch",
        Some("f"),
        "\u{2604}",
        "cyan",
        "Fetch URL with extraction",
        "nika fetch url --extract article",
    );
    verb_cmd(
        "invoke",
        None,
        "\u{229B}",
        "green",
        "Call builtin or MCP tool",
        "nika invoke nika:thumbnail img.jpg",
    );
    verb_cmd(
        "agent",
        Some("a"),
        "\u{274B}",
        "red",
        "Multi-turn AI agent",
        "nika agent \"Research AI\" --turns 5",
    );
    println!();

    // ── Interactive ──────────────────────────────────────────────────────
    #[cfg(feature = "tui")]
    {
        section("INTERACTIVE");
        cmd("ui", None, "Launch TUI", "nika ui");
        cmd("chat", Some("c"), "Chat mode", "nika chat");
        cmd("studio", Some("s"), "Studio editor", "nika studio");
        println!();
    }

    // ── Models & Providers ───────────────────────────────────────────────
    section("MODELS & PROVIDERS");
    cmd(
        "model",
        Some("m"),
        "LLM models (cloud + local)",
        "nika model list",
    );
    cmd("provider", None, "Manage API keys", "nika provider list");
    cmd("mcp", None, "MCP server connections", "nika mcp list");
    println!();

    // ── Learning ─────────────────────────────────────────────────────────
    section("LEARNING");
    cmd(
        "course",
        Some("learn"),
        "Interactive 12-level course",
        "nika course status",
    );
    cmd(
        "showcase",
        None,
        "Browse 115+ showcase workflows",
        "nika showcase list",
    );
    println!();

    // ── Project ──────────────────────────────────────────────────────────
    section("PROJECT");
    cmd("init", None, "Initialize .nika/ project", "nika init");
    cmd("config", None, "Manage configuration", "nika config list");
    cmd("pkg", Some("p"), "Package management", "nika pkg list");
    cmd("media", None, "Media store management", "nika media stats");
    println!();

    // ── System ───────────────────────────────────────────────────────────
    section("SYSTEM");
    cmd(
        "doctor",
        Some("d"),
        "System health check",
        "nika doctor --fix",
    );
    #[cfg(unix)]
    {
        cmd("daemon", None, "Background daemon", "nika daemon status");
        cmd("cache", None, "LLM response cache", "nika cache stats");
        cmd("job", None, "Background jobs", "nika job list");
    }
    cmd("setup", None, "API key setup wizard", "nika setup");
    cmd("features", None, "Compiled feature flags", "nika features");
    cmd(
        "completion",
        None,
        "Shell completions",
        "nika completion zsh",
    );
    cmd("trace", None, "Execution traces", "nika trace list");
    println!();

    // ── Deep Dive ────────────────────────────────────────────────────────
    section("DEEP DIVE");
    topic("nika help verbs", "The 5 semantic verbs explained");
    topic("nika help providers", "All 9 providers with status");
    topic("nika help templates", "Template syntax & 31 transforms");
    topic("nika help examples", "Common workflow patterns");
    println!();

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

    // infer
    println!(
        "  {} {}  LLM text generation",
        "\u{2727}".magenta().bold(),
        "infer:".green().bold()
    );
    println!(
        "  {}",
        "  Sends a prompt to any cloud or local model.".dimmed()
    );
    println!(
        "  {}",
        "  Supports vision (content: array), structured output, extended thinking.".dimmed()
    );
    println!();
    println!(
        "  {}  {}",
        "$".dimmed(),
        "nika infer \"Explain quantum computing\"".cyan()
    );
    println!(
        "  {}  {}",
        "$".dimmed(),
        "cat file.txt | nika infer \"Summarize\" --stdin".cyan()
    );
    println!(
        "  {}  {}",
        "$".dimmed(),
        "nika infer \"Extract names\" --from-example '{\"names\":[\"\"]}' ".cyan()
    );
    println!();

    // exec
    println!(
        "  {} {}  Shell command execution",
        "\u{2388}".yellow().bold(),
        "exec:".green().bold()
    );
    println!(
        "  {}",
        "  Runs commands in a subprocess. Use shell: true for pipes/redirects.".dimmed()
    );
    println!(
        "  {}",
        "  Commands validated against security blocklist (NIKA-053).".dimmed()
    );
    println!();
    println!(
        "  {}  {}",
        "yaml".dimmed(),
        "exec: \"npm run build\"".cyan()
    );
    println!(
        "  {}  {}",
        "yaml".dimmed(),
        "exec: { command: \"cat data | jq '.items'\", shell: true }".cyan()
    );
    println!();

    // fetch
    println!(
        "  {} {}  HTTP requests with smart extraction",
        "\u{2604}".cyan().bold(),
        "fetch:".green().bold()
    );
    println!(
        "  {}",
        "  9 extract modes: markdown, article, text, selector, metadata,".dimmed()
    );
    println!(
        "  {}",
        "  links, jsonpath, feed, llm_txt. Also response: full | binary.".dimmed()
    );
    println!();
    println!(
        "  {}  {}",
        "$".dimmed(),
        "nika fetch https://blog.com --extract article".cyan()
    );
    println!(
        "  {}  {}",
        "$".dimmed(),
        "nika fetch https://api.x.com/data --extract jsonpath --selector \"$.items\"".cyan()
    );
    println!();

    // invoke
    println!(
        "  {} {}  MCP tool calls & 24 builtin tools",
        "\u{229B}".green().bold(),
        "invoke:".green().bold()
    );
    println!(
        "  {}",
        "  24 builtin tools (nika:*) + any MCP server tool.".dimmed()
    );
    println!(
        "  {}",
        "  Double-colon separator for MCP: server::tool_name".dimmed()
    );
    println!();
    println!(
        "  {}  {}",
        "$".dimmed(),
        "nika invoke nika:dimensions photo.jpg".cyan()
    );
    println!("  {}  {}", "$".dimmed(), "nika invoke --list".cyan());
    println!();

    // agent
    println!(
        "  {} {}  Multi-turn agentic loops",
        "\u{274B}".red().bold(),
        "agent:".green().bold()
    );
    println!(
        "  {}",
        "  AI agent with tool access. Completion: explicit | natural | pattern.".dimmed()
    );
    println!(
        "  {}",
        "  Guardrails: length, schema, regex, llm. Cost/time limits.".dimmed()
    );
    println!();
    println!(
        "  {}  {}",
        "$".dimmed(),
        "nika agent \"Research AI workflows\" --tool web_search --turns 5".cyan()
    );
    println!();
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
            "    {} {:<12} {:<40} {}",
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
        "\u{2298}".dimmed().to_string() // ⊘
    };
    println!(
        "    {} {:<12} {:<40} {}",
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
        "    {} {:<12} {:<40} {}",
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
        "nika provider set <name>".dimmed()
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

    println!("  {} (31 available)", "PIPE TRANSFORMS".bold());
    println!(
        "    {} upper, lower, trim, trim_start, trim_end, length, to_string",
        "String:".yellow()
    );
    println!(
        "    {} first, last, flatten, reverse, sort, unique, compact, keys, values",
        "Array:".yellow()
    );
    println!(
        "    {} to_number, round, abs, ceil, floor",
        "Numeric:".yellow()
    );
    println!(
        "    {} to_bool, to_json, parse_json, type_of",
        "Type:".yellow()
    );
    println!(
        "    {} join(\", \"), split(\",\"), default(\"fallback\")",
        "Param:".yellow()
    );
    println!(
        "    {} shell (escape for safe interpolation)",
        "System:".yellow()
    );
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
    example(
        "Learning course",
        &[
            "nika init --course",
            "cd nika-course && nika course status",
            "nika course next",
        ],
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// FORMATTING HELPERS
// ═══════════════════════════════════════════════════════════════════════════

fn section(title: &str) {
    println!("  {}", title.bold().magenta());
}

fn cmd(name: &str, alias: Option<&str>, desc: &str, example: &str) {
    let alias_str = match alias {
        Some(a) => format!("({})", a),
        None => String::new(),
    };
    println!(
        "    {:<10} {:<8} {:<34} {}",
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
        "    {:<10} {:<6} {} {:<32} {}",
        name.green(),
        alias_str.dimmed(),
        colored_icon,
        desc,
        example.dimmed()
    );
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

fn example(title: &str, commands: &[&str]) {
    println!("  {}:", title.bold());
    for cmd in commands {
        println!("    {}  {}", "$".dimmed(), cmd.cyan());
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_help_doesnt_panic() {
        // Just verify it doesn't crash
        print_help();
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
}
