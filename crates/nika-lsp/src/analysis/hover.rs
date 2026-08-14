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
/// (verb / envelope / task-field key) OR a task REFERENCE (an `after:` target
/// or `${{ tasks.X }}`) showing the target task's verb.
#[must_use]
pub fn hover(text: &str, offset: usize) -> Option<Hover> {
    value_card_hover(text, offset)
        .or_else(|| task_decl_hover(text, offset))
        .or_else(|| member_ref_hover(text, offset))
        .or_else(|| vocab_hover(text, offset))
        .or_else(|| task_ref_hover(text, offset))
}

/// Hover on a `${{ inputs.X / const.X / secrets.X }}` member — the
/// declaration's card, without leaving the line. Same resolver as
/// go-to-definition; same prose the completion detail teaches.
fn member_ref_hover(text: &str, offset: usize) -> Option<Hover> {
    use std::fmt::Write as _;
    let (root, name) = super::definition::template_member_at(text, offset)?;
    let wf = nika_schema::parse(
        text,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Lenient,
    )
    .ok()?;
    let ty = |t: &nika_schema::Spanned<serde_json::Value>| {
        nika_schema::types::type_expr_display(&t.value)
    };
    let body = match root {
        "inputs" => {
            let (_, decl) = wf.inputs.iter().find(|(n, _)| n.value == name)?;
            match decl {
                nika_schema::VarDecl::Typed {
                    r#type,
                    required,
                    default,
                    description,
                } => {
                    let mut b = format!("**`inputs.{name}`** — _{}_", ty(r#type));
                    if *required {
                        b.push_str(" · required");
                    }
                    if let Some(d) = default {
                        let _ = write!(b, " · default `{d}`");
                    }
                    if let Some(desc) = description {
                        let _ = write!(b, "\n\n{desc}");
                    }
                    b
                }
                nika_schema::VarDecl::Untyped(v) => {
                    format!("**`inputs.{name}`** — _input_\n\n`{v}`")
                }
            }
        }
        "const" => {
            let (_, decl) = wf.consts.iter().find(|(n, _)| n.value == name)?;
            match decl {
                nika_schema::VarDecl::Typed {
                    r#type, default, ..
                } => format!(
                    "**`const.{name}`** — _{}_ · constant `{v}`",
                    ty(r#type),
                    v = default.as_ref()?
                ),
                nika_schema::VarDecl::Untyped(v) => {
                    format!("**`const.{name}`** — _const_\n\n`{v}`")
                }
            }
        }
        "secrets" => {
            wf.secrets.iter().find(|(n, _)| n.value == name)?;
            format!("**`secrets.{name}`** — _secret_\n\nmasked · never echoed · taint-tracked")
        }
        "with" => {
            // task-local: the ENCLOSING task's alias (spec 04)
            let editing = super::scope::current_task_id(text, offset)?;
            let task = wf.tasks.iter().find(|t| t.value.id.value == editing)?;
            let (_, value) = task.value.with.iter().find(|(n, _)| n.value == name)?;
            format!(
                "**`with.{name}`** — _this task's alias_\n\nbinds `{}`",
                value.value
            )
        }
        _ => return None,
    };
    let range = word_at(text, offset).map(|(_, start, end)| {
        let index = LineIndex::new(text);
        Range::new(index.position(start), index.position(end))
    });
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: body,
        }),
        range,
    })
}

/// Hover on a task DECLARATION (the `X:` map key) — the task's place in the
/// DAG, from the SAME wave computation the engine schedules with
/// (`nika_check::analyze` · Kahn levels over the DERIVED `with:`/`after:`
/// edges — one math, one voice). Cyclic or unresolved graphs stay
/// silent here: the diagnostics lane already carries that story.
fn task_decl_hover(text: &str, offset: usize) -> Option<Hover> {
    // W1 « the map »: the declaring token is the task's MAP KEY — the
    // parser's id span (anchored on the key) is the one anchor every
    // surface shares.
    let wf = nika_schema::parse(
        text,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Lenient,
    )
    .ok()?;
    let task = wf.tasks.iter().find(|t| {
        let sp = t.value.id.span;
        (sp.start.0 as usize) <= offset && offset <= (sp.end.0 as usize)
    })?;
    let id = task.value.id.value.clone();
    let (start, end) = (
        task.value.id.span.start.0 as usize,
        task.value.id.span.end.0 as usize,
    );
    let body = dag_card(&wf, &id)?;
    let index = LineIndex::new(text);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: body,
        }),
        range: Some(Range::new(index.position(start), index.position(end))),
    })
}

