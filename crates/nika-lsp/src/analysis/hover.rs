// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Hover documentation for the locked language vocabulary.
//!
//! The token under the cursor is matched against the [`vocab`]
//! tables — the 4 verbs, the top-level envelope keys, and the task-field
//! keys. A match yields a markdown [`Hover`] whose range is the token (so
//! the client highlights it), titled with the keyword and its category and
//! bodied with the one-line doc. A token that is not language vocabulary
//! (a task id, a string value, a provider) yields no hover — v0.1 documents
//! the LOCKED language surface only.
//!
//! Pure: `(text, offset) -> Option<Hover>`.

use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Range};

use super::position::LineIndex;
use super::vocab::{self, Entry};

/// Compute hover for the token under `offset` — a language-vocabulary token
/// (verb / envelope / task-field key) OR a task REFERENCE (`depends_on:` item
/// or `${{ tasks.X }}`) showing the target task's verb.
#[must_use]
pub fn hover(text: &str, offset: usize) -> Option<Hover> {
    value_card_hover(text, offset)
        .or_else(|| vocab_hover(text, offset))
        .or_else(|| task_ref_hover(text, offset))
}

/// Hover on a `tool:` or `model:` VALUE — the catalog card for that
/// builtin (category · args · required) or that model (context/output
/// windows). Line-based: the value spans past `word_at`'s identifier
/// alphabet (`nika:jq` · `ollama/qwen3.5:4b`), so the span is the
/// scalar after the key, quotes and comment shed.
fn value_card_hover(text: &str, offset: usize) -> Option<Hover> {
    let (key, value, start, end) = keyed_value_at(text, offset)?;
    let body = match key {
        "tool" => builtin_card(value)?,
        "model" => model_card(value)?,
        _ => return None,
    };
    let index = LineIndex::new(text);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: body,
        }),
        range: Some(Range::new(index.position(start), index.position(end))),
    })
}

/// The `key` and value span of the cursor's line, when the cursor sits
/// INSIDE the value of a `key: value` line.
fn keyed_value_at(text: &str, offset: usize) -> Option<(&str, &str, usize, usize)> {
    let line_start = text.get(..offset)?.rfind('\n').map_or(0, |i| i + 1);
    let line_end = text
        .get(offset..)
        .and_then(|rest| rest.find('\n'))
        .map_or(text.len(), |i| offset + i);
    let line = text.get(line_start..line_end)?;
    let colon = line.find(':')?;
    let key = line.get(..colon)?.trim_start().trim_start_matches("- ");
    let after = line.get(colon + 1..)?;
    let shed = after.split('#').next().unwrap_or("");
    let value = shed.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return None;
    }
    let value_start = line_start + colon + 1 + shed.len() - shed.trim_start().len()
        + usize::from(shed.trim_start().starts_with('"') || shed.trim_start().starts_with('\''));
    let value_end = value_start + value.len();
    (offset >= value_start && offset <= value_end).then_some((key, value, value_start, value_end))
}

/// The catalog card for a `nika:*` builtin.
fn builtin_card(value: &str) -> Option<String> {
    let short = value.strip_prefix("nika:")?;
    let b = nika_catalog::all_builtins()
        .iter()
        .find(|b| b.name == short)?;
    let args = if b.args.is_empty() {
        "none".to_owned()
    } else {
        b.args
            .iter()
            .map(|a| format!("`{a}`"))
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let mut body = format!(
        "**`nika:{}`** — _{:?} builtin_\n\nargs: {args}",
        b.name, b.category
    );
    if !b.required.is_empty() {
        use std::fmt::Write as _;
        let _ = write!(
            body,
            "\nrequired: {}",
            b.required
                .iter()
                .map(|a| format!("`{a}`"))
                .collect::<Vec<_>>()
                .join(" · ")
        );
    }
    Some(body)
}

/// The catalog card for a `provider/model` address.
fn model_card(value: &str) -> Option<String> {
    let (prov, model) = value.split_once('/')?;
    let provider = nika_catalog::all_providers()
        .iter()
        .find(|p| p.id == prov)?;
    let m = provider.models.iter().find(|m| m.model == model)?;
    Some(format!(
        "**`{prov}/{model}`** — _catalog model_\n\ncontext {}k tokens · output {}k tokens",
        m.context_window_tokens / 1000,
        m.max_output_tokens / 1000
    ))
}

/// Hover for a language-vocabulary token (a verb or an envelope/task key).
fn vocab_hover(text: &str, offset: usize) -> Option<Hover> {
    let (word, start, end) = word_at(text, offset)?;
    let (entry, category) = resolve(word)?;
    let index = LineIndex::new(text);
    let range = Range::new(index.position(start), index.position(end));
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: render(entry, category),
        }),
        range: Some(range),
    })
}

