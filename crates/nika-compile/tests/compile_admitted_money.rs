// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A monetary directive the caller admitted is its ceiling, never a business clause (R4 A6 ·
//! D6-S2/S3). « … write them to ./open.csv. Budget: $0. » was an unresolved clause, and
//! « …, budget=0 » a filter on a field named `budget`. A caller that admitted the directive
//! (Session's money gate) says so; the compiler reads the request with those exact spans
//! blanked, records them beside the original request's identity, and certifies no cap. Without
//! an admission nothing changes. A directive with no currency whose anchor names an observed
//! field reads both ways and is asked; a span that is no directive is refused. Money-shaped words
//! are a directive or business data by their role in the request, never by a word or a field
//! (R4 B15).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile, intent_sha256};
use serde_json::{Value, json};

mod common;

const WORK: &str =
    "Read ./tickets.csv, keep only the rows whose status is open and write them to ./open.csv";
const PLAIN: &str = "[.records[] | select(.status == \"open\")]";

/// The world a host observes for one CSV head.
fn world(head: &str) -> Value {
    world_at("./tickets.csv", head)
}

/// The world a host observes for one CSV head at `path`.
fn world_at(path: &str, head: &str) -> Value {
    let sample = nika_compile::observation::csv(head, false);
    let mut row = json!({
        "path": path, "state": "observed", "complete": false, "kind": "csv",
        "columns": sample.columns, "bytes": head.len(), "peek_sha256": format!("len-{}", head.len()),
        "delimiter": ",",
    });
    if !sample.values.is_empty() {
        row["values"] = Value::Object(sample.values.into_iter().collect());
    }
    json!({"observed": [row], "kinds": {path: sample.kinds}})
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
    // Without an admission, the words are read as they always were.
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

/// The frozen B15 fixture: every field the matrix names, `budget` among them.
const INPUT: &str = "id,cost,budget,price,plafond,montant,amount_usd\na,3,1500,10,3,120,100\nb,5,1000,15,5,250,250\nc,7,2000,20,3,300,300\nd,4,1500,14,10,80,251\ne,0,999,0,0,0,0\nf,12,1200,30,3,600,600\n";

/// The request with every directive the money reader finds admitted, over the B15 fixture when
/// it is observed and over no observation at all otherwise.
fn admitted_input(request: &str, observed: bool) -> CompileOutcome {
    let spans = nika_compile::money::directives(request)
        .unwrap()
        .found
        .into_iter()
        .map(|d| d.span)
        .collect();
    let mut with = CompileRequest::create(request).with_admitted_money(spans);
    if observed {
        with = with.with_knowledge(world_at("./data/input.csv", INPUT));
    }
    compile(&with).unwrap()
}

/// R4 B15 · Q2: a phrase is money or business data by its role, never by a word or a field. The
/// source has a `budget` column, yet the operator's trailing « with a budget of 2 USD » stays the
/// caller's ceiling; « rows whose budget is 1500 USD » and « rows with a budget of 1500 USD »
/// stay business data, observed or not.
#[test]
fn a_phrase_is_money_or_data_by_its_role_never_by_a_field() {
    let directive = "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json with a budget of 2 USD";
    let out = admitted_input(directive, true);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let decision = out.provenance.decision.as_ref().unwrap();
    assert_eq!(
        decision["money"]["directives"][0]["text"], "with a budget of 2 USD",
        "{decision:#}"
    );
    let compute = common::compute(out.candidate.as_deref().unwrap());
    assert!(
        compute.contains("amount_usd") && compute.contains("250"),
        "{compute}"
    );
    let equality = "read ./data/input.csv, keep the rows whose budget is 1500 USD, write them to ./out/result.json";
    let with_a = "read ./data/input.csv, keep the rows with a budget of 1500 USD, write them to ./out/result.json";
    for business in [equality, with_a] {
        for observed in [true, false] {
            let out = admitted_input(business, observed);
            let money = out
                .provenance
                .decision
                .as_ref()
                .and_then(|d| d.get("money"));
            assert!(
                money.is_none(),
                "{business} (observed: {observed}): {money:?}"
            );
            assert!(
                !says(&out, "the monetary ceiling the caller admitted"),
                "{out:#?}"
            );
        }
    }
    let rule = admitted_input(equality, true);
    assert_eq!(rule.status, CompileStatus::Ready, "{rule:#?}");
    let compute = common::compute(rule.candidate.as_deref().unwrap());
    assert!(
        compute.contains("budget") && compute.contains("1500"),
        "{compute}"
    );
}

/// The directive spans of `text` (as a money gate admits them).
fn spans(text: &str) -> Vec<std::ops::Range<usize>> {
    nika_compile::money::directives(text)
        .unwrap()
        .found
        .into_iter()
        .map(|d| d.span)
        .collect()
}

/// The cognition door compiling `request` (the conflict path folds the replacement): no plan
/// call; a reading the reader composed is checked by the approving judge double (R1).
async fn door(request: CompileRequest) -> CompileOutcome {
    let provider = common::Provider::new(json!({}));
    let judged = common::Judged::approving(&provider);
    let request = request.with_authoring_policy(common::policy());
    let out = nika_compile_cognition::compile_with_provider(&request, &judged)
        .await
        .unwrap();
    let calls = provider.calls.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(calls, 0, "{out:#?}");
    out
}

/// What the outcome records as the caller's admitted directives, by their words.
fn admitted_words(out: &CompileOutcome) -> Vec<String> {
    out.provenance
        .decision
        .as_ref()
        .and_then(|d| d["money"]["directives"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|d| d["text"].as_str().map(str::to_owned))
        .collect()
}

/// A replacement request never inherits the spans the caller admitted on the request it replaces
/// (R4 A11, C11's adversary): spans index the bytes the caller read. « … Budget: 5 USD »
/// admitted, replaced by a request whose own « Budget: 9 USD » sits at the same bytes: the new
/// directive was recorded as the caller's admitted ceiling and blanked from the reading, and a
/// span landing on no directive refused the replacement. The replacement is now read as it
/// states itself; unchanged bytes keep their spans, and a caller re-admits on the new bytes. The
/// replacement contradicts itself, so the door folds it on its conflict path and calls no one.
#[tokio::test]
async fn a_replacement_never_inherits_the_spans_admitted_on_another_text() {
    let new = "write 'hello' to ./a.txt but do not write anything. Budget: 9 USD";
    let at = spans(new)[0].clone();
    let old = format!(
        "{:<width$}Budget: 5 USD",
        "keep the rows of ./orders.csv.",
        width = at.start
    );
    let old = old.as_str();
    assert_eq!(spans(old), std::slice::from_ref(&at));
    assert_eq!(&new[at.clone()], "Budget: 9 USD");
    let clarified = |base: &str, spans: Vec<std::ops::Range<usize>>, answer: &str| {
        CompileRequest::create(base)
            .with_admitted_money(spans)
            .answer("intent.clarification", json!(answer).to_string())
    };
    // Same offset: the new directive is no admission of the caller.
    let same = door(clarified(old, spans(old), new)).await;
    assert!(admitted_words(&same).is_empty(), "{same:#?}");
    // Another offset: no refusal on a span of another text.
    let short = "keep the rows of ./orders.csv. Budget: 5 USD";
    let moved = door(clarified(short, spans(short), new)).await;
    assert!(
        !says(&moved, "is not a monetary directive of the request"),
        "{moved:#?}"
    );
    assert!(admitted_words(&moved).is_empty(), "{moved:#?}");
    // Unchanged bytes keep what the caller admitted on them.
    let kept = door(clarified(new, spans(new), new)).await;
    assert_eq!(admitted_words(&kept), ["Budget: 9 USD"], "{kept:#?}");
    // A caller admits on the replacement's own bytes by stating them as its request.
    let readmitted = door(CompileRequest::create(new).with_admitted_money(spans(new))).await;
    assert_eq!(
        admitted_words(&readmitted),
        ["Budget: 9 USD"],
        "{readmitted:#?}"
    );
    // The same law for any caller folding a replacement.
    let folded = CompileRequest::create(old)
        .with_admitted_money(spans(old))
        .with_replaced_input(new);
    assert!(folded.money.is_empty());
    let same_bytes = CompileRequest::create(new)
        .with_admitted_money(spans(new))
        .with_replaced_input(new);
    assert_eq!(same_bytes.money, [at]);
}

/// Repeating the identical request preserves its admission and its useful deterministic result,
/// not just the money receipt. Its already blanked directive must not reenter as business work.
#[tokio::test]
async fn an_unchanged_clarification_keeps_the_admitted_work_ready() {
    let text = format!("{WORK}. Budget: 9 USD");
    let request = CompileRequest::create(text.as_str())
        .with_knowledge(world(TICKETS))
        .with_admitted_money(spans(&text));
    let direct = door(request.clone()).await;
    assert_eq!(direct.status, CompileStatus::Ready, "{direct:#?}");
    let repeated = door(request.answer("intent.clarification", json!(text).to_string())).await;
    assert_eq!(repeated.status, CompileStatus::Ready, "{repeated:#?}");
    assert_eq!(admitted_words(&repeated), ["Budget: 9 USD"]);
    assert_eq!(
        common::compute(repeated.candidate.as_deref().unwrap()),
        PLAIN
    );
    assert_eq!(repeated.candidate, direct.candidate);
    assert_eq!(
        repeated.provenance.decision.as_ref().unwrap()["intent_sha256"],
        intent_sha256(&text)
    );
}
