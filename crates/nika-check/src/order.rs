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

use std::collections::{BTreeMap, VecDeque};

use nika_schema::raw::{RawAction, RawWorkflow};

use crate::analyzer::Edge;

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
        for (sink, path) in reachable_execs(wf, edges, source) {
            let route = path.join(" → ");
            let (from, to) = (
                wf.tasks[source].value.id.value.clone(),
                wf.tasks[sink].value.id.value.clone(),
            );
            out.push(OrderFinding {
                detail: format!(
                    "the order law · `{to}` shells on content `{from}` fetched — \
                     {route}. Content the workflow did not author must not reach \
                     a shell: this holds with no block declaring it and none able \
                     to disable it. Fix: do the work in a builtin \
                     (`nika:jq` · `nika:grep`) instead of a shell, or cut the \
                     route so the fetched value never reaches `{to}`."
                ),
                source: from,
                sink: to,
            });
        }
    }
    out
}

/// Every `exec:` task reachable from `source`, each with the shortest
/// route to it (BFS · the first path found IS the shortest, and a
/// shortest witness is the one an author can read).
fn reachable_execs(wf: &RawWorkflow, edges: &[Edge], source: usize) -> Vec<(usize, Vec<String>)> {
    let mut adjacency: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for e in edges {
        adjacency.entry(e.from).or_default().push(e.to);
    }
    let mut came_from: BTreeMap<usize, usize> = BTreeMap::new();
    let mut seen = vec![false; wf.tasks.len()];
    let mut queue = VecDeque::from([source]);
    seen[source] = true;
    let mut hits = Vec::new();
    while let Some(node) = queue.pop_front() {
        for &next in adjacency.get(&node).into_iter().flatten() {
            if seen.get(next).copied().unwrap_or(true) {
                continue;
            }
            seen[next] = true;
            came_from.insert(next, node);
            if matches!(wf.tasks[next].value.action, RawAction::Exec(_)) {
                hits.push((next, route_to(wf, &came_from, source, next)));
            }
            queue.push_back(next);
        }
    }
    hits
}

/// The route as the author reads it — `a → b → c`, source first.
fn route_to(
    wf: &RawWorkflow,
    came_from: &BTreeMap<usize, usize>,
    source: usize,
    sink: usize,
) -> Vec<String> {
    let mut rev = vec![sink];
    let mut at = sink;
    while at != source {
        let Some(&prev) = came_from.get(&at) else {
            break;
        };
        rev.push(prev);
        at = prev;
    }
    rev.reverse();
    rev.into_iter()
        .map(|i| format!("`{}`", wf.tasks[i].value.id.value))
        .collect()
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
    #[test]
    fn a_bare_control_edge_carries_it_too() {
        let r = report(&format!(
            "nika: t\n{PERMITS}tasks:\n  fetch_page:\n    invoke:\n      tool: \"nika:fetch\"\n      \
             args: {{ url: \"https://example.com/data\" }}\n  act:\n    \
             after: {{ fetch_page: success }}\n    exec: {{ command: [\"echo\", \"hi\"] }}\n"
        ));
        assert_eq!(r.order_findings.len(), 1, "{:?}", r.order_findings);
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
