// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The picker's home screen over the actual Session runtime: the recommendation selected so
//! one `Enter` applies it, the current choice kept, every other choice, the three roles with
//! their values, and the default. The decision seat and the default go through their Session
//! doors; the Run model is said to the author as the person's own line. Hermetic: the stub
//! route never answers, the decision seat is pinned in each test, and no provider is called.

use super::*;
use crate::composer::{Composer, answer::Offered};
use crate::model::{Offer, Shape};
use crate::session::intelligence::{Shown, View};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use nika_session::authoring::{AuthoringContext, DecisionSetup};
use nika_session::work::{self, ModelFacts, OfferValue, ValueRole};

fn rows(waiting: &Waiting) -> Vec<Offer> {
    let Waiting::Selection { asked } = waiting else {
        panic!("a settings list: {waiting:?}")
    };
    let Shape::Choice(offers) = &asked.shape else {
        panic!("one choice")
    };
    offers.clone()
}

fn row<'a>(rows: &'a [Offer], key: &str) -> &'a Offer {
    rows.iter()
        .find(|offer| offer.key == key)
        .unwrap_or_else(|| panic!("{key}: {rows:#?}"))
}

/// A host whose decision seat is pinned, whatever this machine's environment names.
fn pinned(live: &mut Live, seat: Option<DecisionSetup>) {
    let runtime = live.runtime.as_mut().expect("runtime");
    let alone = seat.is_none();
    runtime.set_authoring_context(AuthoringContext::default().with_decision(seat));
    if alone {
        let _ = runtime.choose_decision(nika_session::runtime::DecisionChoice::AuthorAlone);
    }
}

fn jev(key: Option<&str>) -> DecisionSetup {
    DecisionSetup::with_key(
        "typesafe/jev-1.13.0",
        key.map(str::to_owned),
        Some("http://127.0.0.1:1"),
    )
}

/// One model option, its facts typed as the Session states them.
fn model(key: &str, model: &str, (via, class): (&str, &str), current: bool) -> work::Offer {
    let facts = ModelFacts::new(
        "author",
        model,
        (via.to_owned(), class.to_owned(), "api_metered".to_owned()),
        true,
        Some(10.0),
    )
    .with_catalogue(Some(2.0), Some(1_000_000), false);
    let value = OfferValue::new(ValueRole::Value, model, None).with_choice(Some(facts));
    work::Offer::new(key, format!("{model} · raw"), current).with_values(vec![value])
}

#[test]
fn home_offers_the_current_choice_every_other_choice_and_the_three_roles() {
    let room = Room::new("home-rows");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let home = waits(&live.submit("/intelligence").beats).expect("home");
    let rows = rows(&home);
    let keys: Vec<&str> = rows.iter().map(|offer| offer.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "cancel",
            "/intelligence connections",
            "/intelligence author",
            "/intelligence decision",
            "/intelligence run",
            "/intelligence default",
        ],
        "no recommendation without a model's facts"
    );
    assert_eq!(
        row(&rows, "cancel").label,
        "Keep current · s90-unpriced-fixture"
    );
    assert_eq!(
        row(&rows, "/intelligence connections").label,
        "Other choice…"
    );
    assert_eq!(
        row(&rows, "/intelligence author").label,
        "Author: s90-unpriced-fixture"
    );
    assert_eq!(
        row(&rows, "/intelligence decision").label,
        "Decision: the author decides alone"
    );
    assert_eq!(
        row(&rows, "/intelligence run").label,
        "Run: set by each workflow"
    );
    assert_eq!(
        row(&rows, "/intelligence default").note,
        "new conversations start with s90-unpriced-fixture"
    );
    let Waiting::Selection { asked } = &home else {
        panic!("home")
    };
    assert_eq!(asked.label, "Intelligence for this conversation");
    assert_eq!(asked.back, None, "Esc at home keeps the current choice");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// The recommendation leads home and reads its display name and facts. Reopened, home opens
/// on « Keep current », so one `Enter` keeps the choice in use; where a first choice is
/// required, the recommendation is selected, so one `Enter` applies its exact key. A decision
/// never opens on a selection.
#[test]
fn the_recommendation_is_selected_so_one_enter_applies_it() {
    let room = Room::new("home-recommended");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![
        model(
            "2 deepseek/deepseek-flash",
            "deepseek/deepseek-flash",
            ("deepseek", "api"),
            true,
        ),
        model(
            "2 anthropic/claude-sonnet-5-5",
            "anthropic/claude-sonnet-5-5",
            ("anthropic", "api"),
            false,
        ),
    ];
    intelligence.recommended = Some("2 anthropic/claude-sonnet-5-5".to_owned());
    let mut waiting = crate::session::asked::capture(&snapshot, 21);
    let shown = Shown {
        view: &View::Home,
        run: None,
        saved: None,
        jev: None,
        keys: &[],
        testable: false,
        checked: None,
        voice: crate::lang::Voice::default(),
    };
    crate::session::intelligence::project(&mut waiting, &snapshot, &shown);
    let rows = rows(&waiting);
    assert_eq!(rows[0].key, "2 anthropic/claude-sonnet-5-5");
    assert_eq!(rows[0].label, "Recommended for you · Claude Sonnet 5.5");
    assert_eq!(
        rows[0].note,
        "Anthropic API · $2 in / $10 out per MTok · 1M context"
    );
    assert_eq!(rows[1].label, "Keep current · DeepSeek Flash");
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(&waiting);
    assert_eq!(
        composer.offer_selected(&waiting),
        Some(1),
        "reopened: the choice in use"
    );
    assert_eq!(
        composer.offer_key(&waiting, enter),
        Offered::Answer("cancel".into()),
        "one Enter keeps it"
    );
    let mut first = crate::session::asked::capture(&snapshot, 21);
    if let Waiting::Selection { asked } = &mut first {
        asked.mandatory = true;
    }
    crate::session::intelligence::project(&mut first, &snapshot, &shown);
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(&first);
    assert_eq!(
        composer.offer_selected(&first),
        Some(0),
        "a first choice: the recommendation"
    );
    assert_eq!(
        composer.offer_key(&first, enter),
        Offered::Answer("2 anthropic/claude-sonnet-5-5".into()),
        "one Enter applies it"
    );
    let Waiting::Selection { asked } = waiting else {
        panic!("home")
    };
    let mut decision = asked.clone();
    decision.mandatory = true;
    let decision = Waiting::Action {
        key: "save".into(),
        asked: decision,
    };
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(&decision);
    assert_eq!(composer.offer_selected(&decision), None, "never a decision");
}