/// The DAG card body for one task id.
fn dag_card(wf: &nika_schema::raw::RawWorkflow, id: &str) -> Option<String> {
    use std::fmt::Write as _;
    let idx = wf.tasks.iter().position(|t| t.value.id.value == id)?;
    let task = &wf.tasks[idx].value;
    let analyzed = nika_check::analyze(wf).ok()?;
    let waves = &analyzed.topo_waves;
    let wave = waves.iter().position(|w| w.contains(&idx))?;
    let mut body = format!(
        "**task `{id}`** — _{}_ · wave {}/{}",
        task.action.verb(),
        wave + 1,
        waves.len()
    );
    // incoming edges with their ROLES (spec 03 §with/§after · the two doors)
    let incoming = nika_check::analyzer::edges::incoming_of(task);
    let waits: Vec<String> = {
        let mut seen = std::collections::BTreeSet::new();
        incoming
            .iter()
            .filter(|(p, _)| seen.insert(p.clone()))
            .map(|(p, kind)| match kind {
                nika_check::analyzer::EdgeKind::Control(pred) => {
                    format!("`{p}` (control · {pred})")
                }
                other => format!("`{p}` ({})", other.wire_kind()),
            })
            .collect()
    };
    if !waits.is_empty() {
        let _ = write!(body, "\n\nwaits on: {}", waits.join(", "));
    }
    // reverse edges + transitive closure over the SAME derived edge set
    let feeds: Vec<&str> = wf
        .tasks
        .iter()
        .filter(|t| {
            nika_check::analyzer::edges::producer_ids(&t.value)
                .iter()
                .any(|p| p == id)
        })
        .map(|t| t.value.id.value.as_str())
        .collect();
    if feeds.is_empty() {
        let _ = write!(body, "\n\nfeeds: nothing downstream — a terminal task");
    } else {
        let sep = if waits.is_empty() { "\n\n" } else { "\n" };
        let reach = downstream_reach(wf, id);
        let _ = write!(body, "{sep}feeds: {}", code_list(&feeds));
        if reach > feeds.len() {
            let _ = write!(body, " · downstream reach: {reach} tasks");
        }
    }
    Some(body)
}

/// |{ t : id →⁺ t }| — the one BFS lives in [`super::graph`]; the card
/// only needs the count.
fn downstream_reach(wf: &nika_schema::raw::RawWorkflow, id: &str) -> usize {
    super::graph::downstream_ids(wf, id).len()
}

