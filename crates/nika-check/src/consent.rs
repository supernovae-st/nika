// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The affirmative-consent lane (NEP-0020 · `NIKA-SEC-014` · P0-2 of the
//! 2026-07-30 UX audit — the hint-only lane landed 2026-07-30 and
//! escalates here to the refusal it was measuring).
//!
//! The defect: a REFUSED confirm is a task SUCCESS whose value is
//! `false` — the runtime records the Deny in the approval attestation
//! only (`nika-runtime/src/approval.rs` · `SettleAs::Ran(Success)`), so
//! every route that does not consume the answer passes the effect
//! through:
//!
//! - a bare state edge `after: { ask: success }` — the refusal settles
//!   `success`, the edge admits it, the exec fires;
//! - a `when:` that never references the prompt's output — the answer
//!   is decoration;
//! - a `when:` that references the output but stays TRUE on `false`
//!   (`with.go == true || with.go == false`) — the refusal cannot block.
//!
//! The law (spec `10-authority.md` §the affirmative-consent law):
//! **false triggers exactly zero effects.** For every confirm-mode
//! `invoke: nika:prompt` and every egress-capable task (the ONE effect
//! table, `trifecta::egress_capable`), every route from the gate to the
//! task must be CLOSED — by an affirmative gate (its `when:` evaluates
//! to false under the refusal substitution · [`gate_verdict`]), by `when: false`,
//! or by a closer confirm gate (the nearest gate owns its closure — the
//! approval-batch precedent).
//!
//! **Sound, never a false red.** The blocking row fires only on the
//! PROVEN route: every edge on it admits the refusal (the pass-set —
//! a refusal settles `Success`, a `failure`/`skipped` predicate carries
//! nothing) and every gate on it is proven open. A gate the fragment
//! cannot decide (a nested template binding carrying the answer · a
//! non-fragment expression) makes the route UNPROVEN — the advisory
//! hint's ground, exactly the pre-escalation behavior, never a code.
//! `mode: choice` stays out of scope (silence, never wrong).
//!
//! **A closed gate stops a route only where its skip cancels.** A gate
//! proven FALSE under the refusal settles its task `skipped`, never
//! `cancelled` (`when:` is POST-gate · spec 03), and a value edge ADMITS a
//! skipped producer (the binding reads defined-null): a task that reads a
//! gated stage's value still REACHES ITS VERB on « no ». So the walk
//! continues past a closed gate over the edges that carry its value and
//! admit `skipped` (the observation edges, the `after: { x: skipped }`
//! handler and the stage's own cleanup unit stay out of this slice).
//! GATE-v2 is an AND over EVERY incoming edge, so reaching a task proves
//! nothing by itself — past a skipped stage the verdict reads [`Refusal`]
//! and keeps three fates apart: cancelled in EVERY refusal run is silence
//! · its verb reached in EVERY refusal run is the refusal · anything
//! between is the advisory. « Not proven cancelled » is never « proven
//! open », and no verb is ever ASSUMED to succeed: a route that needs an
//! intermediate or an independent task to settle `success` (the
//! laundering hop · `after: { other: success }`) is advisory here, however
//! likely that success is. The witness is the verb being REACHED — the
//! law counts the attempt, not what the verb then makes of a null input.
//!
//! Method: per confirm-mode prompt, a BFS over the refusal-admitting
//! derived edges carrying the route's proof state (clean vs tainted —
//! a sink reached BOTH ways refuses, the proven route dominates). The
//! refusal substitution resolves the gate's settled facts: the exact
//! single-island `with:` carrier of `tasks.<prompt>.output` is `false`,
//! the one of `tasks.<prompt>.status` is `"success"` (a status read is
//! decidable, and it is NOT consent). The lane is gated on a valid DAG
//! (the caller's IFC/policy gating) and skips past the analysis task
//! cap (the O(P·E) per-prompt walk shares the `DoS` floor).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use nika_schema::raw::{RawAction, RawTask, RawWorkflow};

// The refusal substitution itself (what a `when:` evaluates to once the
// gate answered « no ») is substrate — `analyzer::gates`, descended at the
// 15k wall. This lane keeps the verdicts.
use crate::analyzer::gates::{Gate, gate_certain, gate_verdict};
use crate::analyzer::settle::{S_ALL, S_CANCELLED, S_SKIPPED, S_SUCCESS, Settled, fold_settled};
use crate::analyzer::{Edge, SettledState};
use crate::hints::Hint;

/// The blocking row (NEP-0020 · `NIKA-SEC-014`): a confirm-mode human
/// gate whose refusal an egress-capable task cannot escape — the
/// witness names the gate AND the sink, the fix teaches the
/// affirmative pattern.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct ConsentFinding {
    /// The confirm gate whose answer leaks (`tasks.<prompt>`).
    pub prompt: String,
    /// The egress-capable task the refusal reaches.
    pub sink: String,
    /// The human row — the defect, its mechanism, the repair.
    pub detail: String,
}

impl ConsentFinding {
    /// The ONE wire code (spec 10 · NEP-0020) — every surface reads it
    /// (the findings fold · the extra-conformance list).
    pub const WIRE_CODE: &'static str = "NIKA-SEC-014";
}

/// The lane's two outputs: the PROVEN refusal (blocking) and the
/// undecidable remainder (advisory — the pre-escalation hint).
#[derive(Debug, Default)]
pub(crate) struct ConsentScan {
    /// One row per (gate, sink) pair on a proven non-affirmative route.
    pub(crate) findings: Vec<ConsentFinding>,
    /// One row per (gate, sink-or-gate) pair the fragment cannot decide.
    pub(crate) hints: Vec<Hint>,
}

