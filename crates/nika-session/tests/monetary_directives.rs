// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The B15 money law at the Session door (R4 · frozen EN/FR matrix): money-shaped business
//! words stay the business rule and never become the Session ceiling, an explicit directive is
//! the ceiling, a conflicting or malformed directive refuses, and a consent line changes money
//! only when the whole line is a money amendment. Counting protocol doubles: no provider, no
//! workflow execution.
#![allow(clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nika_session::money::MonetarySource;
use nika_session::turn::{RoutingMethod, TurnAct, TurnClassifier, TurnContext, TurnDecision};
use nika_session::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, SessionReasoner,
    SessionRuntime, TurnOutcome, UserIntelligencePreference,
};

/// The frozen B15 fixture: every field the matrix names, `budget` among them.
const INPUT: &str = "id,cost,budget,price,plafond,montant,amount_usd\na,3,1500,10,3,120,100\nb,5,1000,15,5,250,250\nc,7,2000,20,3,300,300\nd,4,1500,14,10,80,251\ne,0,999,0,0,0,0\nf,12,1200,30,3,600,600\n";

/// The Session default ceiling a request without a directive keeps.
const DEFAULT_USD: f64 = 0.25;

/// What the Session must hold after the request: its default ceiling, an explicit ceiling, or
/// a refusal of the stated money.
#[derive(Clone, Copy, Debug)]
enum Money {
    Default,
    Ceiling(f64),
    Refused,
}

/// The frozen matrix at the Session door: each request, the money the Session must hold, and
/// the business fields a proposal must keep when the reader already covers its operation (`&[]`
/// when the case is recorded, not gated).
const MATRIX: [(&str, &str, Money, &[&str]); 24] = [
    (
        "S01-EN",
        "read ./data/input.csv, keep the rows where cost is under 5 USD, write them to ./out/result.json",
        Money::Default,
        &["cost"],
    ),
    (
        "S01-FR",
        "lis ./data/input.csv, garde les lignes où cost est inférieur à 5 USD, écris-les dans ./out/result.json",
        Money::Default,
        &[],
    ),
    (
        "S02-EN",
        "read ./data/input.csv, keep the rows whose budget is 1500 USD, write them to ./out/result.json",
        Money::Default,
        &["budget", "1500"],
    ),
    (
        "S02-FR",
        "lis ./data/input.csv, garde les lignes dont le budget est 1500 USD, écris-les dans ./out/result.json",
        Money::Default,
        &["budget", "1500"],
    ),
    (
        "S03-EN",
        "read ./data/input.csv, keep the rows whose budget is over 1000 USD, write them to ./out/result.json",
        Money::Default,
        &["budget", "1000"],
    ),
    (
        "S03-FR",
        "lis ./data/input.csv, garde les lignes dont budget dépasse 1000 USD, écris-les dans ./out/result.json",
        Money::Default,
        &["budget", "1000"],
    ),
    (
        "S04-EN",
        "read ./data/input.csv, keep the rows whose budget is between 1000 USD and 1600 USD, write them to ./out/result.json",
        Money::Default,
        &[],
    ),
    (
        "S04-FR",
        "lis ./data/input.csv, garde les lignes dont le budget est entre 1000 USD et 1600 USD, écris-les dans ./out/result.json",
        Money::Default,
        &[],
    ),
    (
        "S05-EN",
        "read ./data/input.csv, keep the rows where cost is under $5, write them to ./out/result.json",
        Money::Default,
        &["cost"],
    ),
    (
        "S05-FR",
        "lis ./data/input.csv, garde les lignes où plafond=3USD, écris-les dans ./out/result.json",
        Money::Default,
        &[],
    ),
    (
        "S06-EN",
        "read ./data/input.csv, keep the rows where price is under 15 USD, write them to ./out/result.json",
        Money::Default,
        &["price"],
    ),
    (
        "S06-FR",
        "lis ./data/input.csv, garde les lignes dont le montant est supérieur à 100 dollars, écris-les dans ./out/result.json",
        Money::Default,
        &["montant"],
    ),
    (
        "S07-EN",
        "read ./data/input.csv, keep the rows with a budget of 1500 USD, write them to ./out/result.json",
        Money::Default,
        &[],
    ),
    (
        "S07-FR",
        "lis ./data/input.csv, garde les lignes avec un budget de 1500 USD, écris-les dans ./out/result.json",
        Money::Default,
        &[],
    ),
    (
        "S08-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json, budget 0 USD",
        Money::Ceiling(0.0),
        &["amount_usd"],
    ),
    (
        "S08-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json, plafond de 0 dollar",
        Money::Ceiling(0.0),
        &["amount_usd"],
    ),
    (
        "S09-EN",
        "read ./data/input.csv, keep the rows whose budget is 1500 USD, write them to ./out/result.json. Budget: 0 USD.",
        Money::Ceiling(0.0),
        &["budget", "1500"],
    ),
    (
        "S09-FR",
        "lis ./data/input.csv, garde les lignes dont le budget est 1500 USD, écris-les dans ./out/result.json. Budget : 0 dollar.",
        Money::Ceiling(0.0),
        &["budget", "1500"],
    ),
    (
        "S10-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json with a budget of 2 USD",
        Money::Ceiling(2.0),
        &["amount_usd"],
    ),
    (
        "S10-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json avec un plafond de 2 dollars",
        Money::Ceiling(2.0),
        &["amount_usd"],
    ),
    (
        "S11-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json. Budget: 1 USD. Cap: 2 USD.",
        Money::Refused,
        &[],
    ),
    (
        "S11-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json. Budget : 1 dollar. Plafond : 2 dollars.",
        Money::Refused,
        &[],
    ),
    (
        "S12-EN",
        "read ./data/input.csv, keep the rows where amount_usd is over 250, write them to ./out/result.json. Budget: $abc.",
        Money::Refused,
        &[],
    ),
    (
        "S12-FR",
        "lis ./data/input.csv, garde les lignes dont amount_usd dépasse 250, écris-les dans ./out/result.json avec un plafond de $NaN",
        Money::Refused,
        &[],
    ),
];

