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
    let inner = 56;
    let bar = "\u{2500}".repeat(inner + 2);

    let l1_left = "\u{2727} N I K A";
    let l1_right = format!("v{version}");
    let l1_pad = inner - l1_left.len() - l1_right.len();

    let l2 = "Semantic YAML workflow engine for AI tasks";
    let l2_pad = inner - l2.len();

    let l3 = "5 verbs \u{00B7} 9 providers \u{00B7} 24 builtin tools";
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
// MAIN HELP
// ═══════════════════════════════════════════════════════════════════════════

/// Print the full custom help (called from `nika` with no args or `nika help`).
pub fn print_help() {
    print_banner();

    // ── Quick Start ────────────────────────────────────────────────────
    section("QUICK START");
    quick("nika infer \"Explain quantum computing\"", "LLM call");
    quick("nika run workflow.nika.yaml", "Execute workflow");
    quick("nika help verbs", "Learn the 5 verbs");
    sep();

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
        "nika workflow graph f.nika.yaml",
    );
    sep();

    // ── 5 Verbs ──────────────────────────────────────────────────────────
    section("5 VERBS \u{2014} Direct Use");
    verb_cmd(
        "infer",
        Some("i"),
        "\u{2727}",
        "magenta",
        "Call an LLM directly",
        "nika infer \"hello\"",
    );
    verb_cmd(
        "exec",
        None,
        "\u{2388}",
        "yellow",
        "Shell command (workflows)",
        "exec: \"npm build\"",
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
        "Builtin or MCP tool",
        "nika invoke nika:thumbnail img.jpg",
    );
    verb_cmd(
        "agent",
        Some("a"),
        "\u{274B}",
        "red",
        "Multi-turn AI agent",
        "nika agent \"Research AI\"",
    );
    sep();

    // ── Interactive ──────────────────────────────────────────────────────
    #[cfg(feature = "tui")]
    {
        section("INTERACTIVE");
        cmd("ui", None, "Launch TUI", "nika ui");
        cmd("chat", Some("c"), "Chat mode", "nika chat");
        cmd("studio", Some("s"), "Studio editor", "nika studio");
        sep();
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
    sep();

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
    sep();

    // ── Project ──────────────────────────────────────────────────────────
    section("PROJECT");
    cmd("init", None, "Initialize .nika/ project", "nika init");
    cmd("config", None, "Manage configuration", "nika config list");
    cmd("pkg", Some("p"), "Package management", "nika pkg list");
    cmd("media", None, "Media store management", "nika media stats");
    sep();

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
    sep();

    // ── Deep Dive ────────────────────────────────────────────────────────
    section("DEEP DIVE");
    topic("nika help verbs", "The 5 semantic verbs explained");
    topic("nika help providers", "All 9 providers with status");
    topic("nika help templates", "Template syntax & 31 transforms");
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
        "MCP tool calls & 24 builtin tools",
        &[
            "24 builtin tools (nika:*) + any MCP server tool.",
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
        "join(\", \"), split(\",\"), default(\"fallback\")",
    );
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

fn sep() {
    println!("  {}", "\u{2500}".repeat(60).dimmed()); // ─
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_help_doesnt_panic() {
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