/// The decision row switches the seat through the Session's door, back home on success; an
/// unavailable Jev says why, keeps the current choice and stays on its list.
#[test]
fn the_decision_row_switches_the_seat_and_an_unavailable_jev_says_why() {
    let room = Room::new("home-decision");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, Some(jev(Some("k"))));
    let home = waits(&live.submit("/intelligence").beats).expect("home");
    assert_eq!(
        row(&rows(&home), "/intelligence decision").label,
        "Decision: Jev 1.13.0"
    );
    let list = waits(&live.submit("/intelligence decision").beats).expect("seats");
    let seats = rows(&list);
    assert_eq!(
        row(&seats, "/intelligence decision jev").note,
        "current · Jev 1.13.0"
    );
    assert_eq!(
        row(&seats, "/intelligence decision author").note,
        "no decision seat is consulted"
    );
    let alone = live.submit("/intelligence decision author");
    assert!(joined(&alone.beats).contains("decides alone"), "{alone:?}");
    let home = waits(&alone.beats).expect("home again");
    assert_eq!(
        row(&rows(&home), "/intelligence decision").label,
        "Decision: the author decides alone"
    );
    let back = live.submit("/intelligence decision jev");
    let home = waits(&back.beats).expect("home again");
    assert_eq!(
        row(&rows(&home), "/intelligence decision").label,
        "Decision: Jev 1.13.0"
    );
    pinned(&mut live, Some(jev(None)));
    let list = waits(&live.submit("/intelligence decision").beats).expect("seats");
    let note = row(&rows(&list), "/intelligence decision jev").note.clone();
    assert!(note.contains("unavailable: TYPESAFE_API_KEY"), "{note}");
    let refused = live.submit("/intelligence decision jev");
    assert!(
        joined(&refused.beats).contains("Jev is unavailable"),
        "{refused:?}"
    );
    let still = waits(&refused.beats).expect("the seats stay");
    assert!(
        rows(&still)
            .iter()
            .any(|offer| offer.key == "/intelligence decision author")
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "no decision is a model call"
    );
}

/// The Run model is said to the author as the person's own line once the picker closes with
/// the author kept, and home shows it as requested; without an author nothing is sent.
#[test]
fn the_run_row_tells_the_author_and_shows_the_model_requested() {
    let room = Room::new("home-run");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let list = waits(&live.submit("/intelligence run").beats).expect("models");
    let Waiting::Selection { asked } = &list else {
        panic!("run models")
    };
    assert_eq!(asked.back.as_deref(), Some("/intelligence"));
    let told = live.submit("/intelligence run deepseek/deepseek-flash");
    assert!(
        calls.load(Ordering::SeqCst) >= 1,
        "said to the author: {told:?}"
    );
    assert_eq!(
        live.run_requested.as_deref(),
        Some("deepseek/deepseek-flash")
    );
    assert!(
        !live.runtime.as_ref().expect("runtime").pending_choice(),
        "the picker closed"
    );
    let home = waits(&live.submit("/intelligence").beats).expect("home");
    let run = row(&rows(&home), "/intelligence run").clone();
    assert_eq!(run.label, "Run: DeepSeek Flash");
    assert_eq!(run.note, "change · requested for the next workflows");
    let unchosen_room = Room::new("home-run-unchosen");
    let unchosen_calls = Arc::new(AtomicUsize::new(0));
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("deepseek".into());
    let counted = Arc::clone(&unchosen_calls);
    let mut unchosen = Live::new(
        unchosen_room.0.clone(),
        census,
        None,
        None,
        Box::new(move |_| Box::new(Route(Arc::clone(&counted)))),
        runners(),
    );
    let _ = unchosen.open();
    let refused = unchosen.submit("/intelligence run deepseek/deepseek-flash");
    assert!(
        joined(&refused.beats).contains("Choose an author first"),
        "{refused:?}"
    );
    assert_eq!(unchosen.run_requested, None);
    assert_eq!(unchosen_calls.load(Ordering::SeqCst), 0);
}

/// The Run list offers each ready model once whatever its route, by its exact id.
#[test]
fn the_run_list_offers_each_ready_model_once() {
    let room = Room::new("home-run-list");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![
        model(
            "2 anthropic/claude-sonnet-5-5",
            "anthropic/claude-sonnet-5-5",
            ("anthropic", "api"),
            false,
        ),
        model(
            "1 claude-code/claude-sonnet-5-5",
            "anthropic/claude-sonnet-5-5",
            ("claude-code", "harness"),
            false,
        ),
        model(
            "2 deepseek/deepseek-flash",
            "deepseek/deepseek-flash",
            ("deepseek", "api"),
            true,
        ),
    ];
    let mut waiting = crate::session::asked::capture(&snapshot, 22);
    let shown = Shown {
        view: &View::Run,
        run: Some("deepseek/deepseek-flash"),
        saved: None,
        jev: None,
        keys: &[],
        testable: false,
        checked: None,
        voice: crate::lang::Voice::default(),
    };
    crate::session::intelligence::project(&mut waiting, &snapshot, &shown);
    let rows = rows(&waiting);
    let listed: Vec<(&str, &str)> = rows
        .iter()
        .map(|offer| (offer.key.as_str(), offer.label.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            (
                "/intelligence run anthropic/claude-sonnet-5-5",
                "Claude Sonnet 5.5"
            ),
            (
                "/intelligence run deepseek/deepseek-flash",
                "DeepSeek Flash"
            ),
        ]
    );
    assert!(rows[1].note.starts_with("requested · "), "{}", rows[1].note);
}

