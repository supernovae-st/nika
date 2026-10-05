// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The selection a conversation prepares with, in words: the author model and its connection,
//! then who judges what it prepares. These are configured or resolved facts read from Session,
//! never a served-model receipt: projecting them asks no model and sends nothing.

use nika_session::authoring::DecisionSetup;
use nika_session::intelligence::{DataLocus, IntelligenceKind};
use nika_session::{AuthoringSeat, SessionRuntime};

/// The intelligence the session reasons with, in words (the model when one
/// is named, and a choice this machine cannot serve now says so), then its
/// verifier; `None` while none was chosen.
pub(super) fn seat(runtime: &SessionRuntime) -> Option<String> {
    if !runtime.intelligence_chosen() {
        return None;
    }
    let checks = verifier(
        runtime.authoring_seat(),
        runtime.authoring_context().decision(),
    )
    .map_or_else(String::new, |words| format!("; {words}"));
    let chosen = &runtime.intelligence;
    let base = match (&chosen.kind, &chosen.locus) {
        (IntelligenceKind::None, _) => {
            return Some(format!("none, the engine facts answer{checks}"));
        }
        (IntelligenceKind::Harness { seat, transport }, _) => {
            format!("{seat} {transport}, through your account")
        }
        (IntelligenceKind::Api { provider }, DataLocus::Gateway { host, .. }) => {
            format!("{provider} API through {host}, metered")
        }
        (IntelligenceKind::Api { provider }, _) => format!("{provider} API, metered"),
        (IntelligenceKind::Local { provider }, _) => format!("{provider}, on this machine"),
        _ => "an intelligence this view cannot name".to_owned(),
    };
    // These are configured or resolved preparation facts, not a served-model receipt.
    let selected_model = chosen
        .model
        .as_deref()
        .or_else(|| match runtime.authoring_seat() {
            AuthoringSeat::Provider { model } => Some(model.as_str()),
            _ => None,
        });
    let model = selected_model.map_or_else(
        || "model chosen by provider - ".to_owned(),
        |model| format!("{model} - "),
    );
    let ready = if chosen.ready { "" } else { ", not ready here" };
    Some(format!("{model}{base}{ready}{checks}"))
}

/// Who judges a prepared candidate. The compiler judges through the operator's decision seat
/// when one is selected — consulted only for a finite choice, so it is named as selected, never
/// as having answered — and through the authoring model itself otherwise. A deterministic or
/// unavailable author calls no judge: a selected seat is said to sit unused, and nothing else.
fn verifier(author: &AuthoringSeat, selected: Option<&DecisionSetup>) -> Option<String> {
    let authors = matches!(
        author,
        AuthoringSeat::Provider { .. } | AuthoringSeat::Harness { .. }
    );
    match selected {
        Some(setup) if setup.refusal().is_some() => {
            Some(format!("verifier: {} refused, see /status", setup.model()))
        }
        Some(setup) if authors => Some(format!("verifier: {} (selected)", setup.model())),
        Some(setup) => Some(format!(
            "verifier: {} selected, unused without an AI author",
            setup.model()
        )),
        None if authors => Some("verifier: same model".to_owned()),
        None => None,
    }
}

#[cfg(test)]
#[cfg(unix)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use nika_session::authoring::AuthoringContext;
    use nika_session::intelligence::{IntelligenceCensus, UserIntelligencePreference};
    use nika_session::{ReasonError, Reply, SessionReasoner};

    const JEV: &str = "typesafe/jev-1.13.0";

    /// A ready decision seat on a loopback peer that is never called.
    fn ready() -> DecisionSetup {
        DecisionSetup::with_key(JEV, Some("fixture".into()), Some("http://127.0.0.1:1"))
    }

    /// A reasoner that authors with `0`; projecting a selection must never ask it.
    struct Authors(Option<String>);

    impl SessionReasoner for Authors {
        fn name(&self) -> String {
            "fixture connection".to_owned()
        }

        fn reason(&mut self, _: &str) -> Result<Reply, ReasonError> {
            panic!("projecting a selection must not ask a model")
        }

        fn authoring_model(&self) -> Option<String> {
            self.0.clone()
        }
    }

    #[test]
    fn the_verifier_names_the_judge_the_compiler_would_use() {
        let provider = AuthoringSeat::Provider {
            model: "deepseek/deepseek-chat".into(),
        };
        let deterministic = AuthoringSeat::Deterministic { why: None };
        let unavailable = AuthoringSeat::Unavailable { why: "no".into() };
        let unused = format!("verifier: {JEV} selected, unused without an AI author");
        for (author, selected, expected) in [
            (&provider, None, Some("verifier: same model".to_owned())),
            (&deterministic, None, None),
            (&unavailable, None, None),
            (
                &provider,
                Some(ready()),
                Some(format!("verifier: {JEV} (selected)")),
            ),
            (&deterministic, Some(ready()), Some(unused.clone())),
            (&unavailable, Some(ready()), Some(unused)),
            (
                &provider,
                Some(DecisionSetup::with_key(JEV, None, None)),
                Some(format!("verifier: {JEV} refused, see /status")),
            ),
        ] {
            let words = verifier(author, selected.as_ref());
            assert_eq!(words, expected, "{author:?}");
            assert!(words.is_none_or(|w| w.is_ascii() && !w.contains("fixture")));
        }
    }

    #[test]
    fn the_preparation_line_carries_the_sessions_own_verifier() {
        let root = std::env::temp_dir().join(format!(
            "nika-tui-selection-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&root).expect("room");
        let mut census = IntelligenceCensus::empty();
        census.api_keys.push("deepseek".into());
        let open = |kind: IntelligenceKind, authors: Option<&str>| {
            let authors = authors.map(str::to_owned);
            SessionRuntime::open_with(
                &root,
                census.clone(),
                &UserIntelligencePreference::new(kind, None),
                None,
                Box::new(move |_| Box::new(Authors(authors.clone()))),
            )
        };
        let api = || IntelligenceKind::Api {
            provider: "deepseek".into(),
        };
        let line = |runtime: &mut SessionRuntime, decision: Option<DecisionSetup>| {
            runtime.set_authoring_context(AuthoringContext::default().with_decision(decision));
            seat(runtime).expect("chosen")
        };
        let mut none = open(IntelligenceKind::None, None);
        assert_eq!(line(&mut none, None), "none, the engine facts answer");
        assert_eq!(
            line(&mut none, Some(ready())),
            format!(
                "none, the engine facts answer; verifier: {JEV} selected, unused without an AI author"
            )
        );
        let mut author = open(api(), Some("deepseek/deepseek-chat"));
        assert_eq!(
            line(&mut author, None),
            "deepseek/deepseek-chat - deepseek API, metered; verifier: same model"
        );
        assert_eq!(
            line(&mut author, Some(ready())),
            format!("deepseek/deepseek-chat - deepseek API, metered; verifier: {JEV} (selected)")
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
