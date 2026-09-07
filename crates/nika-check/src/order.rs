// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The order law (spec `10-authority.md` §the unconditional laws ·
//! `NIKA-SEC-015`) — **content the workflow did not author must not
//! reach a shell**.
//!
//! An `exec:` task that sits transitively downstream of a net-effecting
//! task over the derived graph is refused. No block declares this and
//! none can disable it: it is the half of the dead `policy:` family that
//! survived, and it survived UNCONDITIONAL. The engine never implemented
//! it while it was `require.net_before_exec`, because a rule nobody
//! declared was a rule nobody ran.
//!
//! **The trifecta does not subsume it.** That law's first leg wants a
//! non-empty `permits.fs.read`; a file that fetches and then shells with
//! no private read at all clears the leg and walks straight through
//! (`core/order/001-net-before-exec-violation` is exactly that file).
//!
//! **Cost, measured by the spec before the ruling** · 194 `exec:` tasks
//! across the shipped corpus, **1** refused — and that one is already a
//! declared `check-reject`. Zero green files pay for this.
//!
//! The witness is the PATH, not the pair. A refusal that only named the
//! two ends would leave the author hunting for which edge carried the
//! content; naming every hop makes the route the thing you fix.

use nika_schema::raw::{RawAction, RawWorkflow};

use crate::analyzer::{Edge, EdgeKind, Route};

/// One refused route — a net-effecting source, an `exec:` sink, and the
/// path between them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[non_exhaustive]
pub struct OrderFinding {
    /// The net-effecting task the content enters at.
    pub source: String,
    /// The `exec:` task it reaches.
    pub sink: String,
    /// The human row — the law, the route, the repair.
    pub detail: String,
}

impl OrderFinding {
    /// The ONE wire code (spec 10 · the unconditional order law).
    pub const WIRE_CODE: &'static str = "NIKA-SEC-015";
}

/// Whether this action reaches the network — `nika:fetch` · `nika:notify`
/// over a webhook · any URL-reaching builtin. The ONE effect table
/// answers; an `mcp:` server is fail-closed, as everywhere else.
fn net_effecting(action: &RawAction) -> bool {
    let RawAction::Invoke(inv) = action else {
        return false;
    };
    let Some(tool) = inv.tool() else {
        return false; // a child-workflow call — spec 14 owns its boundary
    };
    let id = tool.value.as_str();
    if id.starts_with("mcp:") {
        return true;
    }
    let args = inv.args.as_ref().map(|a| &a.value);
    matches!(
        nika_cap::builtin_effect(id, args),
        Some(nika_cap::BuiltinEffect::Net { .. })
    )
}

/// Judge the order law over the derived graph.
///
/// The graph is EVERY derived edge — `with:` data edges and `after:`
/// control edges alike, the unwind attachment included. An unwind edge
/// stays out of the precedence graph because it carries no ORDER, but it
/// carries CONTENT: a cleanup reads its producer, so a cleanup that
/// shells after a fetch is the same defect wearing a different key. That
/// distinction is exactly the blind spot the `on_finally` rewrite closed
/// in the IFC pass; it is not re-opened here.
#[must_use]
pub(crate) fn scan_order(wf: &RawWorkflow, edges: &[Edge]) -> Vec<OrderFinding> {
    let sources: Vec<usize> = (0..wf.tasks.len())
        .filter(|&i| net_effecting(&wf.tasks[i].value.action))
        .collect();
    if sources.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for source in sources {
        for r in crate::analyzer::witness_routes(edges, wf.tasks.len(), source) {
            if !matches!(wf.tasks[r.sink].value.action, RawAction::Exec(_)) {
                continue;
            }
            let route = r
                .hops
                .iter()
                .map(|&i| format!("`{}`", wf.tasks[i].value.id.value))
                .collect::<Vec<_>>()
                .join(" → ");
            let (from, to) = (
                wf.tasks[source].value.id.value.clone(),
                wf.tasks[r.sink].value.id.value.clone(),
            );
            let (claim, cut) = route_claim(wf, &r, &from, &to);
            out.push(OrderFinding {
                detail: format!(
                    "the order law · {claim} — {route}. Content the workflow did \
                     not author must not reach a shell: this holds with no block \
                     declaring it and none able to disable it. Fix: do the work \
                     in a builtin (`nika:jq` · `nika:grep`) instead of a shell, \
                     or {cut}."
                ),
                source: from,
                sink: to,
            });
        }
    }
    out
}