#[derive(Clone, Default)]
struct Calls {
    classifier: Arc<AtomicUsize>,
    reasoner: Arc<AtomicUsize>,
}

impl TurnClassifier for Calls {
    fn classify(&mut self, _: &TurnContext, _: &str) -> TurnDecision {
        self.classifier.fetch_add(1, Ordering::SeqCst);
        TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback)
    }
}

impl SessionReasoner for Calls {
    fn name(&self) -> String {
        "counting protocol double".to_owned()
    }

    fn reason(&mut self, prompt: &str) -> Result<nika_session::Reply, nika_session::ReasonError> {
        self.reasoner.fetch_add(1, Ordering::SeqCst);
        nika_session::ScriptedReasoner::new(vec!["controlled reply".to_owned()]).reason(prompt)
    }
}

/// A Session without intelligence over `root`, with the B15 fixture written when `observed`.
fn open(root: &Path, observed: bool) -> (SessionRuntime, Calls) {
    if observed {
        std::fs::create_dir_all(root.join("data")).expect("data");
        std::fs::write(root.join("data/input.csv"), INPUT).expect("fixture");
    }
    let selected = ResolvedSessionIntelligence::resolve(
        &UserIntelligencePreference::new(IntelligenceKind::None, None),
        &IntelligenceCensus::empty(),
    );
    let calls = Calls::default();
    let mut session = SessionRuntime::open(root, selected, Box::new(calls.clone()));
    session.with_classifier(Box::new(calls.clone()));
    (session, calls)
}

/// Every case the Session reads unlike its frozen expectation.
fn misread(root: &Path, id: &str, request: &str, money: Money, rule: &[&str]) -> Option<String> {
    let (mut session, calls) = open(root, true);
    let out = session.turn(request);
    let decision = session.monetary_decision().cloned();
    let Some(held) = decision else {
        return Some(format!("{id}: no monetary decision after {out:?}"));
    };
    let right = match money {
        Money::Default => {
            held.source == MonetarySource::SessionDefault
                && held.effective_usd.map(f64::to_bits) == Some(DEFAULT_USD.to_bits())
        }
        Money::Ceiling(usd) => {
            held.source == MonetarySource::Explicit
                && held.effective_usd.map(f64::to_bits) == Some(usd.to_bits())
        }
        Money::Refused => {
            held.source == MonetarySource::Rejected && matches!(out, TurnOutcome::Refusal(_))
        }
    };
    let kept = rule.is_empty()
        || matches!(&out, TurnOutcome::Proposal { preview, .. }
            if rule.iter().all(|word| preview.contains(word)));
    let cognition = calls.reasoner.load(Ordering::SeqCst);
    (!right || !kept || cognition != 0).then(|| {
        format!(
            "{id}: money {:?} {:?} (expected {money:?}), rule {rule:?} kept: {kept}, cognition calls {cognition}: {out:?}",
            held.source, held.effective_usd
        )
    })
}

