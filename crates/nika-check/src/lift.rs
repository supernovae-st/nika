// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authored doors, rule 6 (spec `10-authority.md` §the authored
//! doors · `NIKA-AUTH-011` · LAW-AUTH-0332) — **a lift that lifts
//! nothing is refused**.
//!
//! A well-shaped `lift:` naming a law that would never have fired on
//! its task is a trapdoor guarding an empty room: review reads it as a
//! deliberate, justified exception and budgets attention for it, while
//! the thing it names was never going to happen. That is worse than no
//! entry at all — the reason text is real, the risk it describes is
//! not, and the next author copies the shape.
//!
//! **The same class as `NIKA-SEC-015`** · a declared door that guards
//! nothing, sitting in a file that reads as governed. There, a law hid
//! behind a block nobody declared; here, a declaration points at a law
//! that never runs. Both are answered the same way: by RUNNING the law
//! and reading its verdict.
//!
//! **How the question is asked, and the two readings the corpus killed.**
//!
//! - `taint` · the named binding must REACH this task's effect surface.
//!   Reading one — « does the door SUPPRESS a finding » — died on
//!   `core/authority/011`: a live door whose resolved value already sits
//!   inside `permits.fs.read` suppresses nothing, and the fixture is
//!   VALID. Reading two — « does the STATIC re-gate NAME the binding » —
//!   died on the `declassify_admits_the_binding` runtime fixture: an
//!   exec `argv[2]` path is covered by the RUNTIME re-gate alone. What
//!   survives is the spec's own sentence for `028` · « inputs.p is
//!   declared but DOES NOT REACH a verb arg ».
//! - `data-as-code` · [`crate::data_sink::would_fire`] runs law 1's own
//!   per-task classification with the door ignored — no second copy of
//!   the code-bearing table to keep in step.
//!
//! And when the surface DEFERS (a run-derived root · a computed island),
//! rule 6 makes no claim at all — see [`surface_defers`].
//!
//! **Borrowed from a regression, and the regression is the argument.**
//! Terraform's `nonsensitive()` is the same construct: an authored
//! trapdoor lowering one label. In `v1.5.0` a redundant call was an
//! ERROR; on `main` the guard is gone and a redundant declassification
//! is silent. The spec cites the removal as the thing not to repeat.

use nika_schema::expression::NamespaceRef;
use nika_schema::raw::{LiftLaw, RawWorkflow};

/// One authored door that guards nothing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct LiftFinding {
    /// The task carrying the idle door.
    pub task: String,
    /// The law the entry names (`taint` · `data-as-code`).
    pub law: &'static str,
    /// The `from:` binding, for the `taint` arm.
    pub from: Option<String>,
    /// The witness sentence — what was declared, and what never fires.
    pub detail: String,
    /// The two repairs (drop the entry · or fix the file it describes).
    pub fix: String,
}

impl LiftFinding {
    /// The ONE wire code (spec 10 · the authored doors rule 6).
    pub const WIRE_CODE: &'static str = "NIKA-AUTH-011";
}

/// Scan a workflow for authored doors that lift nothing (rule 6).
#[must_use]
pub(crate) fn scan_idle_doors(wf: &RawWorkflow) -> Vec<LiftFinding> {
    let mut out = Vec::new();
    for spanned in &wf.tasks {
        let task = &spanned.value;
        if surface_defers(task) {
            // the static laws cannot see this surface · no claim
            // (law 3 / law 4) — the door may well be the runtime twin's
            continue;
        }
        let reaches = surface_refs(task);
        for entry in &task.lift {
            let id = task.id.value.clone();
            match entry.law.value {
                LiftLaw::Taint => {
                    // `from:` is parser-required on this law; a missing
                    // one is already a parse refusal, never our finding.
                    let Some(from) = entry.from.as_ref() else {
                        continue;
                    };
                    if !reaches.contains(&from.value) {
                        out.push(idle_taint(id, &from.value));
                    }
                }
                LiftLaw::DataAsCode => {
                    if !crate::data_sink::would_fire(wf, task) {
                        out.push(idle_data_as_code(id));
                    }
                }
                // #[non_exhaustive] · a new law joins deliberately, with
                // its own way of being asked whether it would have fired.
                _ => {}
            }
        }
    }
    out
}

