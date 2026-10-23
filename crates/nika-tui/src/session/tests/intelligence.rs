// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::composer::{Composer, answer::Offered};
use crate::model::Shape;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn choosing(
    room: &Room,
    pref: Option<UserIntelligencePreference>,
    calls: &Arc<AtomicUsize>,
) -> Live {
    let mut census = IntelligenceCensus::empty();
    census.api_keys.push("deepseek".into());
    let calls = Arc::clone(calls);
    Live::new(
        room.0.clone(),
        census,
        pref,
        None,
        Box::new(move |_| Box::new(Route(Arc::clone(&calls)))),
        runners(),
    )
}

/// Before the person writes, the TUI's own words take this machine's locale (live gauntlet 6,
/// r04 and r10: the top bar and the first screen spoke English until the first French line):
/// under a French locale the opening list and its guide speak French and the shell reads
/// French; the person's English line then decides, and a host given no locale opens in
/// English.
#[test]
fn the_first_screen_speaks_the_locale_until_the_persons_words_decide() {
    use crate::model::Conversation as _;
    use nika_session::Lang;
    let room = Room::new("intelligence-locale");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = choosing(&room, None, &calls).with_locale(Lang::Fr);
    let Waiting::Selection { asked } = required(&live.open()) else {
        panic!("the opening list");
    };
    assert_eq!(asked.label, "Intelligence de cette conversation");
    let guide = asked.guide.unwrap_or_default();
    assert!(
        guide.starts_with("Choisissez une intelligence ci-dessous."),
        "{guide}"
    );
    assert_eq!(live.language(), Some(Lang::Fr));
    let _ = live.submit("please read the notes and write a summary of them for me");
    assert_eq!(live.language(), Some(Lang::En), "their words decide");
    let plain_room = Room::new("intelligence-no-locale");
    let mut plain = choosing(&plain_room, None, &calls);
    let Waiting::Selection { asked } = required(&plain.open()) else {
        panic!("the opening list");
    };
    assert_eq!(asked.label, "Intelligence for this conversation");
    assert_eq!(plain.language(), Some(Lang::En));
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no model was called");
}

#[test]
fn opening_api_configuration_explains_the_key_without_offering_a_missing_model() {
    let room = Room::new("api-configuration-guidance");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = choosing(&room, None, &calls);
    required(&live.open());
    let opened = live.submit("/intelligence models 2 anthropic");
    let Waiting::Selection { asked } = waits(&opened.beats).expect("API configuration") else {
        panic!("API configuration stays open");
    };
    assert_eq!(asked.label, "Configure API · anthropic");
    let feedback = asked.guide.clone().unwrap_or_default();
    assert!(feedback.contains("Configure the API key"), "{feedback}");
    assert!(!feedback.contains("Choose a model below"), "{feedback}");
    assert!(
        !joined(&opened.beats).contains("Configure the API key"),
        "the list's guide is never a message: {opened:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(
        !live
            .runtime
            .as_ref()
            .expect("runtime")
            .intelligence_chosen()
    );
}