/// "Set as default" writes the choice in use with its decision seat through the Session's
/// door; home then says it is already the default, and a model row naming it is marked.
#[test]
fn set_as_default_saves_the_choice_and_home_marks_it() {
    let room = Room::new("home-default");
    let home_dir = Room::new("home-default-home");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("deepseek".to_owned());
    let pref = UserIntelligencePreference::new(
        IntelligenceKind::Api {
            provider: "deepseek".to_owned(),
        },
        Some(MODEL.to_owned()),
    );
    let counted = Arc::clone(&calls);
    let mut live = Live::new(
        room.0.clone(),
        census,
        Some(pref),
        Some(home_dir.0.clone()),
        Box::new(move |_| Box::new(Route(Arc::clone(&counted)))),
        runners(),
    );
    let _ = live.open();
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let saved = live.submit("/intelligence default");
    assert!(
        joined(&saved.beats).contains("Saved as the default"),
        "{saved:?}"
    );
    let back = UserIntelligencePreference::load(&home_dir.0).expect("written");
    assert_eq!(back.model.as_deref(), Some(MODEL));
    assert_eq!(back.decision.as_deref(), Some("author"));
    let home = waits(&saved.beats).expect("home");
    assert_eq!(
        row(&rows(&home), "/intelligence default").note,
        "already the default"
    );
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![model(
        &format!("2 {MODEL}"),
        MODEL,
        ("deepseek", "api"),
        true,
    )];
    intelligence.recommended = Some(format!("2 {MODEL}"));
    let mut waiting = crate::session::asked::capture(&snapshot, 23);
    let runtime = live.runtime.as_ref().expect("runtime");
    let shown = Shown {
        view: &View::Home,
        run: None,
        saved: runtime.operator_default(),
        jev: None,
        keys: &[],
        testable: false,
        checked: None,
        voice: crate::lang::Voice::default(),
    };
    crate::session::intelligence::project(&mut waiting, &snapshot, &shown);
    let first = &rows(&waiting)[0];
    assert!(
        first.note.starts_with("current · default · "),
        "{}",
        first.note
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

fn both_anthropic_keys() -> crate::session::intelligence::keys::KeyNames {
    crate::session::intelligence::keys::KeyNames {
        provider: "anthropic".to_owned(),
        vars: vec![
            "NIKA_ANTHROPIC_API_KEY".to_owned(),
            "ANTHROPIC_API_KEY".to_owned(),
        ],
        conflict: true,
    }
}

/// API rows name the variable that supplies their key and warn when two set variables
/// differ: the recommendation's detail, the provider row among every connection, and a
/// "Test the key" row on an opened API connection, by name only.
#[test]
fn api_rows_name_the_key_variable_and_warn_when_two_differ() {
    let room = Room::new("home-keys");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![model(
        "2 anthropic/claude-sonnet-5-5",
        "anthropic/claude-sonnet-5-5",
        ("anthropic", "api"),
        false,
    )];
    intelligence.recommended = Some("2 anthropic/claude-sonnet-5-5".to_owned());
    let keys = [both_anthropic_keys()];
    let warning =
        "⚠ NIKA_ANTHROPIC_API_KEY and ANTHROPIC_API_KEY differ · Nika uses NIKA_ANTHROPIC_API_KEY";
    let paint = |view: &View| {
        let mut waiting = crate::session::asked::capture(&snapshot, 24);
        let shown = Shown {
            view,
            run: None,
            saved: None,
            jev: None,
            keys: &keys,
            testable: true,
            checked: Some(("anthropic", "valid")),
            voice: crate::lang::Voice::default(),
        };
        crate::session::intelligence::project(&mut waiting, &snapshot, &shown);
        rows(&waiting)
    };
    let home = paint(&View::Home);
    assert!(home[0].note.ends_with(warning), "{}", home[0].note);
    let connections = paint(&View::Connections);
    let anthropic = row(&connections, "/intelligence models 2 anthropic");
    assert_eq!(anthropic.note, warning);
    let models = paint(&View::Models("2 anthropic".to_owned()));
    let test = row(&models, "/intelligence test anthropic");
    assert_eq!(test.label, "Test the key");
    assert_eq!(test.note, format!("{warning} · valid"));
    let last = models.last().expect("the row back");
    assert_eq!(
        last.key, "/intelligence connections",
        "the test row comes first"
    );
    for listed in [&home, &connections, &models] {
        assert!(
            listed.iter().all(|offer| !offer.note.contains("sk-")),
            "names only"
        );
    }
}

/// "Test the key" sends the host's one check only when picked, never at open or while
/// browsing; its verdict is said and shown on the row, and without a checker nothing is sent.
#[test]
fn the_key_check_runs_only_when_picked_and_its_verdict_shows_inline() {
    let room = Room::new("home-key-check");
    let calls = Arc::new(AtomicUsize::new(0));
    let checks = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&checks);
    let mut live = session_live(&room, &calls).with_key_check(Box::new(move |provider| {
        counted.fetch_add(1, Ordering::SeqCst);
        assert_eq!(provider, "deepseek");
        Ok(nika_session::KeyCheck::Refused {
            status: 401,
            said: "the provider refused the key".to_owned(),
        })
    }));
    live.key_names = vec![crate::session::intelligence::keys::KeyNames {
        provider: "deepseek".to_owned(),
        vars: vec!["DEEPSEEK_API_KEY".to_owned()],
        conflict: false,
    }];
    pinned(&mut live, None);
    let home = waits(&live.submit("/intelligence").beats).expect("home");
    let test = row(&rows(&home), "/intelligence test deepseek").clone();
    assert_eq!(test.label, "Test the key · DeepSeek");
    assert_eq!(
        test.note,
        "key: DEEPSEEK_API_KEY · one free request, no model runs"
    );
    let _ = live.submit("/intelligence connections");
    assert_eq!(
        checks.load(Ordering::SeqCst),
        0,
        "never at open or while browsing"
    );
    let _ = live.submit("/intelligence");
    let checked = live.submit("/intelligence test deepseek");
    assert_eq!(checks.load(Ordering::SeqCst), 1, "once, when picked");
    let said = joined(&checked.beats);
    assert!(
        said.contains("key refused by DeepSeek (401) · check the key or choose another connection"),
        "{said}"
    );
    assert!(
        !said.contains("the provider refused the key"),
        "one plain line, never the provider's words: {said}"
    );
    let home = waits(&checked.beats).expect("home stays");
    assert_eq!(
        row(&rows(&home), "/intelligence test deepseek").note,
        "key: DEEPSEEK_API_KEY · refused (401)"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no model ran");
    let bare_room = Room::new("home-key-check-bare");
    let bare_calls = Arc::new(AtomicUsize::new(0));
    let mut bare = session_live(&bare_room, &bare_calls);
    bare.key_names = live.key_names.clone();
    let home = waits(&bare.submit("/intelligence").beats).expect("home");
    assert!(
        rows(&home)
            .iter()
            .all(|offer| !offer.key.starts_with("/intelligence test ")),
        "no checker, no row"
    );
    let refused = bare.submit("/intelligence test deepseek");
    assert!(
        joined(&refused.beats).contains("no key check"),
        "{refused:?}"
    );
}

/// A connection whose key the person's check found refused is not put in use (live gauntlet 6,
/// r02 B1: `OpenAI` answered 401 on every request after « Test the key » said so): keeping it,
/// choosing one of its models or typing words over the list says in one plain line that the
/// key was refused and nothing reaches the Session; the list stays open for another choice,
/// typed words come back to the box, and a later check that passes lets the choice through.
#[test]
fn a_connection_whose_key_was_refused_is_not_put_in_use() {
    let room = Room::new("home-key-refused");
    let calls = Arc::new(AtomicUsize::new(0));
    let checks = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&checks);
    let mut live = session_live(&room, &calls).with_key_check(Box::new(move |_| {
        // Refused at first; accepted once the person fixed the key.
        Ok(if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            nika_session::KeyCheck::Refused {
                status: 401,
                said: "invalid key".to_owned(),
            }
        } else {
            nika_session::KeyCheck::Valid
        })
    }));
    live.key_names = vec![crate::session::intelligence::keys::KeyNames {
        provider: "deepseek".to_owned(),
        vars: vec!["DEEPSEEK_API_KEY".to_owned()],
        conflict: false,
    }];
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let _ = live.submit("/intelligence test deepseek");
    let refused = "key refused by DeepSeek (401) · this choice is not applied · check the key or \
                   choose another connection";
    let choosing = |live: &Live| {
        live.runtime
            .as_ref()
            .is_some_and(SessionRuntime::pending_choice)
    };
    let back = |turn: &Turn| (turn.beats.iter()).any(|beat| matches!(beat, Beat::NotTaken(_)));
    // Keeping the choice in use, the connection's own row, and one of its models by name.
    for picked in [
        "cancel",
        "2 deepseek",
        "/intelligence 2 deepseek/deepseek-flash",
    ] {
        let turn = live.submit(picked);
        assert!(
            joined(&turn.beats).contains(refused),
            "{picked}: {:?}",
            turn.beats
        );
        assert!(
            matches!(waits(&turn.beats), Some(Waiting::Selection { .. })),
            "{picked}: the list stays open"
        );
        assert!(
            !back(&turn),
            "{picked}: a row picked puts nothing in the box"
        );
        assert!(choosing(&live), "{picked}: nothing reached the Session");
    }
    let typed = live.submit("bonjour, on reprend");
    assert!(joined(&typed.beats).contains(refused), "{:?}", typed.beats);
    assert!(back(&typed), "typed words come back to the box");
    assert!(choosing(&live));
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no model ran");
    let _ = live.submit("/intelligence test deepseek");
    let kept = live.submit("cancel");
    assert!(
        !joined(&kept.beats).contains("key refused"),
        "{:?}",
        kept.beats
    );
    assert!(!choosing(&live), "the choice is kept once the key passes");
}

