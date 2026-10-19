// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::panic)]

use super::*;
use nika_session_change::outcome::Incarnation;

struct Capture(Arc<Mutex<Vec<Value>>>);

impl Desk for Capture {
    fn read(&mut self, _: &str, _: Option<u64>, _: Option<u64>) -> Result<String, String> {
        panic!("unexpected read")
    }
    fn parse(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn report(&mut self, _: &str, _: bool) -> Result<String, String> {
        panic!("unexpected report")
    }
    fn explain(&mut self, _: &str) -> Result<String, String> {
        panic!("unexpected explain")
    }
    fn language(&mut self, _: &str, _: Option<&str>) -> Result<String, String> {
        panic!("unexpected language")
    }
    fn models(&mut self, _: &str, _: &Value) -> Result<String, String> {
        panic!("unexpected models")
    }
    fn knowledge(&mut self, _: Option<&str>, _: Option<&str>) -> Result<String, String> {
        panic!("unexpected knowledge")
    }
    fn compose(&mut self, _: &str, _: &[Value]) -> Result<String, String> {
        panic!("unexpected compose")
    }
    fn verify(
        &mut self,
        source: &str,
        (_, answers, _): (&str, &BTreeMap<String, String>, Option<&str>),
        _: (&[Resolution], &[Resolution]),
        presentation: &Value,
        _: Option<&str>,
    ) -> Result<Judged, String> {
        assert_eq!(answers, &BTreeMap::new());
        self.0.lock().unwrap().push(presentation.clone());
        let reference = format!("verification-{}", self.0.lock().unwrap().len());
        let mut out = nika_compile::initial();
        out.status = nika_compile::CompileStatus::Ready;
        out.provenance.decision = Some(json!({"semantic_verification": [{
            "candidate_sha256": nika_compile::surface::sha256(source),
            "request": "a summary of technology news in a file", "context_sha256": "c".repeat(64),
            "reference": reference, "attempted": 2, "returned": 1, "consumed": 1, "settled": true
        }]}));
        Ok(Judged::ready("verified scope".into(), source, &out))
    }
    fn judged_now(
        &mut self,
        _: &str,
        _: (&str, &BTreeMap<String, String>),
        _: (&[Resolution], &[Resolution]),
    ) -> Vec<String> {
        vec![]
    }
    fn model_facts(&mut self, _: &str) -> Option<ModelFacts> {
        None
    }
}

fn output() -> Value {
    json!({"key":"output", "question":"Where should the summary go?", "role":"output_path",
        "options":[
            {"key":"md", "label":"Markdown", "recommended":true,
             "values":[{"role":"output_path", "value":"./news/digest.md"}]},
            {"key":"html", "label":"Web page", "recommended":false,
             "values":[{"role":"output_path", "value":"./news/digest.html"}]}]})
}

fn written() -> (Toolbox, Arc<Mutex<Vec<Value>>>) {
    let mut citations = Citations::default();
    citations.record(
        1,
        Some(("u1".into(), "a summary of technology news in a file".into())),
    );
    let tools = Toolbox::new(Conversation::default(), Arc::new(Mutex::new(citations)));
    let captured = Arc::new(Mutex::new(vec![]));
    let asker = Arc::new(Incarnation);
    tools.begin(
        Box::new(Capture(captured.clone())),
        Box::new(move |context| QuestionId::new(context.to_owned(), &asker)),
    );
    let written = tools.call(ToolCall::new(
        "candidate_write",
        json!({"source":"nika: news\n# writes ./news/digest.md\n"}),
    ));
    assert!(!written.is_error, "{written:?}");
    (tools, captured)
}

#[test]
fn a_proposal_transports_all_validated_choices_without_human_answers_or_consent() {
    let (tools, captured) = written();
    let reply =
        tools.call(ToolCall::new("propose", json!({"open":[output()]})).with_meta("proposal-call"));
    assert!(!reply.is_error, "{reply:?}");
    let open = tools.with(|conversation| {
        assert_eq!(conversation.bindings().len(), 0);
        assert_eq!(conversation.answers(), BTreeMap::new());
        assert_eq!(conversation.questions().len(), 0);
        conversation.open().to_vec()
    });
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].options.len(), 2);
    assert_eq!(open[0].options[1].label, "Web page");
    assert_eq!(
        *captured.lock().unwrap(),
        vec![json!({"kind":"proposal", "pending_choices":open, "selected":null})]
    );
    assert_eq!(tools.end().acts, None);
}

#[test]
fn an_invalid_proposal_never_reaches_verification_as_presentation() {
    let (tools, captured) = written();
    let mut choice = output();
    choice["options"][1]["recommended"] = json!(true);
    let reply = tools.call(ToolCall::new("propose", json!({"open":[choice]})));
    assert!(reply.is_error);
    assert!(
        reply.text.contains("recommends exactly one option"),
        "{reply:?}"
    );
    assert_eq!(*captured.lock().unwrap(), Vec::<Value>::new());
    assert_eq!(tools.with(|conversation| conversation.open().len()), 0);
    assert_eq!(tools.end().acts, None);
}