/// Every `${{ }}` reference of this task's effect surface, in the
/// canonical dotted form a `lift.from:` names (`inputs.p` ·
/// `tasks.fetch.output`).
fn surface_refs(task: &nika_schema::raw::RawTask) -> Vec<String> {
    let mut texts: Vec<&str> = crate::flow::action_effect_fields(&task.action);
    if let nika_schema::raw::RawAction::Invoke(a) = &task.action
        && let Some(args) = a.args.as_ref()
    {
        texts.extend(crate::flow::collect_json_strings(&args.value));
    }
    for (_, value) in &task.with {
        texts.extend(crate::flow::collect_json_strings(&value.value));
    }
    let mut out = Vec::new();
    for text in texts {
        for reference in crate::flow::refs_in_str(text) {
            match reference {
                NamespaceRef::Inputs(name) => out.push(format!("inputs.{name}")),
                NamespaceRef::Const(name) => out.push(format!("const.{name}")),
                NamespaceRef::Secrets(name) => out.push(format!("secrets.{name}")),
                NamespaceRef::With(name) => out.push(format!("with.{name}")),
                NamespaceRef::Tasks { id, field } => out.push(match field {
                    Some(f) => format!("tasks.{id}.{f}"),
                    None => format!("tasks.{id}"),
                }),
                _ => {}
            }
        }
    }
    out
}

/// Whether this task's effect surface carries a reference the STATIC
/// laws cannot resolve — a run-derived root (`tasks.*` · `with.*` · a
/// loop-local) or a computed island.
///
/// **A deferral is not an answer.** Both laws rule 6 judges have runtime
/// twins and both DEFER on a dynamic value (NEP-0006 law 3 · NEP-0004
/// law 4). A door declared for the runtime twin is therefore invisible
/// to the static half, and refusing it would forbid the one door that
/// surface can have. So when the surface defers, rule 6 makes NO CLAIM —
/// the same discipline the drift lane uses when a dynamic consumer
/// poisons its used-set: no claim, never wrong.
///
/// Deliberately OVER-approximating: every `${{ }}` island of the task's
/// effect fields is scanned, not just the arguments the re-gate selects.
/// A wider scan can only make rule 6 quieter, never louder — the safe
/// direction for a law that refuses.
fn surface_defers(task: &nika_schema::raw::RawTask) -> bool {
    let mut texts: Vec<&str> = crate::flow::action_effect_fields(&task.action);
    if let nika_schema::raw::RawAction::Invoke(a) = &task.action
        && let Some(args) = a.args.as_ref()
    {
        texts.extend(crate::flow::collect_json_strings(&args.value));
    }
    for (_, value) in &task.with {
        texts.extend(crate::flow::collect_json_strings(&value.value));
    }
    texts.iter().any(|text| {
        nika_schema::expression::scan_templates(text).is_ok_and(|islands| {
            islands.iter().any(|island| {
                let refs = nika_schema::expression::expr_refs(&island.expr);
                // zero or many refs = a computed island · not decidable
                let [reference] = refs.as_slice() else {
                    return true;
                };
                !matches!(
                    reference,
                    NamespaceRef::Inputs(_) | NamespaceRef::Const(_) | NamespaceRef::Secrets(_)
                )
            })
        })
    })
}

/// The `taint` arm's witness.
fn idle_taint(task: String, from: &str) -> LiftFinding {
    LiftFinding {
        detail: format!(
            "task `{task}` lifts the `taint` law on `{from}`, but nothing in this task \
             reads that binding: the value never reaches an argument, so the law has \
             nothing to move here · the door guards an empty room (rule 6 · \
             LAW-AUTH-0332)"
        ),
        fix: format!(
            "drop the entry · or, if the task was meant to READ `{from}`, fix the \
             reference the door was written for"
        ),
        task,
        law: "taint",
        from: Some(from.to_owned()),
    }
}

