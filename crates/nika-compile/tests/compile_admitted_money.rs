// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A monetary directive the caller admitted is its ceiling, never a business clause (R4 A6 ·
//! D6-S2/S3). « … write them to ./open.csv. Budget: $0. » was an unresolved clause, and
//! « …, budget=0 » a filter on a field named `budget`. A caller that admitted the directive
//! (Session's money gate) says so; the compiler reads the request with those exact spans
//! blanked, records them beside the original request's identity, and certifies no cap. Without
//! an admission (the CLI) nothing changes. A directive with no currency whose anchor names an
//! observed field reads both ways and is asked; a span that is no directive is refused.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile, intent_sha256};
use serde_json::{Value, json};

mod common;

const WORK: &str =
    "Read ./tickets.csv, keep only the rows whose status is open and write them to ./open.csv";
const PLAIN: &str = "[.records[] | select(.status == \"open\")]";

/// The world a host observes for one CSV head.
fn world(head: &str) -> Value {
    let sample = nika_compile::observation::csv(head, false);
    let mut row = json!({
        "path": "./tickets.csv", "state": "observed", "complete": false, "kind": "csv",
        "columns": sample.columns, "bytes": head.len(), "peek_sha256": format!("len-{}", head.len()),
        "delimiter": ",",
    });
    if !sample.values.is_empty() {
        row["values"] = Value::Object(sample.values.into_iter().collect());
    }
    json!({"observed": [row], "kinds": {"./tickets.csv": sample.kinds}})
}

const TICKETS: &str = "id,status,amount\n1,open,10\n2,closed,20\n3,open,30\n";

/// The request compiled with its directives admitted (all of them, as a money gate admits).
fn admitted(request: &str, head: &str) -> CompileOutcome {
    let spans = nika_compile::money::directives(request)
        .unwrap()
        .found
        .into_iter()
        .map(|d| d.span)
        .collect();
    let with = CompileRequest::create(request)
        .with_knowledge(world(head))
        .with_admitted_money(spans);
    compile(&with).unwrap()
}

fn plain(request: &str, head: &str) -> CompileOutcome {
    compile(&CompileRequest::create(request).with_knowledge(world(head))).unwrap()
}

fn says(out: &CompileOutcome, text: &str) -> bool {
    out.diagnostics.iter().any(|d| d.message.contains(text))
}

#[test]
fn an_admitted_ceiling_is_read_as_the_callers_never_as_a_clause() {
    for (suffix, text) in [
        (". Budget: $0.", "Budget: $0"),
        (", budget=0", "budget=0"),
        (" with a budget of 5 USD", "with a budget of 5 USD"),
    ] {
        let request = format!("{WORK}{suffix}");
        let out = admitted(&request, TICKETS);
        assert_eq!(out.status, CompileStatus::Ready, "{request}: {out:#?}");
        assert_eq!(
            common::compute(out.candidate.as_deref().unwrap()),
            PLAIN,
            "{request}"
        );
        let decision = out.provenance.decision.as_ref().unwrap();
        assert_eq!(
            decision["money"]["directives"][0]["text"], text,
            "{decision:#}"
        );
        assert_eq!(
            decision["intent_sha256"],
            intent_sha256(&request),
            "{decision:#}"
        );
        assert!(says(&out, "the compiler certifies no cap"), "{out:#?}");
        assert!(
            !out.candidate.as_deref().unwrap().contains(text),
            "never in the bytes"
        );
    }
    // Without an admission (the CLI), the words are read as they always were.
    let unadmitted = plain(&format!("{WORK}. Budget: $0."), TICKETS);
    assert_ne!(unadmitted.status, CompileStatus::Ready, "{unadmitted:#?}");
    assert!(
        says(&unadmitted, "Unresolved clause: Budget: $0"),
        "{unadmitted:#?}"
    );
}

#[test]
fn a_ceiling_without_a_currency_that_names_an_observed_field_is_asked() {
    let head = "id,status,amount,budget\n1,open,10,5\n2,closed,20,30\n3,open,30,0\n";
    let compact = admitted(&format!("{WORK}, budget=0"), head);
    assert_ne!(compact.status, CompileStatus::Ready, "{compact:#?}");
    assert!(compact.candidate.is_none());
    assert!(
        says(&compact, "as a rule over the observed field `budget`"),
        "{compact:#?}"
    );
    assert!(
        compact
            .questions
            .iter()
            .any(|q| q.key == "intent.clarification" && q.mandatory),
        "{compact:#?}"
    );
    // A currency marks it money: the same file, READY on the plain rule.
    let marked = admitted(&format!("{WORK}. Budget: $0."), head);
    assert_eq!(marked.status, CompileStatus::Ready, "{marked:#?}");
    assert_eq!(common::compute(marked.candidate.as_deref().unwrap()), PLAIN);
}

#[test]
fn a_span_that_is_no_directive_is_refused_and_offsets_survive_multibyte_text() {
    let request = format!("{WORK}. Budget: $0.");
    let clause = request.find("status is open").unwrap();
    let business = clause..clause + "status is open".len();
    let forged = CompileRequest::create(request.as_str())
        .with_knowledge(world(TICKETS))
        .with_admitted_money(vec![business]);
    let out = compile(&forged).unwrap();
    assert_eq!(out.status, CompileStatus::Refused, "{out:#?}");
    assert!(out.candidate.is_none());
    assert!(
        out.diagnostics.iter().any(|d| d.target == "money"),
        "{out:#?}"
    );
    // A request with multibyte words before its directive keeps exact offsets.
    let french = "Lis ./tickets.csv, garde seulement les lignes dont le status est open et écris-les dans ./open.csv. Budget : 0 €uro près ? Budget: $0.";
    let found = nika_compile::money::directives(french).unwrap();
    for directive in &found.found {
        assert!(
            french.is_char_boundary(directive.span.start)
                && french.is_char_boundary(directive.span.end)
        );
    }
    assert_eq!(
        &french[found.found.last().unwrap().span.clone()],
        "Budget: $0"
    );
}