/// A connection's list shows the current generation and folds earlier generations, dated
/// snapshots and legacy models behind one row; the model in use never folds, and the older
/// list goes back to the current one.
#[test]
fn older_generations_and_snapshots_fold_behind_one_row() {
    let room = Room::new("home-older");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    let api = ("anthropic", "api");
    let option = |id: &str, current: bool| {
        model(
            &format!("2 anthropic/{id}"),
            &format!("anthropic/{id}"),
            api,
            current,
        )
    };
    intelligence.options = vec![
        option("claude-sonnet-5-5", false),
        option("claude-haiku-4-5-20251001", false),
        option("claude-opus-5-5", false),
        option("claude-3-7-sonnet", false),
        option("claude-sonnet-4-5", true),
        option("claude-fable-5-1", false),
    ];
    intelligence.recommended = Some("2 anthropic/claude-sonnet-5-5".to_owned());
    let paint = |view: &View| {
        let mut waiting = crate::session::asked::capture(&snapshot, 25);
        let shown = Shown {
            view,
            run: None,
            saved: None,
            jev: None,
            keys: &[],
            testable: false,
            checked: None,
            voice: crate::lang::Voice::default(),
        };
        crate::session::intelligence::project(&mut waiting, &snapshot, &shown);
        waiting
    };
    let current = paint(&View::Models("2 anthropic".to_owned()));
    let labels: Vec<String> = rows(&current)
        .into_iter()
        .map(|offer| offer.label)
        .collect();
    assert_eq!(
        labels,
        [
            "Claude Sonnet 5.5",
            "Claude Opus 5.5",
            "Claude Sonnet 4.5",
            "Claude Fable 5.1",
            "Older models (2)…",
            "All connections",
        ],
        "the model in use stays; two fold"
    );
    assert_eq!(
        row(&rows(&current), "/intelligence older 2 anthropic").note,
        "earlier generations, dated snapshots and legacy models"
    );
    let older = paint(&View::Older("2 anthropic".to_owned()));
    let Waiting::Selection { asked } = &older else {
        panic!("older models")
    };
    assert_eq!(
        asked.back.as_deref(),
        Some("/intelligence models 2 anthropic")
    );
    let listed: Vec<(String, String)> = rows(&older)
        .into_iter()
        .map(|offer| (offer.key, offer.label))
        .collect();
    assert_eq!(
        listed,
        [
            (
                "2 anthropic/claude-haiku-4-5-20251001".to_owned(),
                "Claude Haiku 4.5 (20251001)".to_owned()
            ),
            (
                "2 anthropic/claude-3-7-sonnet".to_owned(),
                "Claude 3.7 Sonnet".to_owned()
            ),
            (
                "/intelligence models 2 anthropic".to_owned(),
                "Current models".to_owned()
            ),
        ]
    );
    let opened = live.submit("/intelligence older 2 deepseek");
    assert!(
        guide(&opened.beats).contains("Older models"),
        "the line opens the older list: {opened:?}"
    );
    assert!(
        !joined(&opened.beats).contains("Older models"),
        "its guide is the list's, not the conversation's: {opened:?}"
    );
}