/// One step of the walk: the node, whether an undecidable gate taints the
/// route, and the nearest stage the refusal SKIPPED on the way here
/// (`None` = the route crossed no closed gate — the pre-existing walk).
type Step = (usize, bool, Option<usize>);

/// Judge the affirmative-consent lane over the derived graph. Empty
/// unless a confirm-mode prompt reaches an egress-capable descendant
/// over a route that never gates on the answer.
///
/// `topo_waves` is the valid topological order (the caller only runs
/// this lane on a conformant DAG) — the refusal fold reads producers
/// before consumers.
pub(crate) fn scan_consent(
    wf: &RawWorkflow,
    edges: &[Edge],
    topo_waves: &[Vec<usize>],
) -> ConsentScan {
    let mut scan = ConsentScan::default();
    if wf.tasks.len() > crate::analysis::ANALYSIS_TASK_CAP {
        return scan;
    }
    // Only the edges that ADMIT the refusal — it settles Success, so a
    // `failure`/`skipped`-only predicate carries nothing (the pass-set
    // is the soundness floor: a predicate-blind walk would red a
    // failure-edge route that can never fire).
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); wf.tasks.len()];
    // The edges a SKIPPED producer still feeds: they carry its value and
    // their pass-set admits `skipped`. Its cleanup unit is not one of them
    // — a producer that never ran unwinds nothing (spec 03 §unwind).
    let mut value_on_skip: Vec<Vec<usize>> = vec![Vec::new(); wf.tasks.len()];
    let cleanup: BTreeSet<usize> = edges
        .iter()
        .filter(|e| !e.kind.is_scheduling())
        .map(|e| e.to)
        .collect();
    for e in edges {
        if e.kind.admits(SettledState::Success) {
            children[e.from].push(e.to);
        }
        if e.kind.carries_value()
            && e.kind.admits(SettledState::Skipped)
            && !cleanup.contains(&e.to)
        {
            value_on_skip[e.from].push(e.to);
        }
    }
    // (gate, sink) → the skipped stage the proven route crossed · `None`
    // when a route that crosses no closed gate proves the sink as well.
    let mut blocked: BTreeMap<(String, String), Option<String>> = BTreeMap::new();
    let mut uncertain: BTreeSet<(String, String)> = BTreeSet::new();
    for (idx, task) in wf.tasks.iter().enumerate() {
        if !is_confirm_prompt(&task.value) {
            continue;
        }
        let prompt = task.value.id.value.as_str();
        // Folded on first need: a workflow with no value read of a gated
        // stage never pays for it.
        let mut refusal: Option<Refusal> = None;
        // BFS with the route's proof state: tainted `false` = every gate
        // so far is proven open or closed (the refusal flows), `true` = an
        // undecidable gate taints the route. The clean state DOMINATES — a
        // sink reached both ways refuses (one proven route is enough) —
        // and so does the route that crossed no skipped stage: it is
        // judged exactly as before, so no earlier verdict can change.
        let mut seen: BTreeMap<usize, Vec<(bool, bool)>> = BTreeMap::new();
        let mut queue: VecDeque<Step> = children[idx].iter().map(|&c| (c, false, None)).collect();
        while let Some((n, tainted, skipped)) = queue.pop_front() {
            let states = seen.entry(n).or_default();
            if states
                .iter()
                .any(|&(t, s)| (tainted || !t) && (skipped.is_some() || !s))
            {
                continue;
            }
            states.push((tainted, skipped.is_some()));
            let t = &wf.tasks[n].value;
            // A closer confirm gate owns its closure (the approval-batch
            // precedent).
            if is_confirm_prompt(t) {
                continue;
            }
            // Past a skipped stage, REACHING a task proves nothing (GATE-v2
            // is an AND over every incoming edge) — the refusal fold
            // decides. Cancelled in every run: no effect, nothing handed
            // on. Verb reached in every run: the witness. Neither: the
            // advisory.
            let mut witnessed = true;
            if skipped.is_some() {
                let world = refusal.get_or_insert_with(|| Refusal::of(wf, edges, topo_waves, idx));
                if world.may.of(n) == S_CANCELLED {
                    continue;
                }
                witnessed = world.sure.of(n) == REACHED;
            }
            let verdict = gate_verdict(t, prompt);
            // A gate proven FALSE under the refusal (the affirmative
            // `when:` · `when: false`) SKIPS its task: every edge that
            // needs its success is cut, the edges that carry its value
            // are not.
            if verdict == Gate::Closed {
                queue.extend(value_on_skip[n].iter().map(|&c| (c, tainted, Some(n))));
                continue;
            }
            // Open = proven TRUE under the refusal; anything else is the
            // undecidable gate — the defect is unproven from here on.
            let open = verdict == Gate::Open;
            if crate::trifecta::egress_capable(&t.action) {
                let pair = (prompt.to_owned(), t.id.value.clone());
                if open && !tainted && witnessed {
                    // The plainest witness wins whatever the walk order:
                    // `None` (no closed gate crossed) sorts first.
                    let via = skipped.map(|s| wf.tasks[s].value.id.value.clone());
                    let witness = blocked.entry(pair).or_insert_with(|| via.clone());
                    if via < *witness {
                        *witness = via;
                    }
                } else {
                    uncertain.insert(pair);
                }
            }
            queue.extend(children[n].iter().map(|&c| (c, tainted || !open, skipped)));
        }
    }
    // A sink that refuses on a proven route keeps no advisory twin.
    for ((prompt, sink), via) in &blocked {
        scan.findings
            .push(consent_finding(prompt, sink, via.as_deref()));
    }
    for (prompt, sink) in &uncertain {
        if !blocked.contains_key(&(prompt.clone(), sink.clone())) {
            scan.hints.push(consent_hint(prompt, sink));
        }
    }
    scan
}