#[test]
fn a_ready_proposal_keeps_compiler_evidence_separate_from_model_text() {
    let (tools, _) = written();
    let reply = tools.call(ToolCall::new("propose", json!({"open":[output()]})));
    assert!(!reply.is_error && reply.ends_turn, "{reply:?}");
    let source = "nika: news\n# writes ./news/digest.md\n";
    let digest = nika_compile::surface::sha256(source);
    assert_eq!(
        reply.verification,
        Some(json!({
            "schema": "nika/semantic-verification@1", "candidate_sha256": digest,
            "candidate_revision": 1, "reused": false, "records": [{
                "candidate_sha256": digest, "request": "a summary of technology news in a file",
                "context_sha256": "c".repeat(64), "reference": "verification-1", "attempted": 2, "returned": 1,
                "consumed": 1, "settled": true
            }]
        }))
    );
    let text: Value = serde_json::from_str(&reply.text).unwrap();
    assert!(text.get("verification").is_none());
    assert_eq!(text["revision"], 1);
}

#[test]
fn consent_after_another_verification_reuses_the_original_shown_records() {
    let (tools, captured) = written();
    let first = tools.call(ToolCall::new("propose", json!({})));
    assert!(!first.is_error && first.ends_turn, "{first:?}");
    let mut expected = first.verification.unwrap();
    assert_eq!(expected["records"][0]["reference"], "verification-1");
    assert_eq!(tools.end().acts, None);
    let asker = Arc::new(Incarnation);
    tools.begin(
        Box::new(Capture(captured.clone())),
        Box::new(move |context| QuestionId::new(context.to_owned(), &asker)),
    );
    let checked = tools.call(ToolCall::new("verify", json!({})));
    assert!(!checked.is_error);
    assert_eq!(
        checked.verification, None,
        "verify does not publish another receipt"
    );
    assert_eq!(captured.lock().unwrap().len(), 2);
    let citations = tools.state().citations.clone();
    citations
        .lock()
        .unwrap()
        .record(2, Some(("u2".into(), "save it".into())));
    let second = tools.call(ToolCall::new(
        "propose",
        json!({"acts": ["save"],
        "authorized_by": {"message": "u2", "excerpt": "save it"}}),
    ));
    assert!(!second.is_error && second.ends_turn, "{second:?}");
    expected["reused"] = json!(true);
    assert_eq!(second.verification, Some(expected));
    assert_eq!(
        captured.lock().unwrap().len(),
        2,
        "consent makes no new verification call"
    );
    let decided = tools.end();
    assert_eq!(decided.reused, Some(1));
    assert_eq!(
        decided.acts.map(|acts| (acts.save, acts.run)),
        Some((true, false))
    );

    // A later edit cannot borrow the shown evidence or the already-used consent.
    let asker = Arc::new(Incarnation);
    tools.begin(
        Box::new(Capture(captured.clone())),
        Box::new(move |context| QuestionId::new(context.to_owned(), &asker)),
    );
    let changed = "nika: revised\n# writes ./news/digest.md\n";
    let written = tools.call(ToolCall::new("candidate_write", json!({"source": changed})));
    assert!(!written.is_error, "{written:?}");
    let third = tools.call(ToolCall::new(
        "propose",
        json!({"acts": ["save"],
        "authorized_by": {"message": "u2", "excerpt": "save it"}}),
    ));
    assert!(!third.is_error && third.ends_turn, "{third:?}");
    let third = third.verification.unwrap();
    assert_eq!(
        third["candidate_sha256"],
        nika_compile::surface::sha256(changed)
    );
    assert_eq!(third["candidate_revision"], 2);
    assert_eq!(third["reused"], false);
    assert_eq!(third["records"][0]["reference"], "verification-3");
    assert_eq!(captured.lock().unwrap().len(), 3);
    let decided = tools.end();
    assert_eq!(decided.reused, None);
    assert_eq!(decided.acts, None);
}

/// A `candidate_write` that copies the stub Nika leaves in place of an earlier candidate's
/// document is told what it copied and what to send instead, not only that `source` is missing
/// (live gauntlet q03: 80 such calls in a row).
#[test]
fn a_copied_stub_is_named_for_what_it_is() {
    let (tools, _) = written();
    let copied = tools.call(ToolCall::new(
        "candidate_write",
        json!({"stub": "(Nika: a later revision replaced this candidate; the record keeps it whole.)"}),
    ));
    assert!(copied.is_error);
    assert!(
        copied.text.contains("`stub` is the note") && copied.text.contains("candidate_edit"),
        "{}",
        copied.text
    );
    let empty = tools.call(ToolCall::new("candidate_write", json!({})));
    assert_eq!(
        empty.text,
        "candidate_write needs the complete document in `source`"
    );
}
