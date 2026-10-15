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
    let feedback = opened
        .beats
        .iter()
        .filter_map(|beat| match beat {
            Beat::Say(words) => Some(words.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(feedback.contains("Configure the API key"), "{feedback}");
    assert!(!feedback.contains("Choose a model below"), "{feedback}");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(
        !live
            .runtime
            .as_ref()
            .expect("runtime")
            .intelligence_chosen()
    );
}

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
    assert!(
        offers
            .iter()
            .any(|offer| offer.key == "/intelligence models 2 deepseek")
    );
    assert!(
        offers
            .iter()
            .all(|offer| !matches!(offer.key.as_str(), "4" | "cancel"))
    );
    waiting
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
        for code in [
            KeyCode::Esc,
            KeyCode::Down,
            KeyCode::Up,
            KeyCode::Home,
            KeyCode::End,
        ] {
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
    crate::session::intelligence::project(&mut waiting, &snapshot, None);
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
    crate::session::intelligence::project(&mut waiting, &snapshot, Some("2 openrouter"));
    let Waiting::Selection { asked } = &waiting else {
        panic!("models")
    };
    let Shape::Choice(offers) = &asked.shape else {
        panic!("models")
    };
    assert_eq!(offers.len(), 221);
    assert_eq!(offers[0].key, "2 openrouter/model-0");
    assert_eq!(offers[219].key, "2 openrouter/model-219");
    assert_eq!(offers[220].key, "/intelligence");
    let opened = live.submit("/intelligence models 2 deepseek");
    let Waiting::Selection { asked } = waits(&opened.beats).expect("models") else {
        panic!("models")
    };
    let Shape::Choice(offers) = asked.shape else {
        panic!("models")
    };
    assert_eq!(
        offers.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
        ["2 deepseek", "/intelligence"]
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
    crate::session::intelligence::project(&mut waiting, &snapshot, None);
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
        [
            "deepseek · API · 2 models · key required",
            "openrouter · API · key required",
            "moonshot · API · 2 models",
            "anthropic · API",
            "mistral · API · 1 model",
        ]
    );
    crate::session::intelligence::project(&mut waiting, &snapshot, Some("2 openrouter"));
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
    assert_eq!(offers[1].key, "/intelligence");
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