/// Every outcome of a verb that RAN (`on_error: skip` settles `skipped`)
/// — which one is the verb's business, and never assumed.
const REACHED: u8 = S_ALL & !S_CANCELLED;

/// The two readings of ONE prompt's refusal over the settled-state fold
/// ([`fold_settled`]) — kept apart because they prove opposite things. In
/// both, the gate having settled `success` is the FACT that defines a
/// refusal, whatever its own producers did.
struct Refusal {
    /// MAY — SOME refusal run, over-approximated: a task that can run can
    /// settle anything, and a closed gate is « never `success` » (it
    /// skips, or its `when:` errors and it fails · [`gate_verdict`]).
    /// `S_CANCELLED` alone PROVES the refusal cancels the task; a wider set
    /// is uncertainty, never a witness that it runs.
    may: Settled,
    /// SURE — EVERY refusal run (no operator stop · no timeout), with no
    /// verb outcome assumed: [`gate_certain`] `Closed` certainly settles
    /// `skipped`, `Open` certainly reaches its verb and may then settle
    /// anything ([`REACHED`]), the rest is unknown. So an edge that needs
    /// a success (`after: { x: success }` · a value edge) is certain only
    /// out of the gate itself or of a task that certainly SKIPS — never
    /// out of a verb that merely ran. [`REACHED`] alone is the witness:
    /// certainly admitted, nothing before the verb can error.
    sure: Settled,
}

impl Refusal {
    fn of(wf: &RawWorkflow, edges: &[Edge], topo_waves: &[Vec<usize>], prompt_idx: usize) -> Self {
        let prompt = wf.tasks[prompt_idx].value.id.value.as_str();
        let task = |n: usize| &wf.tasks[n].value;
        let refused = Some((prompt_idx, S_SUCCESS));
        let may = fold_settled(
            wf.tasks.len(),
            edges,
            topo_waves,
            refused,
            |n| match gate_verdict(task(n), prompt) {
                Gate::Closed => S_ALL & !S_SUCCESS,
                _ => S_ALL,
            },
        );
        let sure = fold_settled(
            wf.tasks.len(),
            edges,
            topo_waves,
            refused,
            |n| match gate_certain(task(n), prompt) {
                Gate::Closed => S_SKIPPED,
                Gate::Open => REACHED,
                _ => S_ALL,
            },
        );
        Self { may, sure }
    }
}

/// A confirm-mode `invoke: nika:prompt` — `mode:` ABSENT is confirm
/// (the builtin's runtime default); another literal mode is a different
/// contract (choice answers are strings), a templated mode is not
/// judged (silence, never wrong).
fn is_confirm_prompt(task: &RawTask) -> bool {
    let RawAction::Invoke(inv) = &task.action else {
        return false;
    };
    let Some(tool) = inv.tool() else {
        return false;
    };
    if tool.value != nika_cap::HUMAN_GATE_TOOL {
        return false;
    }
    let mode = inv
        .args
        .as_ref()
        .and_then(|a| a.value.as_object())
        .and_then(|o| o.get("mode"))
        .and_then(serde_json::Value::as_str);
    mode.is_none_or(|m| m == "confirm")
}

/// The blocking row (NEP-0020) — names the sink AND the gate, teaches
/// the affirmative pattern (the human-gated-ship template's shape).
///
/// `via` is the stage the refusal SKIPPED on the proven route (`None` = a
/// route that crosses no closed gate): the same law, a different
/// mechanism — that stage's gate IS affirmative, and it closed nothing for
/// the task that reads the stage's value. The repair is the same house
/// pattern, on the task that holds the effect.
fn consent_finding(prompt: &str, sink: &str, via: Option<&str>) -> ConsentFinding {
    let mechanism = via.map_or_else(
        || {
            format!(
                "task `{sink}` runs on a route from confirm gate `{prompt}` that provably \
                 never gates on the answer — a REFUSED confirm settles success with value \
                 false (the Deny lives in the approval attestation only), so the effect \
                 fires on 'no'"
            )
        },
        |stage| {
            format!(
                "task `{sink}` still reaches its verb when confirm gate `{prompt}` is REFUSED \
                 — the refusal skips `{stage}`, and a skipped task is not a cancelled one: \
                 the value edge out of `{stage}` admits the skip (the binding reads \
                 defined-null · spec 03; `after: {{ {stage}: success }}` on the reader would \
                 cancel it instead), so the gate on `{stage}` closes nothing for `{sink}` and \
                 the effect is ATTEMPTED on 'no' — whatever the verb then makes of a null \
                 input"
            )
        },
    );
    ConsentFinding {
        prompt: prompt.to_owned(),
        sink: sink.to_owned(),
        detail: format!(
            "{mechanism} (NEP-0020 · false triggers exactly zero effects) — fix: bind the \
             answer and gate on it: `with: {{ go: \"${{{{ tasks.{prompt}.output }}}}\" }}` + \
             `when: ${{{{ with.go == true }}}}` (the human-gated-ship pattern)"
        ),
    }
}