fn code_list(ids: &[&str]) -> String {
    ids.iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(" · ")
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

/// The catalog card for a `provider/model` address. A model the catalog
/// doesn't list (hand-typed · newer than the catalog) still gets the
/// PROVIDER's card — the provider is known even when the model string
/// is the author's own.
fn model_card(value: &str) -> Option<String> {
    let (prov, model) = value.split_once('/')?;
    let provider = nika_catalog::all_providers()
        .iter()
        .find(|p| p.id == prov)?;
    if let Some(m) = provider.models.iter().find(|m| m.model == model) {
        return Some(format!(
            "**`{prov}/{model}`** — _catalog model_\n\ncontext {}k tokens · output {}k tokens",
            m.context_window_tokens / 1000,
            m.max_output_tokens / 1000
        ));
    }
    let key = if provider.requires_key {
        format!("key: `{}`", provider.env_var)
    } else {
        "no key required".to_owned()
    };
    Some(format!(
        "**`{prov}/{model}`** — _{} model_\n\n{} · {key}\n\n_not in the catalog — the string rides to the provider as written_",
        provider.name, provider.description
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

/// Hover for a task REFERENCE (an `after:` target or a `${{ tasks.X }}`
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
        let yaml = "nika: w\ntasks:\n  a:\n    infer: { prompt: \"hi\" }\n";
        let at = yaml.find("infer").expect("verb") + 2;
        let h = hover(yaml, at).expect("hover present");
        assert!(body(&h).contains("**`infer`**"), "{}", body(&h));
        assert!(body(&h).contains("single LLM call"));
        assert!(body(&h).contains("verb"));
    }

    #[test]
    fn hover_on_top_level_key() {
        let yaml = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n";
        let at = yaml.find("tasks").expect("key") + 1;
        let h = hover(yaml, at).expect("hover present");
        assert!(body(&h).contains("**`tasks`**"));
        assert!(body(&h).contains("top-level key"));
    }

    #[test]
    fn hover_on_task_field() {
        let yaml = "nika: w\ntasks:\n  a:\n    timeout: 30s\n    exec: { command: [\"x\"] }\n";
        let at = yaml.find("timeout").expect("field") + 3;
        let h = hover(yaml, at).expect("hover present");
        assert!(body(&h).contains("**`timeout`**"));
        assert!(body(&h).contains("task field"));
    }

    #[test]
    fn hover_carries_a_range_over_the_token() {
        let yaml = "nika: w\ntasks:\n  a:\n    agent: { prompt: \"x\", tools: [\"nika:read\"] }\n";
        let agent_byte = yaml.find("agent").expect("verb");
        let h = hover(yaml, agent_byte).expect("hover");
        let index = LineIndex::new(yaml);
        let range = h.range.expect("range present");
        assert_eq!(range.start, index.position(agent_byte));
        assert_eq!(range.end, index.position(agent_byte + "agent".len()));
    }

    #[test]
    fn hover_on_non_vocabulary_returns_none() {
        // an arbitrary VALUE (a command string) is not vocabulary, not a
        // reference, not a declaration → no hover. (A task id DEFINITION
        // now answers with its DAG card — pinned in its own test below.)
        let yaml = "nika: w\ntasks:\n  my_task:\n    exec: { command: [\"xyzzy\"] }\n";
        let at = yaml.find("xyzzy").expect("command value") + 2;
        assert!(hover(yaml, at).is_none());
    }

    #[test]
    fn hover_on_after_target_shows_target_task_and_verb() {
        let yaml = "nika: w\ntasks:\n  greet:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n  use_it:\n    after: { greet: success }\n    exec: { command: [\"x\"] }\n";
        // the LAST `greet` is the after: target (the first is the id)
        let at = yaml.rfind("greet").expect("after target") + 1;
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
        let yaml = "nika: w\ntasks:\n  greet:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n  use_it:\n    with:\n      msg: \"${{ tasks.greet.output }}\"\n    exec: { command: [\"echo\", \"${{ with.msg }}\"] }\n";
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
        let yaml = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n";
        let after = yaml.find("exec").expect("verb") + "exec".len();
        let h = hover(yaml, after).expect("hover");
        assert!(body(&h).contains("**`exec`**"));
    }

    /// Hover on a `tool:` value → the catalog card (category · args ·
    /// required) — derived, so a builtin's card can never lag its truth.
    #[test]
    fn hover_on_tool_value_shows_the_builtin_card() {
        let text = "nika: v1\ntasks:\n  a:\n    invoke:\n      tool: nika:jq\n      args: { expression: \".\" }\n";
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

    /// An unknown tool or an unknown PROVIDER stays silent — no
    /// invented card. An unknown MODEL of a KNOWN provider gets the
    /// provider's card instead (key env var · honesty line): the
    /// provider is known even when the model string is the author's own.
    #[test]
    fn hover_on_unknown_tool_or_model_stays_silent() {
        let text = "nika: v1\ntasks:\n  a:\n    invoke:\n      tool: github.search\n";
        let offset = text.find("github.search").expect("value") + 3;
        assert!(hover(text, offset).is_none());
        let text2 = "nika: v1\nmodel: nosuch/model-x\n";
        let off2 = text2.find("nosuch/").expect("value") + 2;
        assert!(hover(text2, off2).is_none());

        let text3 = "nika: v1\nmodel: openai/gpt-brand-new\n";
        let off3 = text3.find("openai/").expect("value") + 3;
        let h = hover(text3, off3).expect("the provider card");
        let b = body(&h);
        assert!(b.contains("OPENAI_API_KEY"), "key env var named: {b}");
        assert!(b.contains("not in the catalog"), "honesty line: {b}");
    }

    /// Hover on a task DECLARATION → its DAG card, from the engine's
    /// own wave computation (`analyze` · Kahn levels · the DERIVED
    /// edges). The diamond a → {b, c} → d rides BOTH doors: `b` reads
    /// `a` through a `with:` binding (a value edge), `c` and `d` wait
    /// through `after:` (control edges) — and « waits on » names each
    /// producer WITH its role.
    #[test]
    fn hover_on_task_decl_shows_the_dag_card() {
        let text = "nika: w\ntasks:\n  a:\n    exec: { command: [\"x\"] }\n  b:\n    with:\n      data: \"${{ tasks.a.output }}\"\n    exec: { command: [\"x\"] }\n  c:\n    after: { a: success }\n    exec: { command: [\"x\"] }\n  d:\n    after: { b: success, c: terminal }\n    exec: { command: [\"x\"] }\n";
        let a = hover(text, text.find("\n  a:").expect("a") + 3).expect("card for a");
        let ab = body(&a);
        assert!(ab.contains("wave 1/3"), "{ab}");
        assert!(ab.contains("feeds: `b` · `c`"), "{ab}");
        assert!(
            ab.contains("downstream reach: 3 tasks"),
            "the closure counts b, c AND d: {ab}"
        );
        assert!(
            !ab.contains("waits on"),
            "a root task waits on nothing: {ab}"
        );

        let b = hover(text, text.find("\n  b:").expect("b") + 3).expect("card for b");
        let bb = body(&b);
        assert!(
            bb.contains("waits on: `a` (value)"),
            "a with: binding is a VALUE edge: {bb}"
        );

        let d = hover(text, text.find("\n  d:").expect("d") + 3).expect("card for d");
        let db = body(&d);
        assert!(db.contains("wave 3/3"), "{db}");
        assert!(
            db.contains("waits on: `b` (control · success), `c` (control · terminal)"),
            "control edges carry their predicate: {db}"
        );
        assert!(db.contains("terminal task"), "{db}");
    }

    /// A cyclic graph stays SILENT at the declaration — the diagnostics
    /// lane already tells that story with a span and a code.
    #[test]
    fn hover_on_task_decl_in_a_cycle_stays_silent() {
        let text = "nika: w\ntasks:\n  a:\n    after: { b: success }\n    exec: { command: [\"x\"] }\n  b:\n    after: { a: success }\n    exec: { command: [\"x\"] }\n";
        assert!(hover(text, text.find("\n  a:").expect("a") + 3).is_none());
    }

    /// Hover on island members — the declaration's card: typed input
    /// (type · required · default · description) · secret (masked line)
    /// · a deployment-supplied input (its declared default).
    #[test]
    fn hover_on_member_refs_shows_the_declaration_cards() {
        let text = "nika: w\ninputs:\n  city:\n    type: string\n    required: true\n    default: paris\n    description: target city\n  REGION: { type: string, required: false, default: \"eu\" }\nsecrets:\n  api_key:\n    source: env\n    key: K\ntasks:\n  a:\n    exec: { command: [\"echo\", \"${{ inputs.city }}\", \"${{ inputs.REGION }}\", \"${{ secrets.api_key }}\"] }\n";
        let h = hover(text, text.find("inputs.city").expect("ref") + 6).expect("input card");
        let b = body(&h);
        assert!(
            b.contains("string") && b.contains("required") && b.contains("target city"),
            "{b}"
        );
        let h = hover(text, text.find("secrets.api_key").expect("ref") + 9).expect("secret card");
        assert!(body(&h).contains("never echoed"), "{}", body(&h));
        let h = hover(text, text.find("inputs.REGION").expect("ref") + 8).expect("input card");
        assert!(
            body(&h).contains("string") && body(&h).contains("eu"),
            "{}",
            body(&h)
        );
    }
}