/// Hover for a task REFERENCE (a `depends_on:` item or a `${{ tasks.X }}`
/// ref) → the target task's id + verb, so the cursor shows what it points at
/// without leaving the line. Reuses the go-to-definition resolver.
fn task_ref_hover(text: &str, offset: usize) -> Option<Hover> {
    let (id, verb) = super::definition::referenced_task_at(text, offset)?;
    let range = word_at(text, offset).map(|(_, start, end)| {
        let index = LineIndex::new(text);
        Range::new(index.position(start), index.position(end))
    });
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "**task `{id}`** — _{verb}_\n\nReferenced task · go-to-definition jumps to its `id`."
            ),
        }),
        range,
    })
}

/// Resolve a word to its vocabulary entry + category label. Verbs win over
/// keys (a verb name is never also a top-level/task key).
fn resolve(word: &str) -> Option<(Entry, &'static str)> {
    if let Some(e) = vocab::lookup(vocab::VERBS, word) {
        return Some((e, "verb"));
    }
    if let Some(e) = vocab::lookup(vocab::TOP_LEVEL_KEYS, word) {
        return Some((e, "top-level key"));
    }
    if let Some(e) = vocab::lookup(vocab::TASK_FIELD_KEYS, word) {
        return Some((e, "task field"));
    }
    None
}

/// Render the markdown hover body.
fn render(entry: Entry, category: &str) -> String {
    format!("**`{}`** — _{}_\n\n{}", entry.name, category, entry.doc)
}

/// The identifier word at `offset` and its `[start, end)` byte range.
/// `None` when the cursor is not on an identifier char.
fn word_at(text: &str, offset: usize) -> Option<(&str, usize, usize)> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return None;
    }
    // Clamp into range; if the cursor sits just past an identifier (the
    // common « end of word » caret), step back one so the word is found.
    let probe = if offset >= len { len - 1 } else { offset };
    let probe = if !is_ident_byte(bytes.get(probe).copied())
        && probe > 0
        && is_ident_byte(bytes.get(probe - 1).copied())
    {
        probe - 1
    } else {
        probe
    };
    if !is_ident_byte(bytes.get(probe).copied()) {
        return None;
    }
    let mut start = probe;
    while start > 0 && is_ident_byte(bytes.get(start - 1).copied()) {
        start -= 1;
    }
    let mut end = probe + 1;
    while end < len && is_ident_byte(bytes.get(end).copied()) {
        end += 1;
    }
    text.get(start..end).map(|w| (w, start, end))
}