/// The opening guide and an opened connection guide by their rows alone: the typed
/// route stays an expert fallback (the next test still applies one) but is never taught.
#[test]
fn the_normal_path_never_teaches_the_typed_route() {
    let room = Room::new("no-typed-route");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = choosing(&room, None, &calls);
    let opened = live.open();
    required(&opened);
    let models = live.submit("/intelligence models 2 deepseek");
    for said in [guide(&opened), guide(&models.beats)] {
        assert!(said.contains("below"), "{said}");
        for taught in ["/intelligence 2", "provider/model", "To name a model"] {
            assert!(!said.contains(taught), "{taught}: {said}");
        }
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// The opening list guides its own head: that a message typed now waits for the choice is
/// said there while the list is open, never in the conversation, so the welcome card claims no
/// pending choice once one is made (live gauntlet, 16 October); the list's head is cleared when
/// the list closes.
#[test]
fn the_opening_guide_claims_a_pending_choice_only_while_the_list_is_open() {
    let room = Room::new("opening-guide-head");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = choosing(&room, None, &calls);
    let opened = live.open();
    required(&opened);
    let claim = "A message typed here waits for your choice.";
    assert!(guide(&opened).contains(claim), "{opened:?}");
    assert!(
        !joined(&opened).contains("waits for your choice"),
        "the welcome card claims nothing: {opened:?}"
    );
    // Once chosen, the list closes and nothing the conversation keeps says a choice waits.
    let chosen = live.submit("2 deepseek/deepseek-chat");
    assert_eq!(waits(&chosen.beats), Some(Waiting::Free), "{chosen:?}");
    assert!(
        !joined(&chosen.beats).contains("waits for your choice"),
        "{chosen:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "nothing waited to resume");
}

/// A kept intelligence this machine cannot serve is said by the opening list's head while the
/// list is open, never on the welcome card, which would keep it after another intelligence is
/// chosen; reopened over the same home, the card still says none of it (live gauntlet wave 11,
/// 2026-10-20: all twelve personas read « ⚠ no key for `anthropic` » there after choosing
/// `deepseek`).
#[test]
fn the_welcome_card_never_keeps_the_warning_of_an_intelligence_not_in_use() {
    let room = Room::new("welcome-warning");
    let home = Room::new("welcome-warning-home");
    let calls = Arc::new(AtomicUsize::new(0));
    let open = || {
        let mut census = IntelligenceCensus::empty();
        census.api_keys.push("deepseek".into());
        let kept = UserIntelligencePreference::new(
            IntelligenceKind::Api {
                provider: "anthropic".into(),
            },
            None,
        );
        let calls = Arc::clone(&calls);
        let mut live = Live::new(
            room.0.clone(),
            census,
            Some(kept),
            Some(home.0.clone()),
            Box::new(move |_| Box::new(Route(Arc::clone(&calls)))),
            runners(),
        );
        let opened = live.open();
        (live, opened)
    };
    let welcome = |beats: &[Beat]| {
        (said(beats).into_iter())
            .find_map(|(kind, text)| (kind == Kind::Banner).then_some(text))
            .unwrap_or_else(|| panic!("the welcome card: {beats:?}"))
    };
    let (mut first, opened) = open();
    assert!(
        guide(&opened).contains("no key for `anthropic`"),
        "the list says it while it is open: {opened:?}"
    );
    assert!(welcome(&opened).starts_with("Nika · "), "{opened:?}");
    assert!(!welcome(&opened).contains('⚠'), "{opened:?}");
    let chosen = first.submit("2 deepseek/deepseek-chat");
    assert_eq!(waits(&chosen.beats), Some(Waiting::Free), "{chosen:?}");
    assert!(!joined(&chosen.beats).contains("no key for"), "{chosen:?}");
    drop(first);
    let (_, reopened) = open();
    assert!(!welcome(&reopened).contains('⚠'), "{reopened:?}");
    assert!(!joined(&reopened).contains("no key for"), "{reopened:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no model was called");
}

/// The required guide's home screen: every other choice and the three roles, nothing to keep
/// and no offline choice.
fn required(beats: &[Beat]) -> Waiting {
    let waiting = waits(beats).expect("choice stays visible");
    let Waiting::Selection { asked } = &waiting else {
        panic!("native options: {beats:?}")
    };
    assert!(asked.mandatory);
    assert!(asked.free_text);
    let Shape::Choice(offers) = &asked.shape else {
        panic!("single choice")
    };
    let keys: Vec<&str> = offers.iter().map(|offer| offer.key.as_str()).collect();
    for row in [
        "/intelligence connections",
        "/intelligence author",
        "/intelligence decision",
        "/intelligence run",
    ] {
        assert!(keys.contains(&row), "{row}: {keys:?}");
    }
    assert!(
        keys.iter()
            .all(|key| !matches!(*key, "4" | "cancel" | "/intelligence default")),
        "{keys:?}"
    );
    waiting
}

/// What the picker paints for `view`, nothing else held by the host.
fn shown(view: &crate::session::intelligence::View) -> crate::session::intelligence::Shown<'_> {
    crate::session::intelligence::Shown {
        view,
        run: None,
        saved: None,
        jev: None,
        keys: &[],
        testable: false,
        checked: None,
        voice: crate::lang::Voice::default(),
    }
}

#[test]
fn missing_offline_or_unavailable_selection_opens_the_native_guide() {
    let room = Room::new("required-intelligence");
    let calls = Arc::new(AtomicUsize::new(0));
    for pref in [
        None,
        Some(UserIntelligencePreference::new(
            IntelligenceKind::None,
            None,
        )),
        Some(UserIntelligencePreference::new(
            IntelligenceKind::Api {
                provider: "missing".into(),
            },
            None,
        )),
    ] {
        let mut live = choosing(&room, pref, &calls);
        let waiting = required(&live.open());
        let mut composer = Composer::new();
        composer.set_focused(true);
        composer.paste("my ordinary draft");
        composer.follow(&waiting);
        // A required choice has nothing to keep: Esc says so and sends nothing.
        assert!(matches!(
            composer.offer_key(&waiting, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Offered::Notice(words) if words.contains("required")
        ));
        for code in [KeyCode::Down, KeyCode::Up, KeyCode::Home, KeyCode::End] {
            assert_eq!(
                composer.offer_key(&waiting, KeyEvent::new(code, KeyModifiers::NONE)),
                Offered::Moved
            );
        }
        assert_eq!(composer.text(), "my ordinary draft");
        for line in ["2 unknown-provider", "9", "2bad"] {
            let turn = live.submit(line);
            required(&turn.beats);
            assert!(
                turn.beats
                    .iter()
                    .any(|beat| { matches!(beat, Beat::NotTaken(words) if words == line) }),
                "unselected input remains editable: {turn:?}"
            );
        }
        for line in [
            "/intelligence",
            "API",
            "2 API",
            "cancel",
            "/cancel",
            "annule",
            "4",
            "/intelligence 4",
            "2 unknown-provider",
        ] {
            let turn = live.submit(line);
            required(&turn.beats);
            assert!(turn.handoff.is_none());
            if matches!(line, "4" | "/intelligence 4") {
                assert!(turn.beats.iter().any(|beat| {
                    matches!(beat, Beat::Say(words) if words.text.contains("offline choice was not applied"))
                }));
            }
        }
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "selection never calls a model"
        );
        composer.paste_own("2 deepseek/deepseek-chat");
        let Offered::Answer(answer) =
            composer.offer_key(&waiting, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        else {
            panic!("free model name")
        };
        assert_eq!(answer, "2 deepseek/deepseek-chat");
        let selected = live.submit(&answer);
        assert_eq!(waits(&selected.beats), Some(Waiting::Free));
        let runtime = live.runtime.as_ref().expect("runtime");
        assert_eq!(
            runtime.intelligence.kind,
            IntelligenceKind::Api {
                provider: "deepseek".into()
            }
        );
        assert_eq!(
            runtime.intelligence.model.as_deref(),
            Some("deepseek/deepseek-chat")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn cancelling_a_change_keeps_the_ready_route_and_the_draft() {
    let room = Room::new("keep-intelligence");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let waiting = waits(&live.submit("/intelligence").beats).expect("selector");
    let Waiting::Selection { asked } = &waiting else {
        panic!("native selector")
    };
    assert!(!asked.mandatory);
    let mut composer = Composer::new();
    composer.set_focused(true);
    composer.paste("saved draft");
    composer.follow(&waiting);
    let Offered::Answer(cancel) =
        composer.offer_key(&waiting, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
    else {
        panic!("cancel this change")
    };
    assert_eq!(cancel, "cancel");
    assert_eq!(waits(&live.submit(&cancel).beats), Some(Waiting::Free));
    assert_eq!(
        live.runtime
            .as_ref()
            .expect("runtime")
            .intelligence
            .model
            .as_deref(),
        Some(MODEL)
    );
    assert_eq!(composer.text(), "saved draft");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn connection_groups_keep_the_complete_catalogue_and_open_without_applying() {
    let room = Room::new("intelligence-groups");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = choosing(&room, None, &calls);
    required(&live.open());
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = (0..180)
        .map(|i| {
            nika_session::work::Offer::new(
                format!("2 deepseek/model-{i}"),
                format!("DeepSeek model {i}"),
                i == 179,
            )
        })
        .chain((0..220).map(|i| {
            nika_session::work::Offer::new(
                format!("2 openrouter/model-{i}"),
                format!("OpenRouter model {i}"),
                false,
            )
        }))
        .collect();
    let mut waiting = crate::session::asked::capture(&snapshot, 11);
    let connections = crate::session::intelligence::View::Connections;
    crate::session::intelligence::project(&mut waiting, &snapshot, &shown(&connections));
    let Waiting::Selection { asked } = &waiting else {
        panic!("groups")
    };
    let Shape::Choice(offers) = &asked.shape else {
        panic!("groups")
    };
    assert_eq!(
        offers.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
        [
            "2 deepseek/model-179",
            "/intelligence models 2 deepseek",
            "/intelligence models 2 openrouter",
        ]
    );
    let opened = crate::session::intelligence::View::Models("2 openrouter".into());
    crate::session::intelligence::project(&mut waiting, &snapshot, &shown(&opened));
    let Waiting::Selection { asked } = &waiting else {
        panic!("models")
    };
    let Shape::Choice(offers) = &asked.shape else {
        panic!("models")
    };
    assert_eq!(offers.len(), 221);
    assert_eq!(offers[0].key, "2 openrouter/model-0");
    assert_eq!(offers[219].key, "2 openrouter/model-219");
    assert_eq!(offers[220].key, "/intelligence connections");
    assert_eq!(asked.back.as_deref(), Some("/intelligence connections"));
    let opened = live.submit("/intelligence models 2 deepseek");
    let Waiting::Selection { asked } = waits(&opened.beats).expect("models") else {
        panic!("models")
    };
    let Shape::Choice(offers) = asked.shape else {
        panic!("models")
    };
    assert_eq!(
        offers.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
        ["2 deepseek", "/intelligence connections"]
    );
    assert!(
        !live
            .runtime
            .as_ref()
            .expect("runtime")
            .intelligence_chosen()
    );
    required(&live.submit("/intelligence").beats);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn api_groups_show_missing_keys_without_counting_provider_setup_as_a_model() {
    let room = Room::new("intelligence-missing-key-groups");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = choosing(&room, None, &calls);
    required(&live.open());
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let fallback = "openrouter API · key required · configure OPENROUTER_API_KEY";
    snapshot
        .intelligence
        .as_mut()
        .expect("intelligence")
        .options = [
        (
            "2 deepseek/flash",
            "Flash · key required · configure DEEPSEEK_API_KEY",
        ),
        (
            "2 deepseek/pro",
            "Pro · key required · configure DEEPSEEK_API_KEY",
        ),
        ("2 openrouter", fallback),
        (
            "2 moonshot/kimi",
            "Kimi · key required · configure MOONSHOT_API_KEY",
        ),
        ("2 moonshot/ready", "Ready · API"),
        ("2 anthropic", "anthropic API"),
        ("2 mistral/small", "Small · API"),
    ]
    .into_iter()
    .map(|(key, label)| nika_session::work::Offer::new(key, label, false))
    .collect();
    let mut waiting = crate::session::asked::capture(&snapshot, 12);
    let connections = crate::session::intelligence::View::Connections;
    crate::session::intelligence::project(&mut waiting, &snapshot, &shown(&connections));
    let Waiting::Selection { asked } = &waiting else {
        panic!("groups")
    };
    let Shape::Choice(offers) = &asked.shape else {
        panic!("groups")
    };
    assert_eq!(
        offers
            .iter()
            .map(|offer| offer.label.as_str())
            .collect::<Vec<_>>(),
        // The APIs usable now lead, each side in its order (the rows that need a key last).
        [
            "moonshot · API · 2 models",
            "anthropic · API",
            "mistral · API · 1 model",
            "deepseek · API · 2 models · key required",
            "openrouter · API · key required",
        ]
    );
    let opened = crate::session::intelligence::View::Models("2 openrouter".into());
    crate::session::intelligence::project(&mut waiting, &snapshot, &shown(&opened));
    let Waiting::Selection { asked } = &waiting else {
        panic!("provider setup")
    };
    assert_eq!(asked.label, "Configure API · openrouter");
    let Shape::Choice(offers) = &asked.shape else {
        panic!("provider setup")
    };
    assert_eq!(offers.len(), 2);
    assert_eq!(offers[0].key, "2 openrouter");
    assert_eq!(offers[0].label, fallback);
    assert_eq!(offers[1].key, "/intelligence connections");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn cancelling_or_refreshing_the_initial_choice_keeps_the_interrupted_request() {
    let room = Room::new("kept-intelligence-request");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = choosing(&room, None, &calls);
    required(&live.open());
    let kept = live.submit("hello");
    required(&kept.beats);
    assert!(kept.beats.iter().any(|beat| {
        matches!(beat, Beat::Say(words) if words.text.contains("Your message is kept"))
    }));
    assert!(
        !kept
            .beats
            .iter()
            .any(|beat| matches!(beat, Beat::NotTaken(_)))
    );
    for line in [
        "cancel",
        "annuler",
        "/intelligence",
        "API",
        "4",
        "2 unknown-provider",
    ] {
        required(&live.submit(line).beats);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    let resumed = live.submit("2 deepseek/deepseek-chat");
    assert_eq!(waits(&resumed.beats), Some(Waiting::Free));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the greeting resumes once");
    let _ = live.submit("/intelligence");
    let _ = live.submit("2 deepseek/deepseek-chat");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the kept request is consumed"
    );
}

/// Words typed over the open list that name no row are a message, never a choice: the list
/// closes with the current choice kept, one notice says the intelligence is unchanged, the
/// conversation takes the words once and nothing teaches a typed id. A pick, a command or a
/// cancel still belongs to the list.
#[test]
fn a_sentence_over_the_list_is_a_message_and_the_intelligence_stays() {
    let room = Room::new("sentence-over-list");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = session_live(&room, &calls);
    let waiting = waits(&live.submit("/intelligence").beats).expect("selector");
    assert!(matches!(waiting, Waiting::Selection { .. }), "{waiting:?}");
    for listed in ["2 deepseek/deepseek-chat", "cancel", "/status", "4", ""] {
        assert!(!live.converses(listed), "{listed:?}");
    }
    let sentence = "summarise my inbox every morning";
    assert!(live.converses(sentence));
    let sent = live.submit(sentence);
    let said = joined(&sent.beats);
    assert!(
        matches!(sent.beats.first(), Some(Beat::Say(block))
            if block.kind == Kind::Notice && block.text == "intelligence unchanged"),
        "{:?}",
        sent.beats
    );
    // The refusal that taught typed ids (`1 <app>/<model>`) is never reached by a sentence.
    for taught in [
        "is not a choice",
        "answer a number alone",
        "1 <app>/<model>",
    ] {
        assert!(!said.contains(taught), "{taught}: {said}");
    }
    assert!(
        !matches!(waits(&sent.beats), Some(Waiting::Selection { .. })),
        "the list closed: {:?}",
        sent.beats
    );
    let runtime = live.runtime.as_ref().expect("runtime");
    assert_eq!(runtime.intelligence.model.as_deref(), Some(MODEL));
    assert!(!runtime.pending_choice());
    let goal = runtime.work().request.goal;
    assert!(
        goal.as_deref().is_some_and(|goal| goal.contains(sentence)),
        "the conversation took the words as its request: {goal:?}"
    );
    assert!(!live.converses("another sentence"), "no list is open");
}

/// Without a usable choice the list is required: a sentence is the Session's kept message,
/// never a conversation turn sent around the choice.
#[test]
fn a_sentence_over_a_required_list_waits_for_the_choice() {
    let room = Room::new("sentence-over-required-list");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = choosing(&room, None, &calls);
    required(&live.open());
    assert!(!live.converses("summarise my inbox every morning"));
    let kept = live.submit("summarise my inbox every morning");
    required(&kept.beats);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// The rows of the list `view` paints over `snapshot` that are marked in use, by key and note.
fn marked_in(
    snapshot: &nika_session::work::Work,
    view: &crate::session::intelligence::View,
) -> Vec<(String, String)> {
    let mut waiting = crate::session::asked::capture(snapshot, 13);
    crate::session::intelligence::project(&mut waiting, snapshot, &shown(view));
    let Waiting::Selection { asked } = &waiting else {
        panic!("a settings list")
    };
    let Shape::Choice(offers) = &asked.shape else {
        panic!("a settings list")
    };
    (offers.iter())
        .filter(|offer| offer.current)
        .map(|offer| (offer.key.clone(), offer.note.clone()))
        .collect()
}

/// The connection in use is marked as models are (live gauntlet r06 B14: « ( ) codex · Agent
/// account » while codex authored, with no « (●) » and no « current »): among every connection
/// its facts read « current » first, as its own option does in its list, whether only the
/// selection names it (an agent account listing no model) or the Session marks a model of it.
/// A list holds one filled radio (live gauntlet 5, r01 B11, r05 B11, r07 B10, r09 B14): the
/// connection's while no model of it leads the list, the model's when one does, the
/// connection then keeping its word alone. No other connection is marked.
#[test]
fn the_connection_in_use_is_marked_as_models_are() {
    use crate::session::intelligence::View;
    let room = Room::new("intelligence-current-connection");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = choosing(&room, None, &calls);
    required(&live.open());
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = [
        ("1 claude-code", "claude-code · agent account"),
        ("1 codex", "codex · agent account"),
        (
            "2 deepseek/deepseek-v4-pro",
            "deepseek-v4-pro · deepseek · api_metered",
        ),
    ]
    .into_iter()
    .map(|(key, label)| nika_session::work::Offer::new(key, label, false))
    .collect();
    intelligence.selected = Some(
        nika_session::work::Selected::new("harness", Some("codex".into()), Some("acp".into()))
            .for_conversation()
            .resolved(None, "codex".into(), None, true),
    );
    let codex = |key: &str| vec![(key.to_owned(), "current".to_owned())];
    assert_eq!(
        marked_in(&snapshot, &View::Connections),
        codex("/intelligence models 1 codex")
    );
    assert_eq!(
        marked_in(&snapshot, &View::Models("1 codex".into())),
        codex("1 codex")
    );
    // A model the Session marks in use names its connection, whatever the selection says.
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options[2].recommended = true;
    let marked = marked_in(&snapshot, &View::Connections);
    let keys: Vec<&str> = marked.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        keys,
        ["2 deepseek/deepseek-v4-pro"],
        "one radio: {marked:?}"
    );
    assert!(marked[0].1.starts_with("current"), "{marked:?}");
    let noted = noted_in(
        &snapshot,
        &View::Connections,
        "/intelligence models 2 deepseek",
    );
    assert!(noted.starts_with("current"), "its word stays: {noted}");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// The note of the row `key` in the list `view` paints over `snapshot`.
fn noted_in(
    snapshot: &nika_session::work::Work,
    view: &crate::session::intelligence::View,
    key: &str,
) -> String {
    let mut waiting = crate::session::asked::capture(snapshot, 13);
    crate::session::intelligence::project(&mut waiting, snapshot, &shown(view));
    let Waiting::Selection { asked } = &waiting else {
        panic!("a settings list")
    };
    let Shape::Choice(offers) = &asked.shape else {
        panic!("a settings list")
    };
    (offers.iter())
        .find(|offer| offer.key == key)
        .map_or_else(|| panic!("{key} listed"), |offer| offer.note.clone())
}

/// The list `view` paints over `snapshot` in French: its label and its rows' labels and notes.
fn painted_in_french(
    snapshot: &nika_session::work::Work,
    view: &crate::session::intelligence::View,
) -> (String, Vec<(String, String, String)>) {
    let mut waiting = crate::session::asked::capture(snapshot, 13);
    let mut shown = shown(view);
    shown.voice = crate::lang::Voice {
        lang: crate::lang::Lang::Fr,
        ascii: false,
    };
    crate::session::intelligence::project(&mut waiting, snapshot, &shown);
    let Waiting::Selection { asked } = &waiting else {
        panic!("a settings list")
    };
    let Shape::Choice(offers) = &asked.shape else {
        panic!("a settings list")
    };
    let rows = (offers.iter())
        .map(|offer| (offer.key.clone(), offer.label.clone(), offer.note.clone()))
        .collect();
    (asked.label.clone(), rows)
}

/// Under a French footer the picker says its own words in French (live gauntlet 5, r09 B10 and
/// the picker rows): « en cours », « Compte agent », « 1 modèle », « clé requise », « n'écrit
/// pas de workflow ici », the lists' titles and the home rows; the Session's labels, the models'
/// and providers' names and every key stay as they are.
#[test]
fn the_picker_says_its_own_words_in_french() {
    use crate::session::intelligence::View;
    let room = Room::new("intelligence-french");
    let calls = Arc::new(AtomicUsize::new(0));
    let mut live = choosing(&room, None, &calls);
    required(&live.open());
    let mut snapshot = live.runtime.as_ref().expect("runtime").work();
    let intelligence = snapshot.intelligence.as_mut().expect("intelligence");
    intelligence.options = [
        ("1 codex", "codex · agent account"),
        (
            "1 copilot",
            "copilot · agent account · cannot write workflows here",
        ),
        (
            "2 deepseek/deepseek-v4-pro",
            "deepseek-v4-pro · deepseek · api_metered",
        ),
        (
            "2 mistral",
            "mistral · api_metered · key required · configure MISTRAL_API_KEY",
        ),
    ]
    .into_iter()
    .map(|(key, label)| nika_session::work::Offer::new(key, label, false))
    .collect();
    intelligence.selected = Some(
        nika_session::work::Selected::new("harness", Some("codex".into()), Some("acp".into()))
            .for_conversation()
            .resolved(None, "codex".into(), None, true),
    );
    let (label, rows) = painted_in_french(&snapshot, &View::Connections);
    assert_eq!(label, "Choisir une connexion IA");
    let row = |key: &str| {
        (rows.iter()).find(|(at, ..)| at == key).map_or_else(
            || panic!("{key}: {rows:?}"),
            |(_, label, note)| (label.clone(), note.clone()),
        )
    };
    assert_eq!(
        row("/intelligence models 1 codex"),
        ("codex · Compte agent".to_owned(), "en cours".to_owned())
    );
    assert_eq!(
        row("/intelligence models 1 copilot").0,
        "copilot · Compte agent · n'écrit pas de workflow ici"
    );
    assert_eq!(
        row("/intelligence models 2 deepseek").0,
        "deepseek · API · 1 modèle"
    );
    assert_eq!(
        row("/intelligence models 2 mistral").0,
        "mistral · API · clé requise"
    );
    let (label, rows) = painted_in_french(&snapshot, &View::Home);
    assert_eq!(label, "Intelligence de cette conversation");
    let other = (rows.iter())
        .find(|(key, ..)| key == "/intelligence connections")
        .expect("every connection");
    assert_eq!(
        (other.1.as_str(), other.2.as_str()),
        ("Autre choix…", "chaque connexion et chaque modèle")
    );
    assert!(
        (rows.iter())
            .any(|(key, label, _)| key == "/intelligence decision"
                && label.starts_with("Décision : ")),
        "{rows:?}"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
