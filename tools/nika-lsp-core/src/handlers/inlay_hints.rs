//! Inlay hints handler — inline annotations.
//!
//! Protocol-agnostic: returns `Vec<InlayHintEntry>`.
//! Covers: timeout, bindings, depends_on count, max_turns,
//! concurrency, and model cost annotations.

/// Protocol-agnostic inlay hint.
#[derive(Debug, Clone)]
pub struct InlayHintEntry {
    /// Line number (0-based).
    pub line: u32,
    /// Character offset (end of line for suffix hints).
    pub character: u32,
    /// Hint text to display.
    pub label: String,
    /// Tooltip on hover.
    pub tooltip: String,
    /// Hint kind.
    pub kind: HintKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintKind {
    Type,
    Parameter,
}

/// Compute inlay hints for the byte range [start_offset, end_offset).
pub fn inlay_hints(text: &str, start_offset: u32, end_offset: u32) -> Vec<InlayHintEntry> {
    let _ = (start_offset, end_offset); // range filtering reserved for future use
    let mut hints = Vec::new();

    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }

        let line_char_len = line.chars().map(|c| c.len_utf16()).sum::<usize>() as u32;

        // 1. timeout: N -> "N seconds" / "(Nm Ns)"
        if let Some(rest) = trimmed.strip_prefix("timeout:") {
            if let Ok(secs) = rest.trim().parse::<u64>() {
                let label = if secs == 1 {
                    " second".into()
                } else if secs < 60 {
                    " seconds".into()
                } else {
                    let (m, s) = (secs / 60, secs % 60);
                    if s == 0 {
                        format!(" ({}min)", m)
                    } else {
                        format!(" ({}m{}s)", m, s)
                    }
                };
                hints.push(InlayHintEntry {
                    line: i as u32,
                    character: line_char_len,
                    label,
                    tooltip: "Nika timeout is always in seconds".into(),
                    kind: HintKind::Type,
                });
            }
        }

        // 2. alias: $task_ref -> "<- task_ref output"
        if trimmed.contains(": $") && !trimmed.starts_with('-') {
            if let Some(dp) = trimmed.find(": $") {
                let task_ref: String = trimmed[dp + 3..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                    .collect();
                if !task_ref.is_empty() {
                    hints.push(InlayHintEntry {
                        line: i as u32,
                        character: line_char_len,
                        label: format!(" <- {} output", task_ref),
                        tooltip: format!("Binds the output of task '{}'", task_ref),
                        kind: HintKind::Type,
                    });
                }
            }
        }

        // 3. depends_on: [a, b, c] -> "(3 deps)"
        if let Some(rest) = trimmed.strip_prefix("depends_on:") {
            let deps_str = rest.trim();
            if deps_str.starts_with('[') {
                let count = deps_str
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .count();
                if count > 0 {
                    hints.push(InlayHintEntry {
                        line: i as u32,
                        character: line_char_len,
                        label: format!(" ({} dep{})", count, if count == 1 { "" } else { "s" }),
                        tooltip: "Number of upstream dependencies".into(),
                        kind: HintKind::Type,
                    });
                }
            }
        }

        // 4. max_turns: N -> "iterations"
        if let Some(rest) = trimmed.strip_prefix("max_turns:") {
            if rest.trim().parse::<u64>().is_ok() {
                hints.push(InlayHintEntry {
                    line: i as u32,
                    character: line_char_len,
                    label: " iterations".into(),
                    tooltip: "Maximum agent loop iterations".into(),
                    kind: HintKind::Type,
                });
            }
        }

        // 5. concurrency: N -> "parallel"
        if let Some(rest) = trimmed.strip_prefix("concurrency:") {
            if rest.trim().parse::<u64>().is_ok() {
                hints.push(InlayHintEntry {
                    line: i as u32,
                    character: line_char_len,
                    label: " parallel".into(),
                    tooltip: "Max parallel for_each iterations".into(),
                    kind: HintKind::Type,
                });
            }
        }

        // 6. model: <name> -> "(Provider . $X/$Y per 1M)"
        if let Some(rest) = trimmed.strip_prefix("model:") {
            let model = rest.trim().trim_matches('"').trim_matches('\'');
            if let Some(cost_label) = model_cost_label(model) {
                hints.push(InlayHintEntry {
                    line: i as u32,
                    character: line_char_len,
                    label: cost_label,
                    tooltip: "Cost per million tokens (input/output)".into(),
                    kind: HintKind::Type,
                });
            }
        }
    }

    hints
}

/// Get a cost label for a known model.
///
/// Returns None for unknown models. Format: " (Provider . $in/$out per 1M)"
fn model_cost_label(model: &str) -> Option<String> {
    // Static table -- keep in sync with nika-engine/src/provider/cost.rs
    let (provider, input, output) = match model {
        // Anthropic
        m if m.contains("opus-4") => ("Anthropic", 15.0, 75.0),
        m if m.contains("sonnet-4") => ("Anthropic", 3.0, 15.0),
        m if m.contains("haiku-3.5") => ("Anthropic", 0.8, 4.0),
        // OpenAI
        m if m.starts_with("gpt-4o") && !m.contains("mini") => ("OpenAI", 2.5, 10.0),
        "gpt-4o-mini" => ("OpenAI", 0.15, 0.6),
        m if m.starts_with("gpt-4.1") && !m.contains("mini") && !m.contains("nano") => {
            ("OpenAI", 2.0, 8.0)
        }
        m if m.starts_with("gpt-4.1-mini") => ("OpenAI", 0.4, 1.6),
        m if m.starts_with("gpt-4.1-nano") => ("OpenAI", 0.1, 0.4),
        "o1" => ("OpenAI", 15.0, 60.0),
        "o3-mini" => ("OpenAI", 1.1, 4.4),
        // Mistral
        "mistral-large-latest" | "mistral-large" => ("Mistral", 2.0, 6.0),
        "mistral-small-latest" | "mistral-small" => ("Mistral", 0.2, 0.6),
        // Groq
        m if m.contains("llama") && m.contains("70b") => ("Groq", 0.59, 0.79),
        m if m.contains("llama") && m.contains("8b") => ("Groq", 0.05, 0.08),
        // DeepSeek
        "deepseek-chat" => ("DeepSeek", 0.14, 0.28),
        "deepseek-reasoner" => ("DeepSeek", 0.55, 2.19),
        // Gemini
        m if m.contains("gemini") && m.contains("flash") => ("Gemini", 0.1, 0.4),
        m if m.contains("gemini") && m.contains("pro") => ("Gemini", 1.25, 5.0),
        // xAI
        "grok-3" => ("xAI", 3.0, 15.0),
        "grok-3-mini" => ("xAI", 0.3, 0.5),
        _ => return None,
    };

    Some(format!(
        " ({} \u{00b7} ${}/{})",
        provider,
        format_price(input),
        format_price(output)
    ))
}

fn format_price(price: f64) -> String {
    if price >= 1.0 {
        format!("{}", price)
    } else {
        format!("{:.2}", price)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_claude_sonnet() {
        let label = model_cost_label("claude-sonnet-4-20250514");
        assert!(label.is_some());
        assert!(label.unwrap().contains("Anthropic"));
    }

    #[test]
    fn cost_gpt4o() {
        let label = model_cost_label("gpt-4o");
        assert!(label.is_some());
        assert!(label.unwrap().contains("OpenAI"));
    }

    #[test]
    fn cost_unknown() {
        assert!(model_cost_label("some-random-model").is_none());
    }
}
