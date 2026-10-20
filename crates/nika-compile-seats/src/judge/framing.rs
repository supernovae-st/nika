// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! How a judging question is told about the request it judges (R4 A11): after the verdict's
//! compiler-owned reference, what the question asks, then which request is asked and which is
//! history, read from the state the question is shown ([`state`](super::state),
//! [`over_document`](super::over_document)). A revision says so: the change appended to the
//! earlier request, that request resolved, or the change applied over a base whose request is
//! unknown or over its complete document; any other candidate is told what a request to author
//! this very workflow asks of its bytes. Moved from the verifier of `nika-compile-cognition` at
//! that crate's size cap.

use serde_json::Value;

/// What a question over a revision adds to its instructions: the whole revised request is asked,
/// the earlier request is history.
pub const REVISED: &str = "This candidate REVISES an earlier workflow. `request` is the whole revised request it must carry: the earlier request with each clause the change replaces replaced in place, then the change's additions; every other earlier clause is still asked. `revision.change` is the change as the human stated it; `revision.base_request` is the earlier request, history only: a clause the change replaced is no longer asked. A clause asking to create or modify the workflow file itself is carried by this candidate being that workflow; every clause about the workflow's execution is judged on what its bytes do.";

/// What a question over a revision whose request is the earlier request followed by the change
/// (`… Change: …`) adds to its instructions.
pub const REVISED_APPENDED: &str = "This candidate REVISES an earlier workflow. `request` is the earlier request followed by the change the human stated (« Change: … »): where they differ the change takes precedence, so a clause of the earlier request the change replaces is superseded and no longer asked; every other earlier clause is still asked. `revision.change` is the change as stated. A clause asking to create or modify the workflow file itself is carried by this candidate being that workflow; every clause about the workflow's execution is judged on what its bytes do.";

/// What a question over a revision of a base whose own request is unknown adds to its
/// instructions: the request states only the change, so the base's own behaviour is neither
/// asked again nor extra, and the candidate is judged as the base with exactly that change.
pub const REVISED_DOCUMENT: &str = "This candidate REVISES the existing workflow `revision.base_nika`, whose own request is unknown: `request` and `revision.change` state only the change. What the base already does is not asked again and is not extra: it must stay as in the base wherever the change does not touch it. faithful: the candidate is the base with exactly this change applied. unfaithful: the change is missing or done differently, or the candidate adds, removes or alters anything else of the base. A clause asking to modify the workflow file itself is carried by this candidate being that workflow.";

/// What a question over a revision applied over the complete document of a base whose request
/// is known adds: the base is shown whole and the change is judged over it, the earlier request
/// history where the change takes precedence.
pub const REVISED_OVER_DOCUMENT: &str = "This candidate REVISES the existing workflow `revision.base_nika` by the change `revision.change`, applied over its complete document. `revision.base_request` is the request the base answers, history only: where it and the change differ, the change takes precedence and a clause it replaces is superseded, never asked. What the base already does is not extra: it must stay as in the base wherever the change does not touch it. faithful: the candidate is the base with exactly this change applied. unfaithful: the change is missing or done differently, or the candidate adds, removes or alters anything else of the base. A clause asking to modify the workflow file itself is carried by this candidate being that workflow.";

/// What a question over any other candidate adds to its instructions: a request to author this
/// workflow (« create report.nika that … ») asks for this program, not for a step writing its own
/// file. It attests no save, path or name: a workflow file the request names is the file the
/// host's Save step writes, any other stated name is judged on the bytes, and every write the
/// program itself does (another `.nika` too) stays judged.
pub const CREATED: &str = "This candidate is the workflow the request asks Nika to author. A clause asking to create this workflow asks for this program; it does not ask the program to write its own file. Saving that file is the host's step after review, outside these bytes: it is neither missing nor done here. A workflow file the request names (`<name>.nika`) names the file that step writes, not the candidate's `nika:` name; any other name the request gives this workflow is judged against the candidate's own `nika:` name. A workflow identity is not proof of a Save filename or path; do not infer a destination absent from the state. Every clause about the workflow's execution is judged on what the bytes do, including every file the program itself writes (another `.nika` file among them).";

/// Preparation dialogue and the program it prepares have different obligations. Choices stay
/// grounded in the host's bound answer context; descriptions are never human authorization.
const PREPARATION: &str = "Distinguish what Nika must do while preparing this candidate from what the resulting workflow must do when run. A request to ask the person for choices now, before preparing or saving, does not require the workflow to ask again at runtime. The host's `answers` may carry `bound_values`, exact `bindings`, and `choice_context`: the question and option labels are descriptions authored by Nika, while the selected option's provenance and human citation record what the person actually picked or delegated. An option may have no business values: its selection remains meaningful, but creates no literal binding. Use those descriptions only to interpret that scoped answer; they are not independent human instructions, and grant no Save, Run or other effect. An offered alternative that the person did not choose is not an additional requested operation; a separate explicit requirement remains binding. Read the human request and its later answers together without discarding either. A request for the workflow itself to ask on each run, or before a runtime effect, still requires that runtime interaction. Every runtime requirement remains checked against the bytes, including filters, exclusions, counts, totals, units, targets, conditions and writes. A preparation-only prohibition on saving or running now does not forbid the requested future program's writes; a prohibition about the program's effects still does. This verdict attests the candidate's program, never that host actions or preparation steps happened.";

/// A judging question's instructions: the verdict's `reference` first, so every question of a
/// verdict opens with the same bytes, then `text`, then what the request the `state` shows asks
/// of these bytes: a revision's which request is asked and which is history, any other's what a
/// request to author this very workflow asks.
#[must_use]
pub fn told(state: &Value, reference: &str, text: &str) -> String {
    let framing = match state.get("revision") {
        Some(revision)
            if revision["over_document"] == Value::Bool(true)
                && !revision["base_request"].is_null() =>
        {
            REVISED_OVER_DOCUMENT
        }
        Some(revision) if revision["appended"] == Value::Bool(true) => REVISED_APPENDED,
        Some(revision) if revision.get("base_nika").is_some() => REVISED_DOCUMENT,
        Some(_) => REVISED,
        None => CREATED,
    };
    format!("{reference}\n\n{text} {framing} {PREPARATION}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{CREATED, told};

    /// A create is told that a workflow file the request names is the file the host's Save step
    /// writes, never a demand on the candidate's `nika:` name, while any other name it gives is
    /// still judged against that name.
    #[test]
    fn a_requested_workflow_file_names_the_save_not_the_nika_name() {
        let instructions = told(&json!({}), "reference", "question");
        assert!(instructions.contains(CREATED), "{instructions}");
        for phrase in [
            "A workflow file the request names (`<name>.nika`) names the file that step writes, not the candidate's `nika:` name",
            "any other name the request gives this workflow is judged against the candidate's own `nika:` name",
            "A workflow identity is not proof of a Save filename or path",
        ] {
            assert!(instructions.contains(phrase), "{phrase}");
        }
    }
}