/// The advisory row for the UNPROVEN case — the gate may well consume
/// the answer through a shape the fragment cannot evaluate, so the lane
/// teaches rather than refuses (sound, never a false red).
fn consent_hint(prompt: &str, sink: &str) -> Hint {
    Hint {
        kind: "consent",
        code: None,
        task: sink.to_owned(),
        advice: format!(
            "`{sink}` sits on a route from `{prompt}` the checker cannot PROVE consumes the \
             answer (a nested binding · a non-fragment expression) — if it does not, a \
             REFUSED confirm still settles success with value false and the effect fires on \
             'no'; make the consumption provable: \
             `with: {{ go: \"${{{{ tasks.{prompt}.output }}}}\" }}` + \
             `when: ${{{{ with.go == true }}}}` (the human-gated-ship pattern)"
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::check;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report(yaml: &str) -> crate::CheckReport {
        check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    /// The P0-2 fixture: a confirm whose `default:` answers false
    /// UNATTENDED — and a bare state edge carries that refusal straight
    /// into the irreversible exec.
    const BARE: &str = "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    after: { ask: success }\n    exec: { command: [\"git\", \"push\"] }\n";

    /// NEP-0020 · the closure: the PROVEN non-affirmative route is a
    /// NIKA-SEC-014 refusal (the hint of 2026-07-30 escalates) — the
    /// check fails, the code rides every surface (the findings fold ·
    /// the extra-conformance list), and the witness names gate + sink.
    #[test]
    fn the_proven_non_affirmative_route_refuses_with_sec_014() {
        let r = report(BARE);
        assert!(
            !r.is_clean(),
            "a rubber-stamp route fails the check (NEP-0020): {r:?}"
        );
        let codes: Vec<String> = r
            .extra_conformance_codes()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(
            codes.iter().any(|c| c == "NIKA-SEC-014"),
            "the refusal carries its code: {codes:?}"
        );
        let row = r
            .findings
            .iter()
            .find(|f| f.kind == "consent" && f.code.as_deref() == Some("NIKA-SEC-014"))
            .unwrap_or_else(|| panic!("the blocking row in findings[]: {:#?}", r.findings));
        assert_eq!(row.task.as_deref(), Some("push"), "the sink is the witness");
        assert!(
            row.message.contains("ask") && row.message.contains("push"),
            "gate + sink named: {}",
            row.message
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "the proven case is a refusal, not a plea: {:?}",
            r.hints
        );
    }

    /// The bare state edge into an exec — the refusal settles success,
    /// the edge admits it, the push fires. The finding names the sink
    /// and teaches the affirmative pattern.
    #[test]
    fn the_finding_names_gate_sink_and_the_repair() {
        let r = report(BARE);
        assert_eq!(r.consent_findings.len(), 1, "{:?}", r.consent_findings);
        let f = &r.consent_findings[0];
        assert_eq!(f.prompt, "ask");
        assert_eq!(f.sink, "push");
        assert!(
            f.detail.contains("tasks.ask.output") && f.detail.contains("with.go == true"),
            "the affirmative pattern is the repair: {}",
            f.detail
        );
        assert_eq!(crate::ConsentFinding::WIRE_CODE, "NIKA-SEC-014");
    }

    /// An UNDECIDABLE gate stays advisory (sound — no false red): the
    /// answer reaches the `when:` through a NESTED template the consent
    /// fragment cannot evaluate, so the defect is unproven and the lane
    /// speaks the hint, never the code.
    #[test]
    fn an_undecidable_gate_stays_advisory() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { go: \"answer=${{ tasks.ask.output }}\" }\n    when: ${{ with.go == 'answer=true' }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(r.is_clean(), "unproven is advisory, never a refusal: {r:?}");
        assert!(
            r.hints.iter().any(|h| h.kind == "consent"),
            "the uncertain case keeps the hint: {:?}",
            r.hints
        );
    }

    /// The refusal settles SUCCESS — an edge that admits only `failure`
    /// cannot carry it. The walk reads the pass-set: no route, no
    /// finding, no hint (predicate-blind would be a false red here).
    #[test]
    fn a_failure_only_edge_carries_no_refusal() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    after: { ask: failure }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.is_clean(),
            "the failure edge admits nothing of a refusal: {r:?}"
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "not even a hint — there is no route: {:?}",
            r.hints
        );
    }

    /// The same non-affirmative route feeds the risk grade: a human gate
    /// that cannot block is a High signal (P0-2 · risk.rs — the finding
    /// and the hint both lift it).
    #[test]
    fn the_non_affirmative_gate_is_a_high_risk_signal() {
        let r = report(BARE);
        assert!(
            crate::risk_grade(&r) >= crate::RiskGrade::High,
            "a rubber-stamp route lifts the grade: {:?}",
            crate::risk_grade(&r)
        );
    }

    /// The house pattern — `with: go` + `when: ${{ with.go == true }}` —
    /// consumes the answer and proves false on the refusal: silence, and
    /// the grade stays on its own rung.
    #[test]
    fn affirmative_consumption_silences_the_lane() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(r.is_clean(), "the answer is consumed affirmatively: {r:?}");
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "no advisory either: {:?}",
            r.hints
        );
        assert_eq!(
            crate::risk_grade(&r),
            crate::RiskGrade::Supervised,
            "no consent signal, no bump"
        );
    }

    /// `when: false` closes a route by construction — the never-pattern
    /// is the second lawful closure (NEP-0020). (The dead-task lane
    /// `NIKA-DAG-006` flags the `when: false` itself — the CONSENT read
    /// is what stays silent here.)
    #[test]
    fn a_never_run_task_closes_the_route() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  mid:\n    after: { ask: success }\n    when: false\n    infer: { prompt: \"x\", max_tokens: 9 }\n  push:\n    after: { mid: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings.is_empty(),
            "when: false closes the route by construction: {:?}",
            r.consent_findings
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "no advisory either: {:?}",
            r.hints
        );
    }

    /// The bypass case: ONE affirmative route does not discharge the
    /// OTHER — the sink reached over the bare second route refuses.
    #[test]
    fn a_bypass_route_refuses_its_own_sink() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\", \"curl\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"ship?\", default: false }\n  act:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"git\", \"push\"] }\n  ship:\n    after: { ask: success }\n    exec: { command: [\"curl\", \"-X\", \"POST\", \"https://example.com/hook\"] }\n",
        );
        assert_eq!(
            r.consent_findings.len(),
            1,
            "only the bypassed sink refuses: {:?}",
            r.consent_findings
        );
        assert_eq!(r.consent_findings[0].sink, "ship");
    }

    /// A `when:` that reads the prompt's STATUS instead of its answer is
    /// PROVEN open, not affirmative — the refusal settles `"success"`,
    /// so the gate holds under every refusal and the route refuses
    /// (spec fixture `013-consent-status-gate-is-not-consent`).
    #[test]
    fn a_status_gate_is_proven_open_not_consent() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { st: \"${{ tasks.ask.status }}\" }\n    when: ${{ with.st == 'success' }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings
                .iter()
                .any(|f| f.sink == "push" && f.prompt == "ask"),
            "the status is decidable — and it is not consent: {:?}",
            r.consent_findings
        );
    }

    /// The status substitution is honest BOTH ways: a gate on
    /// `with.st == 'failure'` proves FALSE under the refusal — the route
    /// is closed and the lane is silent (a true negative the
    /// output-only evaluator could not see).
    #[test]
    fn a_status_gate_proven_false_closes_the_route() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { st: \"${{ tasks.ask.status }}\" }\n    when: ${{ with.st == 'failure' }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings.is_empty() && !r.hints.iter().any(|h| h.kind == "consent"),
            "proven false is closed: {:?} · {:?}",
            r.consent_findings,
            r.hints
        );
    }

    /// A `when:` that references the answer but cannot be FALSE on a
    /// refusal is proven open (`go == true || go == false` holds under
    /// every answer) — the tautology refuses.
    #[test]
    fn a_when_true_on_false_is_proven_open() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  push:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true || with.go == false }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings.iter().any(|f| f.sink == "push"),
            "a tautology over the answer blocks nothing: {:?}",
            r.consent_findings
        );
    }

    /// The ungated route is transitive: an intermediate pure-compute
    /// task does not launder the consent — the egress sink downstream
    /// refuses.
    #[test]
    fn a_transitive_ungated_route_refuses_the_egress_sink() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  mid:\n    after: { ask: success }\n    infer: { prompt: \"summarize\", max_tokens: 9 }\n  push:\n    after: { mid: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(
            r.consent_findings.len(),
            1,
            "the infer is not egress — only the push refuses: {:?}",
            r.consent_findings
        );
        assert_eq!(r.consent_findings[0].sink, "push");
    }

    /// A BLOCKING prompt (no `default:`) needs the same consumption: the
    /// interactive « no » settles success-with-false exactly like the
    /// unattended default.
    #[test]
    fn a_blocking_confirm_also_refuses_the_bare_route() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\" }\n  push:\n    after: { ask: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings.iter().any(|f| f.sink == "push"),
            "blocking is not affirmative: {:?}",
            r.consent_findings
        );
    }

    /// The nearest gate owns its closure (the approval-batch precedent):
    /// a second confirm on the route cuts the FIRST prompt's walk — the
    /// bare route past the second gate is the second gate's own refusal.
    #[test]
    fn the_nearest_gate_owns_its_closure() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  first:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"one?\", default: false }\n  second:\n    after: { first: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"two?\", default: false }\n  push:\n    after: { second: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(
            r.consent_findings.len(),
            1,
            "one refusal — the nearest gate's own: {:?}",
            r.consent_findings
        );
        assert_eq!(
            r.consent_findings[0].prompt, "second",
            "the second gate owns the closure"
        );
    }

    /// `mode: choice` is OUT OF SCOPE — its answer is a string and the
    /// affirmative pattern differs (`with.answer == 'yes'`); the lane
    /// claims nothing there (silence, never wrong).
    #[test]
    fn a_choice_prompt_is_out_of_scope() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: choice, message: \"push?\", choices: [\"no\", \"yes\"], default: \"no\" }\n  push:\n    after: { ask: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.is_clean(),
            "choice answers are not the confirm contract: {r:?}"
        );
        assert!(
            !r.hints.iter().any(|h| h.kind == "consent"),
            "{:?}",
            r.hints
        );
    }

    // ── the skipped stage (KG01) ─────────────────────────────────────────
    //
    // A stage that consumes the answer AFFIRMATIVELY settles `skipped` on
    // « no » — and a value edge admits a skipped producer. The fixtures
    // below share one opening and differ only in what reads the stage.

    /// The confirm gate + the affirmatively gated stage (not egress).
    const GATED_STAGE: &str = "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  stage:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    infer: { prompt: \"draft\", max_tokens: 9 }\n";

    fn past_the_stage(tail: &str) -> crate::CheckReport {
        report(&format!("{GATED_STAGE}{tail}"))
    }

    fn consent_hints(r: &crate::CheckReport) -> Vec<&str> {
        r.hints
            .iter()
            .filter(|h| h.kind == "consent")
            .map(|h| h.task.as_str())
            .collect()
    }

    /// Silence — no refusal AND no advisory: the route is PROVEN closed.
    fn assert_proven_closed(r: &crate::CheckReport, why: &str) {
        assert!(
            r.consent_findings.is_empty() && consent_hints(r).is_empty(),
            "{why}: {:?} · {:?}",
            r.consent_findings,
            r.hints
        );
    }

    /// The advisory band — the effect is neither proven to fire nor proven
    /// cancelled: a hint on the sink, never the code.
    fn assert_advisory(r: &crate::CheckReport, why: &str) {
        assert!(
            r.consent_findings.is_empty(),
            "{why} — unproven is never a refusal: {:?}",
            r.consent_findings
        );
        assert_eq!(consent_hints(r), vec!["push"], "{why}: {:?}", r.hints);
    }

    /// KG01 · the direct leak: `push` reads the stage's value and nothing
    /// else. On « no » the stage skips, the value edge admits the skip
    /// (defined-null) and the push FIRES — the check must refuse, name the
    /// stage whose gate closed nothing, and carry the code on every surface.
    #[test]
    fn a_value_read_of_a_skipped_stage_refuses() {
        let r = past_the_stage(
            "  push:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(r.consent_findings.len(), 1, "{:?}", r.consent_findings);
        let f = &r.consent_findings[0];
        assert_eq!((f.prompt.as_str(), f.sink.as_str()), ("ask", "push"));
        assert!(
            f.detail.contains("skips `stage`")
                && f.detail.contains("reaches its verb")
                && f.detail.contains("with.go == true"),
            "the skipped stage is the witness, the verb REACHED the claim, the house pattern \
             the repair: {}",
            f.detail
        );
        assert!(!r.is_clean(), "the leak fails the check: {r:?}");
        assert!(
            r.extra_conformance_codes()
                .iter()
                .any(|c| c.to_string() == "NIKA-SEC-014"),
            "the refusal carries its code"
        );
        assert!(consent_hints(&r).is_empty(), "proven — not a plea");
    }

    /// NO VERB IS ASSUMED TO SUCCEED. The laundering hop reaches the push
    /// only if `mid` SUCCEEDS on the skipped stage's defined-null — likely
    /// (measured on a mock seat: the push fires), never proven: a `mid`
    /// that fails on null cancels the push. Not cancelled in every run, not
    /// reached in every run — advisory, and the hint teaches the same
    /// repair. (The hole is REPORTED, not closed: see the lane's limits.)
    #[test]
    fn a_laundering_hop_is_advisory_its_success_is_never_assumed() {
        let r = past_the_stage(
            "  mid:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    infer: { prompt: \"summarize\", max_tokens: 9 }\n  push:\n    after: { mid: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "the hop's success is an assumption");
    }

    /// The same hop over a VALUE edge: it admits `mid` skipped or
    /// succeeded, not failed — still an outcome nobody proved.
    #[test]
    fn a_laundering_hop_over_a_value_edge_is_advisory_too() {
        let r = past_the_stage(
            "  mid:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    infer: { prompt: \"summarize\", max_tokens: 9 }\n  push:\n    with: { m: \"${{ tasks.mid.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "a value edge does not admit a failed hop");
    }

    /// Two skipped stages in a row: the nearest one is the witness.
    #[test]
    fn a_chain_of_skipped_stages_still_refuses() {
        let r = past_the_stage(
            "  stage2:\n    with: { go: \"${{ tasks.ask.output }}\", s: \"${{ tasks.stage.output }}\" }\n    when: ${{ with.go == true }}\n    infer: { prompt: \"polish\", max_tokens: 9 }\n  push:\n    with: { staged: \"${{ tasks.stage2.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(r.consent_findings.len(), 1, "{:?}", r.consent_findings);
        assert!(
            r.consent_findings[0].detail.contains("skips `stage2`"),
            "{}",
            r.consent_findings[0].detail
        );
    }

    /// A fold carries its members' values too: `${{ group.pages }}` runs
    /// whatever its members settled — a skipped member included.
    #[test]
    fn a_fold_over_a_skipped_member_refuses() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  stage:\n    group: pages\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    infer: { prompt: \"draft\", max_tokens: 9 }\n  push:\n    with: { all: \"${{ group.pages }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert!(
            r.consent_findings
                .iter()
                .any(|f| f.sink == "push" && f.detail.contains("skips `stage`")),
            "{:?}",
            r.consent_findings
        );
    }

    /// An INDEPENDENT prerequisite that must SUCCEED is the same unproven
    /// outcome (measured: in the ordinary refusal run it succeeds and the
    /// push fires) — advisory, never the code.
    #[test]
    fn an_independent_success_prerequisite_is_advisory() {
        let r = past_the_stage(
            "  other:\n    infer: { prompt: \"x\", max_tokens: 9 }\n  push:\n    after: { other: success }\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "`other` succeeding is an assumption");
    }

    /// …while a prerequisite the reader only waits to SETTLE assumes
    /// nothing: `terminal` admits every outcome, so the push is admitted in
    /// every refusal run and the witness stands.
    #[test]
    fn an_outcome_agnostic_prerequisite_keeps_the_witness() {
        let r = past_the_stage(
            "  other:\n    infer: { prompt: \"x\", max_tokens: 9 }\n  push:\n    after: { other: terminal }\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(r.consent_findings.len(), 1, "{:?}", r.consent_findings);
        assert_eq!(r.consent_findings[0].sink, "push");
    }

    /// The gate having settled `success` is the FACT that defines a
    /// refusal — it is not doubted because the gate itself waited for a
    /// `build` nobody can promise: on every « no » the gate DID run.
    #[test]
    fn the_refused_gate_is_a_fact_whatever_it_waited_for() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  build:\n    infer: { prompt: \"x\", max_tokens: 9 }\n  ask:\n    after: { build: success }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  stage:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    infer: { prompt: \"draft\", max_tokens: 9 }\n  push:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(r.consent_findings.len(), 1, "{:?}", r.consent_findings);
        assert_eq!(r.consent_findings[0].sink, "push");
    }

    /// GATE-v2 is an AND, and two edges from ONE producer intersect:
    /// `{success, skipped}` ∩ `{success}` = `{success}`. The skipped stage
    /// CANCELS this reader on every refusal — a walk that followed the
    /// value edge alone would red a protected graph.
    #[test]
    fn a_success_edge_on_the_same_producer_cancels_the_reader() {
        let r = past_the_stage(
            "  push:\n    after: { stage: success }\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_proven_closed(&r, "the same-producer AND cancels the reader");
    }

    /// The cancellation is transitive: `mid` is cancelled by its own
    /// success edge, and a value edge does not admit `cancelled` — so the
    /// push that reads `mid` is cancelled too.
    #[test]
    fn a_cancelled_reader_cancels_its_own_readers() {
        let r = past_the_stage(
            "  mid:\n    after: { stage: success }\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    infer: { prompt: \"summarize\", max_tokens: 9 }\n  push:\n    with: { m: \"${{ tasks.mid.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_proven_closed(&r, "a blocked join stays blocked downstream");
    }

    /// The control-only reader was always closed — it stays closed.
    #[test]
    fn a_control_only_success_edge_stays_closed() {
        let r = past_the_stage(
            "  push:\n    after: { stage: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_proven_closed(&r, "skipped is outside {success}");
    }

    /// A guard on ANOTHER producer closes the reader just as well, when
    /// that producer is itself proven never to succeed on « no ».
    #[test]
    fn a_closed_guard_on_another_producer_cancels_the_reader() {
        let r = past_the_stage(
            "  approved:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    infer: { prompt: \"x\", max_tokens: 9 }\n  push:\n    after: { approved: success }\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_proven_closed(&r, "`approved` never succeeds on a refusal");
    }

    /// The house pattern on the reader itself is the repair the finding
    /// teaches — it closes the route whatever the reader also reads.
    #[test]
    fn the_house_pattern_on_the_reader_closes_it() {
        let r = past_the_stage(
            "  push:\n    with: { go: \"${{ tasks.ask.output }}\", staged: \"${{ tasks.stage.output }}\" }\n    when: ${{ with.go == true }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_proven_closed(&r, "the reader gates on the answer itself");
    }

    /// A producer that never ran unwinds nothing (spec 03 §unwind): the
    /// stage's own cleanup unit reads its value and never fires on « no ».
    #[test]
    fn the_cleanup_of_a_skipped_stage_never_fires() {
        let r = past_the_stage(
            "  push:\n    after: { stage: unwind }\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_proven_closed(&r, "a skipped producer unwinds nothing");
    }

    /// THE SLICE BOUNDARY, pinned so it moves on purpose: the explicit
    /// `after: { x: skipped }` handler and a `.status` observation carry
    /// no value of the stage — this lane does not walk them (silence here
    /// is scope, not a proof that they are harmless).
    #[test]
    fn a_skip_handler_and_a_status_read_stay_out_of_this_slice() {
        let r = past_the_stage(
            "  push:\n    after: { stage: skipped }\n    exec: { command: [\"git\", \"push\"] }\n  tell:\n    with: { st: \"${{ tasks.stage.status }}\" }\n    exec: { command: [\"git\", \"status\"] }\n",
        );
        assert_proven_closed(&r, "out of the value-consumer slice");
    }

    /// NOT-PROVEN-CANCELLED IS NOT PROVEN-OPEN. `idle` may fail, so the
    /// push that waits for its failure is not provably cancelled (measured
    /// on the engine: when `idle` fails the effect fires on « no ») — and
    /// in the ordinary refusal run `idle` succeeds and the push IS
    /// cancelled, so no witness proves the leak either. Advisory.
    #[test]
    fn a_prerequisite_that_may_fail_is_advisory_never_proof() {
        let r = past_the_stage(
            "  idle:\n    infer: { prompt: \"x\", max_tokens: 9 }\n  push:\n    after: { idle: failure }\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "an uncertain prerequisite");
    }

    /// A guard the fragment cannot decide on the reader (the null check on
    /// the skipped value) is the undecidable gate it always was.
    #[test]
    fn an_undecided_guard_on_the_reader_stays_advisory() {
        let r = past_the_stage(
            "  push:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    when: ${{ with.staged != null }}\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "a non-fragment guard");
    }

    /// A `for_each` reader is not a proven runner: a null or empty
    /// collection never iterates (measured: a fan-out over the skipped
    /// stage's null FAILS before any iteration) — the SURE reading never
    /// assumes it.
    #[test]
    fn a_fan_out_reader_is_not_a_proven_witness() {
        let r = past_the_stage(
            "  push:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    for_each: { items: \"${{ with.staged }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "a fan-out may never iterate");
    }

    /// A Kleene-FALSE gate is « never `success` », not « skips ». The
    /// runtime evaluates the LEFT of `&&` first: the cross-type `>` errors
    /// (`NIKA-VAR-006`), the stage FAILS, and a value edge does not admit
    /// `failure` — the reader is cancelled and nothing fires (measured on
    /// the engine). Not a witness: advisory, never the code.
    #[test]
    fn a_gate_that_errors_before_it_decides_is_not_a_proven_skip() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  stage:\n    with: { go: \"${{ tasks.ask.output }}\", report: \"shown-report\" }\n    when: ${{ with.report > 0 && with.go == true }}\n    infer: { prompt: \"draft\", max_tokens: 9 }\n  push:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "the gate may error instead of skipping");
    }

    /// The SAME two operands the other way round: the false answer is on
    /// the left, the runtime never evaluates the right — the stage
    /// certainly skips, and the value read is the proven leak again.
    #[test]
    fn a_false_left_operand_is_a_proven_skip() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  stage:\n    with: { go: \"${{ tasks.ask.output }}\", report: \"shown-report\" }\n    when: ${{ with.go == true && with.report > 0 }}\n    infer: { prompt: \"draft\", max_tokens: 9 }\n  push:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(r.consent_findings.len(), 1, "{:?}", r.consent_findings);
        assert_eq!(r.consent_findings[0].sink, "push");
    }

    /// `==` across two classes is an ERROR at run, never `false`: the
    /// stage fails, the reader is cancelled. Kleene reads the gate closed
    /// (it does close the success routes) — it is no proof of a skip.
    #[test]
    fn a_cross_type_gate_is_not_a_proven_skip() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  stage:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == 'yes' }}\n    infer: { prompt: \"draft\", max_tokens: 9 }\n  push:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "a cross-type compare errors at run");
    }

    /// Bindings are evaluated BEFORE the verb: a reader that navigates the
    /// skipped stage's defined-null may fail there and never reach its
    /// effect — reaching the task is not reaching the verb.
    #[test]
    fn a_reader_whose_binding_navigates_is_not_a_proven_witness() {
        let r = past_the_stage(
            "  push:\n    with: { path: \"${{ tasks.stage.output.path }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "a binding may error before the verb");
    }

    /// A prerequisite whose success NO refusal run can have: the gate is
    /// only ever asked when `build` FAILED, so `after: { build: success }`
    /// cancels the push on every real « no ». The lane does not derive
    /// that (it never reads the gate's own admission backwards) — and it
    /// does not need to: it never assumes `build` succeeds either. No
    /// witness, no refusal; an « everything succeeds » world would have
    /// been a false red here.
    #[test]
    fn a_success_no_refusal_run_can_have_is_never_assumed() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  build:\n    infer: { prompt: \"x\", max_tokens: 9 }\n  ask:\n    after: { build: failure }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push anyway?\", default: false }\n  stage:\n    with: { go: \"${{ tasks.ask.output }}\" }\n    when: ${{ with.go == true }}\n    infer: { prompt: \"draft\", max_tokens: 9 }\n  push:\n    after: { build: success }\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "`build: success` is never assumed");
    }

    /// An UNDECIDED stage was advisory before this slice and stays so —
    /// the taint rides the route exactly as it did.
    #[test]
    fn an_undecided_stage_stays_advisory() {
        let r = report(
            "nika: t\npermits:\n  exec: [\"git\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"push?\", default: false }\n  stage:\n    with: { go: \"answer=${{ tasks.ask.output }}\" }\n    when: ${{ with.go == 'answer=true' }}\n    infer: { prompt: \"draft\", max_tokens: 9 }\n  push:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_advisory(&r, "an undecidable stage");
    }

    /// A closer confirm gate still owns its closure past a skipped stage:
    /// the first gate's walk stops at it, the bare route beyond is the
    /// SECOND gate's own refusal.
    #[test]
    fn a_closer_gate_owns_the_route_past_a_skipped_stage() {
        let r = past_the_stage(
            "  second:\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"sure?\", default: false }\n  push:\n    after: { second: success }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(r.consent_findings.len(), 1, "{:?}", r.consent_findings);
        assert_eq!(r.consent_findings[0].prompt, "second");
    }

    /// A sink ALSO reached over a route that crosses no closed gate keeps
    /// the verdict and the wording it always had — the plainest witness
    /// wins, whatever order the walk met the two routes in.
    #[test]
    fn the_plain_route_keeps_its_own_witness() {
        let r = past_the_stage(
            "  push:\n    after: { ask: success }\n    with: { staged: \"${{ tasks.stage.output }}\" }\n    exec: { command: [\"git\", \"push\"] }\n",
        );
        assert_eq!(r.consent_findings.len(), 1, "{:?}", r.consent_findings);
        let detail = &r.consent_findings[0].detail;
        assert!(
            detail.contains("provably never gates on the answer") && !detail.contains("skips"),
            "{detail}"
        );
    }
}