/// From every connection (and from home), typed words find a model by its display name under
/// any connection, and `Enter` applies that model's own key.
#[test]
fn typed_words_find_a_model_by_name_from_every_connection() {
    let room = Room::new("home-find-model");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![
        model(
            "2 deepseek/deepseek-flash",
            "deepseek/deepseek-flash",
            ("deepseek", "api"),
            true,
        ),
        model(
            "2 anthropic/claude-sonnet-5-5",
            "anthropic/claude-sonnet-5-5",
            ("anthropic", "api"),
            false,
        ),
    ];
    for view in [View::Connections, View::Home] {
        let mut waiting = crate::session::asked::capture(&snapshot, 26);
        let shown = Shown {
            view: &view,
            run: None,
            saved: None,
            jev: None,
            keys: &[],
            testable: false,
            checked: None,
            voice: crate::lang::Voice::default(),
        };
        crate::session::intelligence::project(&mut waiting, &snapshot, &shown);
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.follow(&waiting);
        let listed = composer.offer_rows(&waiting);
        assert!(
            (listed.iter()).all(|at| rows(&waiting)[*at].key != "2 anthropic/claude-sonnet-5-5"),
            "{view:?}: not listed before typing"
        );
        composer.paste_own("son");
        let found = composer.offer_rows(&waiting);
        let names: Vec<String> = found
            .iter()
            .map(|at| rows(&waiting)[*at].label.clone())
            .collect();
        assert_eq!(names, ["Claude Sonnet 5.5 · Anthropic API"], "{view:?}");
        assert_eq!(
            composer.offer_key(&waiting, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Offered::Answer("2 anthropic/claude-sonnet-5-5".into()),
            "{view:?}"
        );
    }
}

/// Words typed where models are reached by name find models of every connection, each naming
/// how it is reached on its row, never only in its quiet facts: `codex` typed home finds the
/// Codex agent account's model and the API models whose names hold « Codex », none of them
/// taken for the other. Over every connection, the same word names the Codex agent account and
/// narrows the list to it (live gauntlet 6, r02 and r11).
#[test]
fn a_found_model_names_how_it_is_reached() {
    let room = Room::new("found-access");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![
        model(
            "1 codex/gpt-5.3-codex",
            "openai/gpt-5.3-codex",
            ("codex", "harness"),
            false,
        ),
        model(
            "2 openai/gpt-5.3-codex-spark",
            "openai/gpt-5.3-codex-spark",
            ("openai", "api"),
            false,
        ),
        model(
            "2 openrouter/openai/gpt-5.1-codex-max",
            "openai/gpt-5.1-codex-max",
            ("openrouter", "api"),
            false,
        ),
        model(
            "3 ollama/qwen3-coder",
            "qwen3-coder",
            ("ollama", "local"),
            false,
        ),
    ];
    let labels = |view: &View| -> Vec<String> {
        (found(&snapshot, view, "codex").into_iter())
            .map(|(label, _)| label)
            .collect()
    };
    assert_eq!(
        labels(&View::Home),
        [
            "GPT-5.3 Codex · Codex agent account",
            "GPT-5.3 Codex Spark · OpenAI API",
            "GPT-5.1 Codex Max · OpenRouter API",
        ]
    );
    assert_eq!(
        labels(&View::Connections),
        ["codex · Agent account · 1 model"]
    );
    let offers = listed(&snapshot, &View::Connections);
    let local = row(&offers, "3 ollama/qwen3-coder");
    assert_eq!(local.label, "qwen3-coder · Ollama on this machine");
    assert!(!local.note.contains("Ollama"), "said once: {}", local.note);
}

/// A model option whose catalogue prices state `input` and `output` per million tokens.
fn priced(key: &str, model: &str, via: &str, (input, output): (f64, f64)) -> work::Offer {
    let facts = ModelFacts::new(
        "author",
        model,
        (via.to_owned(), "api".to_owned(), "api_metered".to_owned()),
        true,
        Some(output),
    )
    .with_catalogue(Some(input), Some(128_000), false);
    let value = OfferValue::new(ValueRole::Value, model, None).with_choice(Some(facts));
    work::Offer::new(key, format!("{model} · raw"), false).with_values(vec![value])
}

/// The rows a filter finds over `view` when `typed` is typed: their labels, in order.
fn found(snapshot: &work::Work, view: &View, typed: &str) -> Vec<(String, String)> {
    let mut waiting = crate::session::asked::capture(snapshot, 27);
    let shown = Shown {
        view,
        run: None,
        saved: None,
        jev: None,
        keys: &[],
        testable: false,
        checked: None,
        voice: crate::lang::Voice::default(),
    };
    crate::session::intelligence::project(&mut waiting, snapshot, &shown);
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.follow(&waiting);
    composer.paste_own(typed);
    let offers = rows(&waiting);
    (composer.offer_rows(&waiting).into_iter())
        .filter_map(|at| offers.get(at))
        .map(|offer| (offer.label.clone(), offer.note.clone()))
        .collect()
}

/// What `view` shows over `snapshot`: its rows, as the list holds them.
fn listed(snapshot: &work::Work, view: &View) -> Vec<Offer> {
    let mut waiting = crate::session::asked::capture(snapshot, 27);
    let shown = Shown {
        view,
        run: None,
        saved: None,
        jev: None,
        keys: &[],
        testable: false,
        checked: None,
        voice: crate::lang::Voice::default(),
    };
    crate::session::intelligence::project(&mut waiting, snapshot, &shown);
    rows(&waiting)
}

/// A connection's row counts the models its own list shows, the set that list's title counts,
/// and says the older ones behind its « Older models » row apart (live gauntlet, 16 October:
/// « openrouter · API · 375 models » opened on « 1-50 of 280 »). The list's moves (older
/// models, every connection) are no choice its title counts.
#[test]
fn a_connections_row_counts_the_models_its_list_shows() {
    let room = Room::new("home-model-count");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    let mut options: Vec<work::Offer> = (0..60)
        .map(|n| {
            let model = format!("openrouter/vendor/model-{n}");
            priced(&format!("2 {model}"), &model, "openrouter", (1.0, 2.0))
        })
        .collect();
    options.extend((0..5).map(|n| {
        let model = format!("openrouter/x-ai/grok-3-2025021{n}");
        priced(&format!("2 {model}"), &model, "openrouter", (1.0, 2.0))
    }));
    intelligence.options = options;
    let connections = listed(&snapshot, &View::Connections);
    let row = row(&connections, "/intelligence models 2 openrouter");
    assert_eq!(row.label, "openrouter · API · 60 models · 5 older");
    let models = listed(&snapshot, &View::Models("2 openrouter".to_owned()));
    let choices: Vec<&Offer> = models.iter().filter(|offer| !offer.is_aside()).collect();
    assert_eq!(choices.len(), 60, "{models:#?}");
    assert!(
        choices
            .iter()
            .all(|offer| offer.key.starts_with("2 openrouter/"))
    );
    let moves: Vec<&str> = (models.iter())
        .filter(|offer| offer.is_aside())
        .map(|offer| offer.label.as_str())
        .collect();
    assert_eq!(moves, ["Older models (5)…", "All connections"]);
    let older = listed(&snapshot, &View::Older("2 openrouter".to_owned()));
    assert_eq!(older.iter().filter(|offer| !offer.is_aside()).count(), 5);
}

/// Within each access the connections usable now lead (live gauntlet, 16 October: with only
/// Anthropic's key present, its row stood 12th of the APIs, under eleven « key required »): an
/// API whose key is present comes first, the rows that need a key keep their order under it,
/// and the agent account and the local server, which the Session detected, keep their places.
/// Nothing is probed to tell.
#[test]
fn the_connections_usable_now_lead_their_access() {
    let room = Room::new("home-ready-first");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    let missing = [
        "deepseek",
        "mistral",
        "openai",
        "xai",
        "gemini",
        "groq",
        "moonshot",
        "openrouter",
        "together",
        "fireworks",
        "cerebras",
    ];
    let mut options = vec![work::Offer::new(
        "1 claude/sonnet",
        "claude/sonnet · agent account",
        false,
    )];
    options.extend(missing.iter().map(|provider| {
        let variable = provider.to_uppercase();
        let label = format!("{provider} API · key required · configure {variable}_API_KEY");
        work::Offer::new(format!("2 {provider}"), label, false)
    }));
    options.push(priced(
        "2 anthropic/claude-sonnet-5-5",
        "anthropic/claude-sonnet-5-5",
        "anthropic",
        (3.0, 15.0),
    ));
    options.push(work::Offer::new(
        "3 ollama",
        "ollama · configured local route (not probed)",
        false,
    ));
    intelligence.options = options;
    let rows = listed(&snapshot, &View::Connections);
    let labels: Vec<&str> = (rows.iter())
        .filter(|offer| !offer.hidden())
        .map(|offer| offer.label.as_str())
        .collect();
    let apis: Vec<&str> = (labels.iter().copied())
        .filter(|label| label.contains(" · API"))
        .collect();
    assert_eq!(apis.len(), 12, "{labels:#?}");
    assert_eq!(apis[0], "anthropic · API · 1 model", "{labels:#?}");
    let rest: Vec<&str> = (apis[1..].iter())
        .map(|label| label.split(" · ").next().unwrap_or_default())
        .collect();
    assert_eq!(rest, missing, "the rows that need a key keep their order");
    assert!(
        apis[1..]
            .iter()
            .all(|label| label.ends_with(" · key required")),
        "{labels:#?}"
    );
    assert_eq!(
        labels.first().copied(),
        Some("claude · Agent account · 1 model")
    );
    assert_eq!(
        labels.last().copied(),
        Some("ollama · Local"),
        "{labels:#?}"
    );
}

/// `xai` typed home finds the xAI models first, together, then a Hugging Face model whose id
/// holds the word, under its own provider; the family's newest model is marked once in the list
/// (`Grok 4.7` over `Grok 4` and Hugging Face's `Grok 2`), never a dated or legacy one. Over
/// every connection, `xai` names the xAI connection and narrows the list to it: its models are
/// its list's (live gauntlet 6, r11: « openrouter » matched 376 of 376 rows).
#[test]
fn a_provider_typed_is_found_first_and_each_provider_keeps_its_rows_together() {
    let room = Room::new("home-provider-first");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![
        priced(
            "2 huggingface/xai-org/grok-2",
            "huggingface/xai-org/grok-2",
            "huggingface",
            (2.0, 10.0),
        ),
        priced(
            "2 huggingface/qwen/qwen3-8b",
            "huggingface/qwen/qwen3-8b",
            "huggingface",
            (0.1, 0.3),
        ),
        priced("2 xai/grok-4", "xai/grok-4", "xai", (3.0, 15.0)),
        priced("2 xai/grok-4.7", "xai/grok-4.7", "xai", (3.0, 15.0)),
        priced(
            "2 xai/grok-3-20250219",
            "xai/grok-3-20250219",
            "xai",
            (3.0, 15.0),
        ),
    ];
    let connections = found(&snapshot, &View::Connections, "xai");
    let labels: Vec<&str> = (connections.iter())
        .map(|(label, _)| label.as_str())
        .collect();
    assert_eq!(labels, ["xai · API · 2 models · 1 older"]);
    let found = found(&snapshot, &View::Home, "xai");
    let labels: Vec<&str> = found.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Grok 4 · xAI API",
            "Grok 4.7 · xAI API",
            "Grok 3 (20250219) · xAI API",
            "Grok 2 · Hugging Face API",
        ],
        "{found:#?}"
    );
    let newest: Vec<&str> = (found.iter())
        .filter(|(_, note)| note.starts_with("newest "))
        .map(|(label, _)| label.as_str())
        .collect();
    assert_eq!(newest, ["Grok 4.7 · xAI API"], "{found:#?}");
    assert!(found[1].1.starts_with("newest Grok · "), "{found:#?}");
}

