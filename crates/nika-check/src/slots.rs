// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The unfilled-scaffold class — a skeleton is not yet a workflow.
//!
//! `nika new chain` laid ten `# SLOT:` **comments**. The parser drops
//! comments by construction, so `check` mentioned none of them, the run
//! exited 0, and `output.md` was left holding the scaffold's own prompt
//! echoed back by a mock — a file that outlives the terminal line which
//! explained it (#1066).
//!
//! **A comment cannot refuse.** That is the whole finding: the marker
//! had to descend into the VALUE, where the parser sees it. The form is
//!
//! ```yaml
//! prompt: |
//!   <SLOT: what should the model do with the gathered text?>
//! ```
//!
//! and the judgment is two-ended by construction — an unfilled scaffold
//! refuses, a filled file must never trip. A marker is a plain scalar
//! whose trimmed line opens with `<SLOT:` and closes with `>`; it is
//! matched per LINE, because a half-filled block scalar that still holds
//! one marker line would send that line to the model verbatim.
//!
//! **Scope, stated rather than implied.** This reads the value surfaces
//! a scaffold leaves for its author to fill: `model:`, every
//! `const:`/`inputs:` literal, and every `infer:`/`agent:` `prompt:` and
//! `system:`. A marker anywhere else is invisible here — which is why
//! the pack's own family traversal walks every shipped skeleton and
//! asserts the judge sees every marker they carry, rather than trusting
//! this list to have stayed wide enough.

use nika_schema::raw::{RawAction, RawWorkflow};
use nika_schema::types::VarDecl;

use crate::ByteSpan;

/// The opening token of a slot marker. A value, never a comment — the
/// one non-negotiable of the form (a comment cannot refuse).
pub const MARKER_OPEN: &str = "<SLOT:";

/// One value a scaffold left for its author.
///
/// Not an error in the usual sense: the person typed `nika new` and did
/// nothing wrong. The render says so — the wording is « ready to be
/// filled », never « broken » — but it IS a refusal, because a run over
/// an unfilled scaffold spends money to produce a lie.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct SlotFinding {
    /// The dotted path to the value (`tasks.think.infer.prompt`).
    pub path: String,
    /// What the scaffold asked for — the marker's own words, so the
    /// message teaches instead of just pointing.
    pub hint: String,
    /// The source range, so the render can name the line.
    pub span: ByteSpan,
}

impl SlotFinding {
    /// Create a slot finding (invariant #19).
    #[must_use]
    pub fn new(path: String, hint: String, span: ByteSpan) -> Self {
        Self { path, hint, span }
    }
}

/// The marker's own words when this line is one, `None` otherwise.
///
/// Deliberately exact at both ends: `<SLOT:` must OPEN the trimmed line
/// and `>` must close it. A prompt that merely discusses the convention
/// (« write `<SLOT: …>` where you want a hole ») is prose, not a hole.
fn marker_hint(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix(MARKER_OPEN)?.strip_suffix('>')?;
    Some(inner.trim())
}

/// The first unfilled line of a scalar, if it has one.
fn unfilled(value: &str) -> Option<&str> {
    value.lines().find_map(marker_hint)
}

/// A JSON literal is unfilled when it is a string carrying a marker.
/// Numbers, bools and structures cannot hold one.
fn unfilled_json(value: &serde_json::Value) -> Option<&str> {
    unfilled(value.as_str()?)
}

fn push(out: &mut Vec<SlotFinding>, path: String, hint: &str, span: nika_schema::Span) {
    out.push(SlotFinding::new(
        path,
        hint.to_owned(),
        ByteSpan::new(span.start.0, span.end.0),
    ));
}

/// The `const:` and `inputs:` literals — where most skeletons put the
/// author's own subject (`brief` · `query` · `subject` · `goal`).
fn scan_values(
    out: &mut Vec<SlotFinding>,
    block: &str,
    entries: &[(nika_schema::Spanned<String>, VarDecl)],
) {
    for (key, decl) in entries {
        let literal = match decl {
            VarDecl::Untyped(v) => Some(v),
            VarDecl::Typed { default, .. } => default.as_ref(),
        };
        if let Some(hint) = literal.and_then(unfilled_json) {
            push(out, format!("{block}.{}", key.value), hint, key.span);
        }
    }
}

