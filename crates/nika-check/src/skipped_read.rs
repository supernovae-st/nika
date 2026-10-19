// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The skipped-read hint — a read that steps into the value of a task that
//! can be skipped (`tasks.X.output.url` · `with.out.url` past a `when:` that
//! lets null through): the skipped value reads null and the step raises
//! `NIKA-VAR-001`/`NIKA-VAR-006` at run, from a binding that runs BEFORE
//! the task's own `when:` could guard it. The judgment is the analyzer's
//! ([`crate::analyzer::skipped_reads`]); this lane names the kind.
//!
//! A HINT, not a finding: a producer that CAN skip is not one that will, the
//! spec makes no such read illegal, and a new refusal would need a new
//! normative code — the advisory channel says it without one. The line the
//! operator reads before a run still counts it (`1 hint`), so « run ready
//! ✔ » is no longer the whole story on such a file.

use nika_schema::raw::RawWorkflow;

use crate::analyzer::Edge;
use crate::hints::Hint;

/// The hint kind (`nika explain skipped-read` · the catalog row).
pub(crate) const KIND: &str = "skipped-read";

/// The hints over a checked graph — none when the graph did not conform
/// (nothing to fold) or past the analysis cap, where the per-producer fold
/// shares the `DoS` floor and the DAG read's stated miss names the skip.
pub(crate) fn hints(
    wf: &RawWorkflow,
    conforms: bool,
    edges: &[Edge],
    topo_waves: &[Vec<usize>],
) -> Vec<Hint> {
    if !conforms || wf.tasks.len() > crate::analysis::ANALYSIS_TASK_CAP {
        return Vec::new();
    }
    crate::analyzer::skipped_reads::scan(wf, edges, topo_waves)
        .into_iter()
        .map(|(task, advice)| Hint {
            kind: KIND,
            code: None,
            task,
            advice,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    /// The NIK-156 file, verbatim — `nika check` said « run ready ✔ » with
    /// zero hints, and `nika run` died on `NIKA-VAR-001`.
    const ISSUE: &str = "\
nika: skipped-ref
permits:
  tools: [nika:jq]
tasks:
  maybe:
    when: false
    invoke:
      tool: nika:jq
      args:
        input: {}
        expression: '{url: \"https://example.com\"}'
  after_maybe:
    when: ${{ with.u != null }}
    with:
      u: ${{ tasks.maybe.output.url }}
    invoke:
      tool: nika:jq
      args:
        input: {}
        expression: '\"ran\"'
";

    /// The same file after the repair the hint teaches.
    const REPAIRED: &str = "\
nika: skipped-ref
permits:
  tools: [nika:jq]
tasks:
  maybe:
    when: false
    invoke:
      tool: nika:jq
      args:
        input: {}
        expression: '{url: \"https://example.com\"}'
  after_maybe:
    when: ${{ with.out != null }}
    with:
      out: ${{ tasks.maybe.output }}
    invoke:
      tool: nika:jq
      args:
        input: ${{ with.out.url }}
        expression: '.'
";

    fn kinds(yaml: &str) -> (crate::CheckReport, Vec<Hint>) {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
        let report = crate::check(&wf);
        let hints = report
            .hints
            .iter()
            .filter(|h| h.kind == KIND)
            .cloned()
            .collect();
        (report, hints)
    }

    #[test]
    fn the_issue_file_carries_one_skipped_read_hint_through_check() {
        let (report, hints) = kinds(ISSUE);
        assert_eq!(hints.len(), 1, "{:?}", report.hints);
        assert_eq!(hints[0].task, "after_maybe");
        assert_eq!(hints[0].code, None);
        assert!(
            hints[0].advice.contains("`tasks.maybe.output.url`")
                && hints[0].advice.contains("NIKA-VAR-001"),
            "{}",
            hints[0].advice
        );
        // Advisory: the file stays valid — the hint is what changed.
        assert!(report.is_clean(), "{:?}", report.findings);
    }

    #[test]
    fn the_repaired_file_carries_none() {
        let (report, hints) = kinds(REPAIRED);
        assert!(hints.is_empty(), "{hints:?}");
        assert!(report.is_clean(), "{:?}", report.findings);
    }

    /// A graph that did not conform makes no claim (nothing to fold), even
    /// over the read the issue file makes.
    #[test]
    fn a_graph_that_did_not_conform_makes_no_claim() {
        let broken = ISSUE.replace(
            "  after_maybe:\n",
            "  after_maybe:\n    after: { ghost: success }\n",
        );
        let (report, hints) = kinds(&broken);
        assert!(
            !report.conformance.is_empty(),
            "the fixture must not conform"
        );
        assert!(hints.is_empty(), "{hints:?}");
    }

    /// Past the analysis cap the per-producer fold does not run — and the
    /// report SAYS so through the DAG read's stated miss, never silently.
    #[test]
    fn past_the_analysis_cap_the_lane_is_a_stated_miss() {
        let mut yaml = ISSUE.to_owned();
        yaml.extend((0..crate::analysis::ANALYSIS_TASK_CAP).map(|i| {
            format!(
                "  pad{i}:\n    invoke:\n      tool: nika:jq\n      args:\n        input: {{}}\n        expression: '.'\n"
            )
        }));
        let (report, hints) = kinds(&yaml);
        assert!(hints.is_empty(), "{hints:?}");
        let miss = report
            .hints
            .iter()
            .find(|h| h.kind == "analysis")
            .expect("the cap is a stated miss");
        assert!(miss.advice.contains("(`skipped-read`)"), "{}", miss.advice);
    }

    /// `nika explain skipped-read` teaches the slot `check` prints.
    #[test]
    fn the_kind_has_a_catalog_row() {
        let help = crate::hint_help(KIND).expect("the printed slot explains");
        assert!(help.contains("when:"), "{help}");
        assert!(help.contains("!= null"), "{help}");
    }
}