/// The model the Session recommends is the one its provider marks; no other row of that
/// provider says newest.
#[test]
fn a_provider_marks_the_recommended_model_rather_than_its_newest() {
    let room = Room::new("home-provider-recommended");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![
        priced("2 xai/grok-4", "xai/grok-4", "xai", (3.0, 15.0)),
        priced("2 xai/grok-4.7", "xai/grok-4.7", "xai", (3.0, 15.0)),
    ];
    intelligence.recommended = Some("2 xai/grok-4".to_owned());
    let found = found(&snapshot, &View::Models("2 xai".to_owned()), "");
    let notes: Vec<(&str, &str)> = (found.iter())
        .map(|(label, note)| (label.as_str(), note.as_str()))
        .collect();
    assert!(notes[0].1.starts_with("recommended · "), "{notes:#?}");
    assert!(
        !notes.iter().any(|(_, note)| note.contains("newest")),
        "{notes:#?}"
    );
}

/// Where a model writes (the author, a Run step), no embedding model is offered: neither one
/// the catalogue prices for input alone nor one whose name says it embeds or reranks.
#[test]
fn the_lists_where_a_model_writes_offer_no_embedding_model() {
    let room = Room::new("home-no-embedding");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![
        priced(
            "2 openai/gpt-6.1-sol",
            "openai/gpt-6.1-sol",
            "openai",
            (2.0, 10.0),
        ),
        priced(
            "2 openai/text-embedding-3-large",
            "openai/text-embedding-3-large",
            "openai",
            (0.13, 0.0),
        ),
        priced(
            "2 mistral/mistral-embed",
            "mistral/mistral-embed",
            "mistral",
            (0.0, 0.0),
        ),
        priced(
            "2 nvidia/nvidia/llama-nemotron-rerank-vl-1b-v2",
            "nvidia/nvidia/llama-nemotron-rerank-vl-1b-v2",
            "nvidia",
            (0.0, 0.0),
        ),
        priced(
            "2 nvidia/meta/llama-4-maverick-17b-128e-instruct",
            "nvidia/meta/llama-4-maverick-17b-128e-instruct",
            "nvidia",
            (0.0, 0.0),
        ),
    ];
    let run = found(&snapshot, &View::Run, "");
    let labels: Vec<&str> = run.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(
        labels,
        ["GPT-6.1 Sol", "Llama 4 Maverick 17B 128e Instruct"],
        "a free model that writes stays: {run:#?}"
    );
    for typed in ["embed", "rerank", "text-embedding"] {
        let author = found(&snapshot, &View::Connections, typed);
        assert!(
            !author
                .iter()
                .any(|(label, _)| label.to_lowercase().contains("embed")
                    || label.to_lowercase().contains("rerank")),
            "{typed}: {author:#?}"
        );
    }
}