#[test]
fn the_frozen_matrix_holds_the_money_each_request_states() {
    let wrong: Vec<String> = MATRIX
        .iter()
        .filter_map(|&(id, request, money, rule)| {
            let dir = tempfile::tempdir().expect("fixture");
            misread(dir.path(), id, request, money, rule)
        })
        .collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// No field observation: the law reads the words, never the world. The source is absent, yet
/// the budget clause stays business data and no ceiling is taken from it.
#[test]
fn an_unobserved_source_takes_no_ceiling_from_its_business_words() {
    for request in [
        "read ./data/input.csv, keep the rows whose budget is 1500 USD, write them to ./out/result.json",
        "read ./data/input.csv, keep the rows with a budget of 1500 USD, write them to ./out/result.json",
    ] {
        let dir = tempfile::tempdir().expect("fixture");
        let (mut session, _) = open(dir.path(), false);
        let out = session.turn(request);
        let held = session.monetary_decision().expect("money");
        assert_eq!(
            held.source,
            MonetarySource::SessionDefault,
            "{request}: {out:?}"
        );
        assert_eq!(held.effective_usd, Some(DEFAULT_USD), "{request}: {out:?}");
    }
}

/// A sentence of its own is an explicit directive in either language (the French copula alone
/// in its sentence); `cost` in a sentence of its own is business prose, never a ceiling.
#[test]
fn an_own_sentence_directive_holds_in_french_and_cost_prose_never_does() {
    let dir = tempfile::tempdir().expect("fixture");
    let (mut session, _) = open(dir.path(), true);
    let out =
        session.turn("Copie ./data/input.csv vers ./out/copy.csv. Le budget est de 2 dollars.");
    let held = session.monetary_decision().expect("money");
    assert_eq!(held.source, MonetarySource::Explicit, "{out:?}");
    assert_eq!(held.effective_usd, Some(2.0), "{out:?}");
    let dir = tempfile::tempdir().expect("fixture");
    let (mut session, _) = open(dir.path(), true);
    let out = session.turn("Copy ./data/input.csv to ./out/copy.csv. The cost is 5 USD.");
    let held = session.monetary_decision().expect("money");
    assert_eq!(held.source, MonetarySource::SessionDefault, "{out:?}");
    assert_eq!(held.effective_usd, Some(DEFAULT_USD), "{out:?}");
}

/// A consent line changes money only when the whole line is a money amendment, which gets a
/// fresh proposal identity; a business revision never changes money by a numeric coincidence.
#[test]
fn a_consent_line_changes_money_only_as_a_whole_money_amendment() {
    const WORK: &str = "read ./data/input.csv, keep the rows where price is under 15 USD, write them to ./out/result.json";
    for business in [
        "only keep the rows whose budget is 1500 USD",
        "garde seulement les lignes dont le budget est 1500 USD",
    ] {
        let dir = tempfile::tempdir().expect("fixture");
        let (mut session, _) = open(dir.path(), true);
        assert!(
            matches!(session.turn(WORK), TurnOutcome::Proposal { .. }),
            "{WORK}"
        );
        let out = session.consent(business);
        let held = session.monetary_decision().expect("money");
        assert_eq!(
            held.source,
            MonetarySource::SessionDefault,
            "{business}: {out:?}"
        );
        assert_eq!(held.effective_usd, Some(DEFAULT_USD), "{business}: {out:?}");
        assert_eq!(held.refusal, None, "{business}: {out:?}");
    }
    for amendment in ["Budget: 0 USD.", "Le budget est de 0 dollar."] {
        let dir = tempfile::tempdir().expect("fixture");
        let (mut session, _) = open(dir.path(), true);
        let TurnOutcome::Proposal { id: first, .. } = session.turn(WORK) else {
            panic!("{WORK}: a proposal");
        };
        let out = session.consent(amendment);
        let TurnOutcome::Proposal { id, .. } = out else {
            panic!("{amendment}: a revised proposal, got {out:?}");
        };
        assert_ne!(id, first, "{amendment}");
        let held = session.monetary_decision().expect("money");
        assert_eq!(held.source, MonetarySource::Explicit, "{amendment}");
        assert_eq!(held.effective_usd, Some(0.0), "{amendment}");
    }
}