/// Whether a byte is part of an identifier (`[A-Za-z0-9_]`).
fn is_ident_byte(b: Option<u8>) -> bool {
    matches!(b, Some(c) if c == b'_' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(h: &Hover) -> &str {
        match &h.contents {
            HoverContents::Markup(m) => &m.value,
            _ => "",
        }
    }

    #[test]
    fn hover_on_verb_returns_its_doc() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    infer: { prompt: \"hi\" }\n";
        let at = yaml.find("infer").expect("verb") + 2;
        let h = hover(yaml, at).expect("hover present");
        assert!(body(&h).contains("**`infer`**"), "{}", body(&h));
        assert!(body(&h).contains("single LLM call"));
        assert!(body(&h).contains("verb"));
    }

    #[test]
    fn hover_on_top_level_key() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: \"x\" }\n";
        let at = yaml.find("tasks").expect("key") + 1;
        let h = hover(yaml, at).expect("hover present");
        assert!(body(&h).contains("**`tasks`**"));
        assert!(body(&h).contains("top-level key"));
    }

    #[test]
    fn hover_on_task_field() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    depends_on: []\n    exec: { command: \"x\" }\n";
        let at = yaml.find("depends_on").expect("field") + 3;
        let h = hover(yaml, at).expect("hover present");
        assert!(body(&h).contains("**`depends_on`**"));
        assert!(body(&h).contains("task field"));
    }

    #[test]
    fn hover_carries_a_range_over_the_token() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    agent: { prompt: \"x\", tools: [\"nika:read\"] }\n";
        let agent_byte = yaml.find("agent").expect("verb");
        let h = hover(yaml, agent_byte).expect("hover");
        let index = LineIndex::new(yaml);
        let range = h.range.expect("range present");
        assert_eq!(range.start, index.position(agent_byte));
        assert_eq!(range.end, index.position(agent_byte + "agent".len()));
    }

    #[test]
    fn hover_on_non_vocabulary_returns_none() {
        // a task id DEFINITION is not language vocabulary and not a reference
        // → no hover
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: my_task\n    exec: { command: \"x\" }\n";
        let at = yaml.find("my_task").expect("id") + 2;
        assert!(hover(yaml, at).is_none());
    }

    #[test]
    fn hover_on_depends_on_ref_shows_target_task_and_verb() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: greet\n    infer: { prompt: \"hi\", max_tokens: 5 }\n  - id: use_it\n    depends_on: [greet]\n    exec: { command: \"x\" }\n";
        // the LAST `greet` is the depends_on reference (the first is the id)
        let at = yaml.rfind("greet").expect("dep ref") + 1;
        let h = hover(yaml, at).expect("hover on the reference");
        assert!(
            body(&h).contains("**task `greet`**"),
            "names the target: {}",
            body(&h)
        );
        assert!(
            body(&h).contains("infer"),
            "shows the target verb: {}",
            body(&h)
        );
    }

    #[test]
    fn hover_on_template_tasks_ref_shows_target_task_and_verb() {
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: greet\n    infer: { prompt: \"hi\", max_tokens: 5 }\n  - id: use_it\n    exec: { command: \"echo ${{ tasks.greet.output }}\" }\n";
        let at = yaml.find("tasks.greet").expect("tpl ref") + "tasks.gr".len();
        let h = hover(yaml, at).expect("hover on the template reference");
        assert!(body(&h).contains("**task `greet`**"), "{}", body(&h));
        assert!(
            body(&h).contains("infer"),
            "shows the target verb: {}",
            body(&h)
        );
    }

    #[test]
    fn hover_on_whitespace_returns_none() {
        let yaml = "nika: v1\n";
        // a space/newline position is not an identifier
        let at = yaml.find('\n').expect("nl");
        assert!(hover(yaml, at).is_none());
    }

    #[test]
    fn word_at_returns_exact_word_and_span_across_two_words() {
        // "ab cd": the space at byte 2 sits just PAST `ab`, so word_at steps
        // back ONE byte and returns `ab`, not `cd`. Every offset inside or
        // bounding a word maps to that exact word + its [start, end) span.
        let text = "ab cd";
        assert_eq!(word_at(text, 0), Some(("ab", 0, 2)), "start of ab");
        assert_eq!(word_at(text, 1), Some(("ab", 0, 2)), "inside ab");
        assert_eq!(
            word_at(text, 2),
            Some(("ab", 0, 2)),
            "the space just past `ab` steps back to ab, NOT forward to cd"
        );
        assert_eq!(word_at(text, 3), Some(("cd", 3, 5)), "start of cd");
        assert_eq!(word_at(text, 4), Some(("cd", 3, 5)), "inside cd");
        assert_eq!(word_at(text, 5), Some(("cd", 3, 5)), "end-of-text past cd");
    }

    #[test]
    fn word_at_handles_single_char_word_and_leading_space() {
        // A one-char word resolves at both its char and one-past (end init
        // `probe + 1` must not collapse the [0,1) span).
        assert_eq!(word_at("x", 0), Some(("x", 0, 1)));
        assert_eq!(word_at("x", 1), Some(("x", 0, 1)), "one past a 1-char word");
        // A leading space at byte 0 has no prior ident to step back to → None
        // (the `probe > 0` guard must hold; `>=` would underflow-probe).
        assert_eq!(word_at(" y", 0), None, "leading space resolves to no word");
        assert_eq!(word_at(" y", 1), Some(("y", 1, 2)), "the y after the space");
    }

    #[test]
    fn word_at_past_end_clamps_to_len_minus_one_then_steps_back() {
        // "ab " ends in a SPACE. An offset AT or PAST the end (>= len) clamps
        // to `len - 1` (the trailing space), then the step-back finds `ab`.
        // If the clamp uses `len` (e.g. `len / 1`) instead of `len - 1`, the
        // probe lands out of range, the step-back sees the space as the prior
        // byte, and the word is lost (None).
        let text = "ab ";
        assert_eq!(text.len(), 3, "two letters + a trailing space");
        assert_eq!(
            word_at(text, 3),
            Some(("ab", 0, 2)),
            "offset == len clamps to len-1 (the space) then steps back to ab"
        );
        assert_eq!(
            word_at(text, 99),
            Some(("ab", 0, 2)),
            "an offset far past the end behaves the same"
        );
    }

    #[test]
    fn word_at_does_not_overrun_the_word_on_the_right() {
        // The right-expansion `end < len` must stop exactly at the word end.
        // "exec," → the word is `exec` (0..4), the comma is excluded.
        assert_eq!(word_at("exec,", 0), Some(("exec", 0, 4)), "comma excluded");
        assert_eq!(
            word_at("exec,", 4),
            Some(("exec", 0, 4)),
            "caret on the comma"
        );
        // and the whole-string word reaches the true end (end==len boundary).
        assert_eq!(word_at("infer", 5), Some(("infer", 0, 5)), "end == len");
    }

    #[test]
    fn hover_just_past_word_still_resolves() {
        // caret at the byte right after `exec` — common end-of-word position
        let yaml = "nika: v1\nworkflow: w\ntasks:\n  - id: a\n    exec: { command: \"x\" }\n";
        let after = yaml.find("exec").expect("verb") + "exec".len();
        let h = hover(yaml, after).expect("hover");
        assert!(body(&h).contains("**`exec`**"));
    }

    /// Hover on a `tool:` value → the catalog card (category · args ·
    /// required) — derived, so a builtin's card can never lag its truth.
    #[test]
    fn hover_on_tool_value_shows_the_builtin_card() {
        let text = "nika: v1\ntasks:\n  - id: a\n    invoke:\n      tool: nika:jq\n      args: { expression: \".\" }\n";
        let offset = text.find("nika:jq").expect("tool value") + 3;
        let h = hover(text, offset).expect("a card");
        let b = body(&h);
        assert!(b.contains("nika:jq"), "{b}");
        assert!(b.contains("`expression`"), "args listed: {b}");
        assert!(b.contains("required"), "required set named: {b}");
    }

    /// Hover on a `model:` value → the model card with its windows.
    #[test]
    fn hover_on_model_value_shows_the_catalog_windows() {
        let text = "nika: v1\nmodel: ollama/llama3.2:3b\n";
        let offset = text.find("ollama/").expect("model value") + 4;
        let h = hover(text, offset).expect("a card");
        let b = body(&h);
        assert!(b.contains("ollama/llama3.2:3b"), "{b}");
        assert!(b.contains("context") && b.contains("output"), "{b}");
    }

    /// An unknown tool or model stays silent — no invented card.
    #[test]
    fn hover_on_unknown_tool_or_model_stays_silent() {
        let text = "nika: v1\ntasks:\n  - id: a\n    invoke:\n      tool: github.search\n";
        let offset = text.find("github.search").expect("value") + 3;
        assert!(hover(text, offset).is_none());
        let text2 = "nika: v1\nmodel: nosuch/model-x\n";
        let off2 = text2.find("nosuch/").expect("value") + 2;
        assert!(hover(text2, off2).is_none());
    }
}