/// The list's own navigation says nothing in the conversation: opening it, then its
/// connections, then one connection's models each guide the list's head (« Choose a connection
/// below. No selection has changed. »), and keeping the current choice closes it with that
/// guide gone, so the history keeps no « below » before the choice (live gauntlet r02 to r09).
#[test]
fn the_lists_navigation_guides_its_head_and_says_nothing() {
    let room = Room::new("home-guide");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let mut said = Vec::new();
    for (line, guided) in [
        ("/intelligence", "Choose an intelligence below."),
        (
            "/intelligence connections",
            "Choose a connection below. No selection has changed.",
        ),
        (
            "/intelligence models 2 deepseek",
            "No selection has changed.",
        ),
    ] {
        let turn = live.submit(line);
        assert!(guide(&turn.beats).contains(guided), "{line}: {turn:?}");
        said.extend(joined(&turn.beats).lines().map(str::to_owned));
    }
    let kept = live.submit("cancel");
    assert!(
        !matches!(waits(&kept.beats), Some(Waiting::Selection { .. })),
        "keeping the current choice closes the list: {kept:?}"
    );
    said.extend(joined(&kept.beats).lines().map(str::to_owned));
    assert!(
        !said.iter().any(|line| line.contains("below")),
        "no guide in the conversation: {said:#?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no model was called");
}