/// The `data-as-code` arm's witness.
fn idle_data_as_code(task: String) -> LiftFinding {
    LiftFinding {
        detail: format!(
            "task `{task}` lifts the `data-as-code` law, but the sink law raises nothing here: \
             the fetch is inert (no code-bearing extension), dynamic, or not a fetch at all · \
             the door guards an empty room (rule 6 · LAW-AUTH-0332)"
        ),
        fix: "drop the entry — an inert fetch needs no door · or, if the artifact IS \
              code-bearing, name the URL the door was written for"
            .to_owned(),
        task,
        law: "data-as-code",
        from: None,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn doors(yaml: &str) -> Vec<LiftFinding> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        scan_idle_doors(&wf)
    }

    /// `core/authority/028` · the door names `inputs.p`, the argument
    /// reads `const.q`. Law 2 never looks at the binding.
    #[test]
    fn a_taint_door_on_an_unused_binding_is_refused() {
        let f = doors(
            "nika: t\npermits:\n  fs: { read: [\"datasets/**\"] }\n  tools: [\"nika:read\"]\n\
             inputs:\n  p: { type: string, default: \"vendor/q3.csv\" }\n\
             const:\n  q: \"datasets/q3.csv\"\n\
             tasks:\n  load:\n    invoke:\n      tool: nika:read\n      args: { path: \"${{ const.q }}\" }\n\
             \x20   lift:\n      - law: taint\n        from: inputs.p\n        because: \"author-baked\"\n",
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].law, "taint");
        assert_eq!(f[0].from.as_deref(), Some("inputs.p"));
        assert!(f[0].detail.contains("empty room"), "{}", f[0].detail);
    }

    /// `core/authority/011` · the SAME shape with the argument actually
    /// reading `inputs.p`. The door is LIVE even though the resolved
    /// value already sits inside `permits.fs.read` — engagement is the
    /// test, not the verdict. This arm is what refuted the first
    /// reading of rule 6; it must stay.
    #[test]
    fn a_taint_door_the_law_looks_at_is_left_alone() {
        let f = doors(
            "nika: t\npermits:\n  fs: { read: [\"datasets/**\", \"vendor/**\"] }\n  tools: [\"nika:read\"]\n\
             inputs:\n  p: { type: string, default: \"vendor/q3.csv\" }\n\
             tasks:\n  load:\n    invoke:\n      tool: nika:read\n      args: { path: \"${{ inputs.p }}\" }\n\
             \x20   lift:\n      - law: taint\n        from: inputs.p\n        because: \"vendor inventory path\"\n",
        );
        assert!(f.is_empty(), "a live door is not idle: {f:?}");
    }

    /// `core/authority/029` · a `.csv` is inert, the sink law never fires.
    #[test]
    fn a_data_as_code_door_on_an_inert_fetch_is_refused() {
        let f = doors(
            "nika: t\npermits:\n  net: { http: [\"data.example.com\"] }\n  tools: [\"nika:fetch\"]\n\
             tasks:\n  rows:\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://data.example.com/q3/rows.csv\" }\n\
             \x20   lift:\n      - law: data-as-code\n        because: \"inert csv\"\n",
        );
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(f[0].law, "data-as-code");
        assert!(f[0].from.is_none());
    }

    /// The clean sibling · a `.pkl` IS code-bearing, so the door earns
    /// its place. Without this arm the law could refuse every door and
    /// still look green on the three above.
    #[test]
    fn a_data_as_code_door_on_a_code_bearing_fetch_is_left_alone() {
        let f = doors(
            "nika: t\npermits:\n  net: { http: [\"data.example.com\"] }\n  tools: [\"nika:fetch\"]\n\
             tasks:\n  rows:\n    invoke:\n      tool: nika:fetch\n      args: { url: \"https://data.example.com/models/legacy.pkl\" }\n\
             \x20   lift:\n      - law: data-as-code\n        because: \"archived for provenance, never loaded\"\n",
        );
        assert!(f.is_empty(), "a live door is not idle: {f:?}");
    }

    /// A file with no `lift:` at all can never mint this code.
    #[test]
    fn no_door_no_finding() {
        let f = doors("nika: t\npermits: {}\ntasks:\n  go:\n    infer: { prompt: \"hi\" }\n");
        assert!(f.is_empty(), "{f:?}");
    }

    #[test]
    fn the_wire_code_is_the_spec_row() {
        assert_eq!(crate::LiftFinding::WIRE_CODE, "NIKA-AUTH-011");
    }
}
