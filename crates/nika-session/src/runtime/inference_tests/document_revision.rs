// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A workflow the person already has, named in a change, is revised over its complete document
//! through the Session door: public turns, real loopback inference (one revision call, then the
//! round's judge), a fixed classifier isolating the routing.
use super::*;
use crate::turn::RoutingMethod;

/// The person's own workflow, written before the session opened: a typed threshold, a filter,
/// a report, comments. No record binds it.
const SAVED: &str = r#"nika: stale-tickets
# Tickets older than the threshold are reported, oldest rules first.
const:
  records_path: ./tickets.json
  report_path: ./out/stale.json
  max_age_hours: { type: integer, value: 48 }
permits:
  fs: { read: ["./tickets.json"], write: ["./out/stale.json"] }
  tools: ["nika:read", "nika:jq", "nika:write"]
tasks:
  read_records:
    invoke: { tool: "nika:read", args: { path: "${{ const.records_path }}" } }
  parse_records:
    with: { raw: "${{ tasks.read_records.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.raw }}", expression: "fromjson" } }
  stale:
    # The threshold the person chose, in hours.
    with: { rows: "${{ tasks.parse_records.output }}", hours: "${{ const.max_age_hours }}" }
    invoke:
      tool: "nika:jq"
      args:
        input: { rows: "${{ with.rows }}", hours: "${{ with.hours }}" }
        expression: "(.hours | tonumber) as $h | [.rows[] | select(.age_hours > $h)]"
  report_text:
    with: { stale: "${{ tasks.stale.output }}" }
    invoke: { tool: "nika:jq", args: { input: { stale: "${{ with.stale }}" }, expression: "{count: (.stale | length), ids: [.stale[].id]} | tojson" } }
  write_report:
    with: { content: "${{ tasks.report_text.output }}" }
    invoke: { tool: "nika:write", args: { path: "${{ const.report_path }}", content: "${{ with.content }}" } }
outputs:
  stale: ${{ tasks.stale.output }}
"#;
const CHANGE: &str = "In stale.nika, report the tickets older than 72 hours instead of 48.";

struct Acts;
impl TurnClassifier for Acts {
    fn classify(&mut self, _: &TurnContext, line: &str) -> TurnDecision {
        let act = if line == CHANGE {
            TurnAct::Modify
        } else {
            TurnAct::NewWork
        };
        TurnDecision::new(act, RoutingMethod::Model)
    }
    fn classify_with_admission(
        &mut self,
        c: &TurnContext,
        line: &str,
        _: &nika_providers::InferenceAdmission,
    ) -> TurnDecision {
        self.classify(c, line)
    }
}

/// The revision call's answer: one literal edit over the document, in the strict text form.
fn revision() -> String {
    json!({"supersedes": [], "adds": [], "notes": "the threshold only",
        "operations": [{"op": "set", "path": "/const/max_age_hours", "value_json": "72",
            "component": "", "version": "", "bindings_json": ""}],
        "replace": ""})
    .to_string()
}

#[test]
fn a_named_workflow_is_revised_in_place_shown_and_saved_with_every_other_byte() {
    let mut replies = vec![(200, response(&revision()))];
    replies.extend((0..4).map(|_| (200, response(JUDGE_APPROVES))));
    let peer = Peer::start(replies);
    let _transport = test_transport::install(&peer.url);
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("stale.nika"), SAVED).unwrap();
    std::fs::write(root.path().join("tickets.json"), "[]").unwrap();
    let mut s = open(root.path());
    s.with_classifier(Box::new(Acts));
    s.admit_money("budget 2 USD", false, false)
        .expect("allowance");

    let out = s.turn(CHANGE);
    let TurnOutcome::Proposal { id, preview } = out else {
        panic!("a proposal over stale.nika: {out:?}")
    };
    assert!(preview.contains("stale.nika"), "{preview}");
    let first = peer.bodies()[0].to_string();
    assert!(
        first.contains("operations") && first.contains("max_age_hours"),
        "the revision call carries the operation schema and the document's nodes"
    );

    let work = s.work();
    let candidate = work.candidate.expect("the candidate");
    assert_eq!(candidate.files[0].path, PathBuf::from("stale.nika"));
    assert_eq!(
        candidate.files[0].landing,
        crate::work::Landing::Update,
        "the person's file is updated over the bytes read"
    );
    let revision = candidate.revision.expect("the document revision is shown");
    assert_eq!(revision.mode, "operations");
    // The typed constant named whole is edited at its value, the node the record names.
    assert_eq!(revision.changed, ["const.max_age_hours.value"]);
    assert!(revision.components.is_empty());

    assert!(matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)));
    let saved = std::fs::read_to_string(root.path().join("stale.nika")).unwrap();
    assert_eq!(
        saved,
        SAVED.replace(
            "max_age_hours: { type: integer, value: 48 }",
            "max_age_hours: { type: integer, value: 72 }"
        ),
        "only the threshold changed: comments, layout and every other value kept"
    );
    assert!(
        !root.path().join("out/stale.json").exists(),
        "Save is not Run"
    );
}
