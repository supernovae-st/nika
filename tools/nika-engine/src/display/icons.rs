//! Cosmic icon palette — Unicode icons for verbs, status, and subsystems.
//!
//! Design: Every icon is Narrow East Asian Width (eaw=N) to avoid
//! alignment issues across terminals. No Ambiguous (A) characters.

use colored::{ColoredString, Colorize};

// ── Verb Icons ──────────────────────────────────────────────────────────

/// Return a colored Cosmic icon for the given verb name.
pub fn verb(v: &str) -> ColoredString {
    match v {
        "infer" => "\u{2727}".magenta(), // ✧ four-pointed star
        "exec" => "\u{2388}".yellow(),   // ⎈ helm
        "fetch" => "\u{2604}".cyan(),    // ☄ comet
        "invoke" => "\u{229B}".green(),  // ⊛ circled asterisk
        "agent" => "\u{274B}".red(),     // ❋ propeller
        _ => "\u{25CF}".white(),         // ● fallback
    }
}

/// Return the plain (uncolored) Cosmic icon for the given verb name.
pub(crate) fn verb_plain(v: &str) -> &'static str {
    match v {
        "infer" => "\u{2727}",  // ✧
        "exec" => "\u{2388}",   // ⎈
        "fetch" => "\u{2604}",  // ☄
        "invoke" => "\u{229B}", // ⊛
        "agent" => "\u{274B}",  // ❋
        _ => "\u{25CF}",        // ●
    }
}

// ── Status Icons ────────────────────────────────────────────────────────

/// Pending task: hollow circle.
pub fn pending() -> ColoredString {
    "\u{25CB}".dimmed() // ○
}

/// Running task: filled circle, bold white.
pub fn running() -> ColoredString {
    "\u{25CF}".white().bold() // ●
}

/// Successful task: check mark, bold green.
pub fn success() -> ColoredString {
    "\u{2713}".green().bold() // ✓
}

/// Failed task: ballot X, bold red.
pub fn failed() -> ColoredString {
    "\u{2717}".red().bold() // ✗
}

/// Skipped task: circled division slash, dimmed.
pub fn skipped() -> ColoredString {
    "\u{2298}".dimmed() // ⊘
}

// ── Subsystem Icons (all Narrow eaw=N) ──────────────────────────────────

/// Provider subsystem: bowtie.
pub fn provider() -> ColoredString {
    "\u{22C8}".blue() // ⋈
}

/// MCP subsystem: squared plus.
pub fn mcp() -> ColoredString {
    "\u{229E}".green() // ⊞
}

/// Guardrail subsystem: squared times.
pub fn guardrail() -> ColoredString {
    "\u{22A0}".yellow() // ⊠
}

/// Artifact subsystem: circled ring.
pub fn artifact() -> ColoredString {
    "\u{229A}".cyan() // ⊚
}

/// Media subsystem: squared dot.
pub fn media() -> ColoredString {
    "\u{22A1}".magenta() // ⊡
}

/// Structured output subsystem: hexagon.
pub fn structured() -> ColoredString {
    "\u{2B21}".blue() // ⬡
}

/// Vision subsystem: diamond dot.
pub fn vision() -> ColoredString {
    "\u{27D0}".purple() // ⟐
}

/// HTTP subsystem: bidirectional arrows.
pub fn http() -> ColoredString {
    "\u{21C4}".cyan() // ⇄
}

/// Retry subsystem: zigzag arrow.
pub fn retry() -> ColoredString {
    "\u{21AF}".yellow() // ↯
}

/// Agent metadata: circled times.
pub(crate) fn agent_meta() -> ColoredString {
    "\u{2297}".red() // ⊗
}

/// Log entry: small square.
pub fn log() -> ColoredString {
    "\u{25AA}".dimmed() // ▪
}