/// The claim and the repair, matched to the edge kinds the route CROSSED.
///
/// « `X` shells on content `Y` fetched » is a DATA-FLOW claim, and the
/// walk that produced it is a reachability walk over data ∪ control
/// edges. Measured on 0.118.7 by a persona wave, verbatim: « The card
/// said `measure` "shells on content `grab` fetched". I stripped the
/// argv to `["date","-u"]` (no fetched value anywhere, no file read) and
/// it STILL refused. `nika explain` was accurate where the card was not
/// … The card's offered fix ("cut the route so the fetched value never
/// reaches `measure`") is unactionable, because no fetched value reached
/// it — the thing to delete was the `after:` edge, which the card never
/// names. »
///
/// The VERDICT is untouched: the order law is unconditional over the
/// derived graph, exactly as `nika explain NIKA-SEC-015` always said.
/// Only the sentence changes, and only where it overclaimed. The route
/// arrives from [`crate::analyzer::witness_routes`], which hands back a
/// value route wherever one exists — so the second arm's « no `with:`
/// chain carries it » is a fact about the whole graph, not about the one
/// route that happened to be shortest.
///
/// Neither arm claims the value CANNOT arrive: the law is unconditional
/// precisely because content also travels where no edge does (a file the
/// fetch wrote, an environment the shell inherits). The sentence says
/// what the graph shows, and the repair names a hop the author wrote.
fn route_claim(wf: &RawWorkflow, r: &Route, from: &str, to: &str) -> (String, String) {
    let Some(hop) = r.kinds.iter().position(|k| !k.carries_value()) else {
        return (
            format!("`{to}` shells on content `{from}` fetched"),
            format!("cut the route so the fetched value never reaches `{to}`"),
        );
    };
    let mut over: Vec<&str> = Vec::new();
    if r.kinds.iter().any(|k| k.carries_value()) {
        over.push("`with:` data edges");
    }
    if r.kinds.iter().any(|k| matches!(k, EdgeKind::Control(_))) {
        over.push("`after:` control edges");
    }
    if r.kinds
        .iter()
        .any(|k| !k.carries_value() && !matches!(k, EdgeKind::Control(_)))
    {
        over.push("observation edges (status · error · timing)");
    }
    let word = match r.kinds.get(hop) {
        Some(EdgeKind::Control(_)) => "`after:`",
        _ => "observation",
    };
    let named = |at: Option<&usize>| {
        at.and_then(|&i| wf.tasks.get(i))
            .map_or_else(|| "?".to_owned(), |t| t.value.id.value.clone())
    };
    let (a, b) = (named(r.hops.get(hop)), named(r.hops.get(hop + 1)));
    (
        format!(
            "`{to}` sits downstream of `{from}` over {} · no `with:` chain carries `{from}`'s output into `{to}`",
            over.join(" and ")
        ),
        format!("cut the {word} edge `{a}` → `{b}`"),
    )
}

