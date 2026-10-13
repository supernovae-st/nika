// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::panic)]

use super::*;

#[test]
fn cached_verification_belongs_to_every_offer_and_the_question_identity() {
    let root = tempfile::tempdir().unwrap();
    let source = "nika: hello\nsteps:\n  - id: hi\n    exec: printf hello\n";
    let stated = "print hello";
    let answers = BTreeMap::<String, String>::new();
    let original: Option<&str> = None;
    let presentation = json!({"kind":"proposal", "selected":null, "pending_choices":[{
        "id":"question-one", "key":"format", "question":"Choose the format", "state":"open",
        "options":[{"key":"md", "label":"Markdown", "recommended":true, "values":[]},
                   {"key":"html", "label":"Web page", "recommended":false, "values":[]}]}]});
    let at = landing(root.path(), source, None, &BTreeMap::new()).unwrap();
    let identity = json!([source, stated, [], [], answers, original, at, presentation]);
    let key = blake3::hash(identity.to_string().as_bytes())
        .to_hex()
        .to_string();
    let mut desk = SessionDesk {
        root: root.path().to_owned(),
        saved: None,
        probes: vec![],
        knowledge: None,
        verifier: Verifier {
            seat: AuthoringSeat::Deterministic { why: None },
            context: AuthoringContext::default(),
            account: None,
            kept: Arc::new(Mutex::new(Verified {
                ready: Some((key, "cached scope".into(), Some("cached doubt".into()))),
                ..Verified::default()
            })),
            trial: (
                None,
                Arc::new(|_| panic!("the cache test cannot observe a remote source")),
            ),
        },
        admitted: None,
        judged: None,
    };
    assert_eq!(
        desk.verify(
            source,
            (stated, &answers, original),
            (&[], &[]),
            &presentation
        ),
        Ok("cached scope".into())
    );
    assert_eq!(
        kept(&desk.verifier).unwrap().doubt.as_deref(),
        Some("cached doubt")
    );
    let mut other_label = presentation.clone();
    other_label["pending_choices"][0]["options"][1]["label"] = json!("HTML document");
    let mut other_value = presentation.clone();
    other_value["pending_choices"][0]["options"][1]["values"] =
        json!([{"role":"output_path", "value":"summary.html"}]);
    let mut other_question = presentation.clone();
    other_question["pending_choices"][0]["id"] = json!("question-two");
    for changed in [other_label, other_value, other_question] {
        let result = desk.verify(source, (stated, &answers, original), (&[], &[]), &changed);
        assert!(
            result.is_err(),
            "a changed presentation reused the old verdict: {result:?}"
        );
    }
}