/// One « newest » per family in a list, by real recency (live gauntlet r07 B5, r01 B7): typed
/// `deepseek` home, where models are reached by name, the `DeepSeek` API's V4 Pro says
/// « newest `DeepSeek` », never Scaleway's dated V4 Flash 0731, Hugging Face's R1 0528 (a date,
/// no version) or its V3; typed `xai`, Grok 4.7 says « newest Grok », never the dated Grok
/// 4.20 0309.
#[test]
fn a_family_is_marked_newest_once_by_real_recency() {
    let room = Room::new("home-newest-once");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![
        priced(
            "2 deepseek/deepseek-v4-pro",
            "deepseek/deepseek-v4-pro",
            "deepseek",
            (1.32, 3.96),
        ),
        priced(
            "2 scaleway/deepseek-v4-flash-0731",
            "scaleway/deepseek-v4-flash-0731",
            "scaleway",
            (0.5, 1.0),
        ),
        priced(
            "2 huggingface/deepseek-ai/deepseek-r1-0528",
            "huggingface/deepseek-ai/deepseek-r1-0528",
            "huggingface",
            (3.0, 5.0),
        ),
        priced(
            "2 huggingface/deepseek-ai/deepseek-v3",
            "huggingface/deepseek-ai/deepseek-v3",
            "huggingface",
            (1.0, 2.0),
        ),
        priced("2 xai/grok-4.7", "xai/grok-4.7", "xai", (2.0, 6.0)),
        priced(
            "2 xai/grok-4.20-0309-non-reasoning",
            "xai/grok-4.20-0309-non-reasoning",
            "xai",
            (2.0, 6.0),
        ),
    ];
    for (typed, family, expected) in [
        (
            "deepseek",
            "newest DeepSeek",
            "DeepSeek V4 Pro · DeepSeek API",
        ),
        ("xai", "newest Grok", "Grok 4.7 · xAI API"),
    ] {
        let found = found(&snapshot, &View::Home, typed);
        let marked: Vec<&str> = (found.iter())
            .filter(|(_, note)| note.contains(family))
            .map(|(label, _)| label.as_str())
            .collect();
        assert_eq!(marked, [expected], "{typed}: {found:#?}");
    }
}

/// The model in use is the one row of its connection's list that holds the value: its radio
/// stays filled wherever the cursor moves (live gauntlet r02 B2, r06 B9).
#[test]
fn the_model_in_use_is_the_row_its_list_holds() {
    let room = Room::new("home-in-use");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![
        model("2 xai/grok-4.7", "xai/grok-4.7", ("xai", "api"), false),
        model("2 xai/grok-4", "xai/grok-4", ("xai", "api"), true),
    ];
    let mut waiting = crate::session::asked::capture(&snapshot, 27);
    let shown = Shown {
        view: &View::Models("2 xai".to_owned()),
        run: None,
        saved: None,
        jev: None,
        keys: &[],
        testable: false,
        checked: None,
        voice: crate::lang::Voice::default(),
    };
    crate::session::intelligence::project(&mut waiting, &snapshot, &shown);
    let held: Vec<String> = (rows(&waiting).into_iter())
        .filter(Offer::is_in_use)
        .map(|offer| offer.key)
        .collect();
    assert_eq!(held, ["2 xai/grok-4"], "{waiting:#?}");
}

/// A seat the Session says cannot write workflows says so on its connection's row and on its
/// own row, before any choice; a seat that can says nothing of the kind (live gauntlet r08 B1).
#[test]
fn a_seat_that_cannot_author_says_so_before_any_choice() {
    let room = Room::new("home-no-authoring");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    pinned(&mut live, None);
    let _ = live.submit("/intelligence");
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = vec![
        work::Offer::new(
            "1 copilot",
            "copilot · agent account · cannot write workflows here",
            false,
        ),
        work::Offer::new("1 claude-code", "claude-code · agent account", false),
    ];
    let listed = found(&snapshot, &View::Connections, "");
    let row = |name: &str| {
        (listed.iter())
            .find(|(label, _)| label.starts_with(name))
            .map(|(label, _)| label.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        row("copilot"),
        "copilot · Agent account · cannot write workflows here",
        "{listed:#?}"
    );
    assert_eq!(
        row("claude-code"),
        "claude-code · Agent account",
        "{listed:#?}"
    );
    let models = found(&snapshot, &View::Models("1 copilot".to_owned()), "");
    assert!(
        (models.iter()).any(|(label, _)| label.contains("cannot write workflows here")),
        "{models:#?}"
    );
}