#[cfg(test)]
mod tests {
    use crate::check;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn report(yaml: &str) -> crate::CheckReport {
        check(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses"))
    }

    const PERMITS: &str = "permits:\n  exec: [\"echo\"]\n  net: { http: [\"example.com\"] }\n  \
                           tools: [\"nika:fetch\"]\n";

    /// The spec's own violation fixture (`core/order/001`) — the exec
    /// binds the fetched body, so the route exists and the law fires.
    #[test]
    fn an_exec_downstream_of_a_fetch_is_refused() {
        let r = report(&format!(
            "nika: t\n{PERMITS}tasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      \
             args: {{ url: \"https://example.com/data\" }}\n  act:\n    \
             with: {{ body: \"${{{{ tasks.fetch_page.output }}}}\" }}\n    \
             exec: {{ command: [\"echo\", \"${{{{ with.body }}}}\"] }}\n"
        ));
        assert_eq!(r.order_findings.len(), 1, "{:?}", r.order_findings);
        let f = &r.order_findings[0];
        assert_eq!((f.source.as_str(), f.sink.as_str()), ("fetch_page", "act"));
        assert!(
            f.detail.contains("`fetch_page` → `act`"),
            "the PATH is the witness: {}",
            f.detail
        );
        assert!(!r.is_clean(), "an unconditional law refuses the file");
    }

    /// The spec's clean twin (`core/order/002`) — the same two tasks with
    /// NO edge between them. Nothing flows, so nothing is refused.
    #[test]
    fn an_independent_exec_is_clean() {
        let r = report(&format!(
            "nika: t\n{PERMITS}tasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      \
             args: {{ url: \"https://example.com/data\" }}\n  act:\n    \
             exec: {{ command: [\"echo\", \"independent\"] }}\n"
        ));
        assert!(r.order_findings.is_empty(), "{:?}", r.order_findings);
    }

    /// A control edge carries the law as surely as a data edge: the spec
    /// names `with:` data edges ∪ `after:` control edges, and an `after:`
    /// alone is enough to put the shell downstream of the fetch.
    ///
    /// Measured on 0.118.7 by a persona wave, verbatim: « The card said
    /// `measure` "shells on content `grab` fetched". I stripped the argv
    /// to `["date","-u"]` (no fetched value anywhere, no file read) and
    /// it STILL refused. … The card's offered fix … is unactionable,
    /// because no fetched value reached it — the thing to delete was the
    /// `after:` edge, which the card never names. » The refusal stands;
    /// the SENTENCE now names the edge kinds it walked, and the repair
    /// names the hop.
    #[test]
    fn a_bare_control_edge_carries_it_too() {
        let r = report(&format!(
            "nika: t\n{PERMITS}tasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      \
             args: {{ url: \"https://example.com/data\" }}\n  act:\n    \
             after: {{ fetch_page: success }}\n    exec: {{ command: [\"echo\", \"hi\"] }}\n"
        ));
        assert_eq!(r.order_findings.len(), 1, "{:?}", r.order_findings);
        let d = &r.order_findings[0].detail;
        assert!(
            d.contains("`act` sits downstream of `fetch_page` over `after:` control edges"),
            "the claim names what the walk proved: {d}"
        );
        assert!(
            d.contains("no `with:` chain carries `fetch_page`'s output into `act`"),
            "and what the graph did NOT show: {d}"
        );
        assert!(
            d.contains("cut the `after:` edge `fetch_page` → `act`"),
            "the repair names the hop to delete: {d}"
        );
        assert!(
            !d.contains("shells on content"),
            "a data-flow claim the judge never made: {d}"
        );
        assert!(
            !r.is_clean(),
            "the verdict is untouched — the law is unconditional"
        );
    }

    /// The measured file itself: a fetch whose output reaches a WRITE by
    /// `with:`, and a shell ordered after the write by `after:` — argv
    /// `["date","-u"]`, no fetched value anywhere in it. The claim names
    /// both kinds it crossed, and the repair names the hop the author
    /// wrote and then deleted: `save` → `measure`.
    #[test]
    fn a_mixed_route_names_both_kinds_and_the_after_hop_to_cut() {
        let r = report(&format!(
            "nika: t\n{PERMITS}tasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      \
             args: {{ url: \"https://example.com/data\" }}\n  save:\n    \
             with: {{ body: \"${{{{ tasks.grab.output }}}}\" }}\n    \
             invoke:\n      tool: \"nika:write\"\n      \
             args: {{ path: \"./out.txt\", content: \"${{{{ with.body }}}}\" }}\n  measure:\n    \
             after: {{ save: success }}\n    exec: {{ command: [\"date\", \"-u\"] }}\n"
        ));
        assert_eq!(r.order_findings.len(), 1, "{:?}", r.order_findings);
        let d = &r.order_findings[0].detail;
        assert!(
            d.contains(
                "`measure` sits downstream of `grab` over `with:` data edges and `after:` control edges"
            ),
            "both kinds, in the order the route crossed them: {d}"
        );
        assert!(
            d.contains("cut the `after:` edge `save` → `measure`"),
            "the hop the reader deleted, named: {d}"
        );
        assert!(!d.contains("shells on content"), "{d}");
    }

    /// The under-claim the preference forbids. `act` sits one `after:`
    /// hop from the fetch AND two `with:` hops from it; the shortest
    /// route is the control one, and describing THAT would say « no
    /// `with:` chain carries it » of a file where one does. The witness
    /// walk hands back the value route, so the strong claim holds.
    #[test]
    fn a_value_route_outranks_a_shorter_control_shortcut() {
        let r = report(&format!(
            "nika: t\n{PERMITS}tasks:\n  grab:\n    invoke:\n      tool: \"nika:fetch\"\n      \
             args: {{ url: \"https://example.com/data\" }}\n  clean:\n    \
             with: {{ body: \"${{{{ tasks.grab.output }}}}\" }}\n    \
             invoke:\n      tool: \"nika:jq\"\n      \
             args: {{ input: \"${{{{ with.body }}}}\", expression: \".\" }}\n  act:\n    \
             after: {{ grab: success }}\n    \
             with: {{ v: \"${{{{ tasks.clean.output }}}}\" }}\n    \
             exec: {{ command: [\"echo\", \"${{{{ with.v }}}}\"] }}\n"
        ));
        assert_eq!(r.order_findings.len(), 1, "{:?}", r.order_findings);
        let d = &r.order_findings[0].detail;
        assert!(
            d.contains("`act` shells on content `grab` fetched")
                && d.contains("`grab` → `clean` → `act`"),
            "the longer VALUE route is the witness, not the one-hop `after:`: {d}"
        );
        assert!(
            !d.contains("no `with:` chain"),
            "the graph holds one — the negative would be false here: {d}"
        );
    }

    /// The data route KEEPS its claim: every hop a `with:` value edge,
    /// so the fetched bytes really do land in the argv. The two prose
    /// arms are the same judge, told apart by the route.
    #[test]
    fn a_data_route_still_says_it_shells_on_the_fetched_content() {
        let r = report(&format!(
            "nika: t\n{PERMITS}tasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      \
             args: {{ url: \"https://example.com/data\" }}\n  act:\n    \
             with: {{ body: \"${{{{ tasks.fetch_page.output }}}}\" }}\n    \
             exec: {{ command: [\"echo\", \"${{{{ with.body }}}}\"] }}\n"
        ));
        let d = &r.order_findings[0].detail;
        assert!(
            d.contains("`act` shells on content `fetch_page` fetched"),
            "{d}"
        );
        assert!(
            d.contains("cut the route so the fetched value never reaches `act`"),
            "{d}"
        );
    }

    /// An observation edge reads the RECORD (`.status` · `.error` ·
    /// timings), which the engine authors — not the bytes the fetch
    /// returned. It is neither a data edge nor an `after:` entry, so the
    /// claim and the repair say so rather than naming an edge that is
    /// not there.
    #[test]
    fn an_observation_route_names_the_observation_never_an_after_edge() {
        let r = report(&format!(
            "nika: t\n{PERMITS}tasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      \
             args: {{ url: \"https://example.com/data\" }}\n  act:\n    \
             with: {{ s: \"${{{{ tasks.fetch_page.status }}}}\" }}\n    \
             exec: {{ command: [\"echo\", \"${{{{ with.s }}}}\"] }}\n"
        ));
        assert_eq!(r.order_findings.len(), 1, "{:?}", r.order_findings);
        let d = &r.order_findings[0].detail;
        assert!(
            d.contains("over observation edges (status · error · timing)")
                && d.contains("cut the observation edge `fetch_page` → `act`"),
            "{d}"
        );
        assert!(
            !d.contains("`after:`"),
            "no edge kind the file never wrote: {d}"
        );
    }

    /// The blind spot, closed by construction: an unwind edge stays out
    /// of the PRECEDENCE graph, never out of the content graph. A cleanup
    /// that shells after a fetch is the same defect in a different key.
    #[test]
    fn an_unwind_cleanup_that_shells_is_the_same_defect() {
        let r = report(&format!(
            "nika: t\n{PERMITS}tasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      \
             args: {{ url: \"https://example.com/data\" }}\n  sweep:\n    \
             after: {{ fetch_page: unwind }}\n    exec: {{ command: [\"echo\", \"bye\"] }}\n"
        ));
        assert_eq!(r.order_findings.len(), 1, "{:?}", r.order_findings);
        assert_eq!(r.order_findings[0].sink, "sweep");
    }

    /// The route is TRANSITIVE — an `infer:` in the middle launders
    /// nothing. The witness names every hop.
    #[test]
    fn the_route_is_transitive_and_the_witness_names_every_hop() {
        let r = report(&format!(
            "nika: t\nmodel: mock/echo\n{PERMITS}tasks:\n  fetch_page:\n    invoke:\n      \
             tool: \"nika:fetch\"\n      args: {{ url: \"https://example.com/data\" }}\n  \
             summarize:\n    with: {{ page: \"${{{{ tasks.fetch_page.output }}}}\" }}\n    \
             infer: {{ prompt: \"tldr ${{{{ with.page }}}}\", max_tokens: 9 }}\n  act:\n    \
             with: {{ s: \"${{{{ tasks.summarize.output }}}}\" }}\n    \
             exec: {{ command: [\"echo\", \"${{{{ with.s }}}}\"] }}\n"
        ));
        assert_eq!(r.order_findings.len(), 1, "{:?}", r.order_findings);
        assert!(
            r.order_findings[0]
                .detail
                .contains("`fetch_page` → `summarize` → `act`"),
            "{}",
            r.order_findings[0].detail
        );
    }

    /// A workflow with no net effect at all makes no claim — the lane
    /// leaves before it walks anything.
    #[test]
    fn a_file_that_never_reaches_the_network_is_silent() {
        let r = report(
            "nika: t\npermits: { exec: [\"echo\"] }\ntasks:\n  \
             act:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        );
        assert!(r.order_findings.is_empty(), "{:?}", r.order_findings);
    }

    /// UNCONDITIONAL means unconditional: with NO `permits:` block the
    /// law still fires. (The file has its own `NIKA-AUTH-006` to answer
    /// for; that is a different judge over the same body.)
    #[test]
    fn no_permits_block_does_not_buy_a_pass() {
        let r = report(
            "nika: t\ntasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      \
             args: { url: \"https://example.com/data\" }\n  act:\n    \
             with: { body: \"${{ tasks.fetch_page.output }}\" }\n    \
             exec: { command: [\"echo\", \"${{ with.body }}\"] }\n",
        );
        assert_eq!(r.order_findings.len(), 1, "{:?}", r.order_findings);
    }

    #[test]
    fn the_wire_code_is_the_spec_row() {
        assert_eq!(crate::OrderFinding::WIRE_CODE, "NIKA-SEC-015");
    }
}