/// The model job — the value whose echo was the artifact in #1066.
fn scan_action(out: &mut Vec<SlotFinding>, task: &str, action: &RawAction) {
    let verb = action.verb();
    let (prompt, system) = match action {
        RawAction::Infer(a) => (Some(&a.prompt), a.system.as_ref()),
        RawAction::Agent(a) => (Some(&a.prompt), a.system.as_ref()),
        // `exec:`/`invoke:` carry argv and tool args, not a model job —
        // and `RawAction` is `#[non_exhaustive]`, so a fifth verb lands
        // here silently rather than failing the build. The family
        // traversal is what would catch a marker this arm cannot see.
        _ => (None, None),
    };
    for (field, scalar) in [("prompt", prompt), ("system", system)] {
        let Some(scalar) = scalar else { continue };
        if let Some(hint) = unfilled(&scalar.value) {
            push(
                out,
                format!("tasks.{task}.{verb}.{field}"),
                hint,
                scalar.span,
            );
        }
    }
}

/// Every value this scaffold still expects its author to fill, in source
/// order — the order the message lists them in, because a person reads
/// a file top to bottom.
#[must_use]
pub fn scan(wf: &RawWorkflow) -> Vec<SlotFinding> {
    let mut out = Vec::new();
    if let Some((model, hint)) = wf
        .model
        .as_ref()
        .and_then(|m| Some((m, unfilled(&m.value)?)))
    {
        push(&mut out, "model".to_owned(), hint, model.span);
    }
    scan_values(&mut out, "inputs", &wf.inputs);
    scan_values(&mut out, "const", &wf.consts);
    for task in &wf.tasks {
        scan_action(&mut out, &task.value.id.value, &task.value.action);
    }
    out.sort_by_key(|f| f.span.start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(yaml: &str) -> Vec<SlotFinding> {
        let wf = nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        scan(&wf)
    }

    /// The scaffold's own shape: a marker in the prompt, a marker in a
    /// const. Both are VALUES — the parser kept them, so the judge sees
    /// them. This is the half a `# SLOT:` comment could never reach.
    #[test]
    fn a_marker_in_a_value_is_found_with_its_path_and_words() {
        let found = slots(concat!(
            "nika: draft\n",
            "model: mock/echo\n",
            "const:\n",
            "  brief: \"<SLOT: what should this produce?>\"\n",
            "tasks:\n",
            "  think:\n",
            "    infer:\n",
            "      prompt: |\n",
            "        <SLOT: the one model job>\n",
            "      max_tokens: 10\n",
        ));
        let paths: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            ["const.brief", "tasks.think.infer.prompt"],
            "{found:#?}"
        );
        assert_eq!(found[0].hint, "what should this produce?");
        assert_eq!(found[1].hint, "the one model job");
        // The parser hands these nodes a POSITION, not a range (start ==
        // end) — enough to name a line, which is what the message owes
        // its reader. Asserted so the day it widens is a decision.
        assert_eq!(found[0].span.start, 38, "the `brief` key: {found:#?}");
        assert_eq!(found[1].span.start, 132, "the `prompt` key: {found:#?}");
    }

    /// The other end, and the one that matters more: a FILLED file must
    /// never trip. Proving only the refusal would prove a judge that
    /// refuses everything.
    #[test]
    fn a_filled_file_carries_no_slot() {
        let found = slots(concat!(
            "nika: draft\n",
            "model: mock/echo\n",
            "const:\n",
            "  brief: \"summarise the release notes\"\n",
            "tasks:\n",
            "  think:\n",
            "    infer:\n",
            "      prompt: |\n",
            "        Summarise this: ${{ const.brief }}\n",
            "      max_tokens: 10\n",
        ));
        assert!(found.is_empty(), "{found:#?}");
    }

    /// A marker is a whole line, not a substring. A prompt that TEACHES
    /// the convention is prose — refusing it would make the docs
    /// unwritable, and « the explanation reproduces the token » is a
    /// drift class this house has already been bitten by.
    #[test]
    fn prose_about_the_marker_is_not_a_marker() {
        let found = slots(concat!(
            "nika: doc\n",
            "model: mock/echo\n",
            "tasks:\n",
            "  t:\n",
            "    infer:\n",
            "      prompt: |\n",
            "        Explain that a hole is written <SLOT: like this> inline.\n",
            "      max_tokens: 10\n",
        ));
        assert!(found.is_empty(), "{found:#?}");
    }

    /// A half-filled block scalar still refuses: the leftover line would
    /// be sent to the model verbatim, which is the exact hazard the
    /// chain template's own comment warns about.
    #[test]
    fn one_leftover_line_in_a_block_scalar_still_refuses() {
        let found = slots(concat!(
            "nika: half\n",
            "model: mock/echo\n",
            "tasks:\n",
            "  t:\n",
            "    infer:\n",
            "      prompt: |\n",
            "        Summarise the document below.\n",
            "        <SLOT: say how long the summary should be>\n",
            "      max_tokens: 10\n",
        ));
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(found[0].hint, "say how long the summary should be");
    }
}
