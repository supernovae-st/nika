// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source compatibility of `nika_session::{authoring, intelligence, reasoner, turn}` and of the
//! session's root re-exports after the four modules moved to the size-cap member below the
//! session, `nika_session_intelligence` (2026-10-09 · ADR-150). This file is an external
//! consumer: it compiles against the session paths, and they name the very same items — the
//! same types (a value of one path IS a value of the other), the same traits, the same
//! functions (their signatures carry the member's types) and the same constants — never
//! copies. The one observable difference is a type's name at run time, which now names the
//! member. Nothing here reads the environment, a home directory or the network.

use std::any::type_name;

use nika_onboard::compile::{CompileOutcome, CompileRequest};
use nika_session::authoring::{AuthoringRound, AuthoringSeat};
use nika_session::intelligence::{DataLocus, IntelligenceKind};
use nika_session::turn::{RoutingMethod, SessionPhase, TurnAct, TurnDecision};
use nika_session_intelligence as member;

/// Functions of the session paths, typed with the member's types: a session-side copy of them
/// could not be assigned here.
const ROUTING_PROMPT: fn(&member::turn::TurnContext, &str) -> String =
    nika_session::turn::routing_prompt;
const COMPILE: fn(&CompileRequest) -> Result<CompileOutcome, member::authoring::AuthoringError> =
    nika_session::authoring::compile_deterministic;

#[test]
fn the_session_paths_name_the_members_items() {
    // One round under the module path, the root path and the member's path.
    let at_root: nika_session::AuthoringRound = AuthoringRound::new("copy a.txt to b.txt");
    let owned: member::authoring::AuthoringRound = at_root.clone();
    assert_eq!(at_root, owned);
    assert_eq!(owned.effective_intent(), "copy a.txt to b.txt");
    // A seat, a choice, an act: the member's values under every path.
    let seat: member::authoring::AuthoringSeat = AuthoringSeat::Deterministic { why: None };
    assert!(!seat.has_model());
    let kind: member::intelligence::IntelligenceKind = IntelligenceKind::None;
    let resolved =
        nika_session::ResolvedSessionIntelligence::new(kind, None, DataLocus::None, true, None);
    assert_eq!(resolved.kind, member::intelligence::IntelligenceKind::None);
    let act: member::turn::TurnAct = TurnAct::parse("DISCUSS");
    assert_eq!(act, TurnAct::Discuss);
    let decision = TurnDecision::new(act, RoutingMethod::Model);
    assert_eq!(decision.method, member::turn::RoutingMethod::Model);
    // The function answers as the member's, under every path.
    let context = member::turn::TurnContext::new(SessionPhase::Idle, None, None);
    let prompt = ROUTING_PROMPT(&context, "copy it");
    assert_eq!(prompt, member::turn::routing_prompt(&context, "copy it"));
    assert!(prompt.contains("«copy it»"), "{prompt}");
    // The deterministic compile answers as the member's (zero provider calls, always).
    let request = owned.request();
    let status = |out: Result<CompileOutcome, member::authoring::AuthoringError>| {
        out.ok().map(|out| out.status)
    };
    let compiled = status(COMPILE(&request));
    assert!(compiled.is_some(), "the deterministic compile answers");
    assert_eq!(
        compiled,
        status(member::authoring::compile_deterministic(&request))
    );
    // A trait object of one path is one of the other.
    let mut reasoner: Box<dyn member::reasoner::SessionReasoner> =
        Box::new(nika_session::ScriptedReasoner::new(vec!["one".to_owned()]));
    let reply: Option<nika_session::Reply> = reasoner.reason("p").ok();
    assert_eq!(
        reply,
        Some(member::reasoner::Reply::new("one".to_owned(), false))
    );
    assert_eq!(
        nika_session::authoring::DECISION_SCHEMA,
        member::authoring::DECISION_SCHEMA
    );
}

/// What changed for a consumer that looks at metadata: a type's run-time name names the
/// member (the same name under every path, and no longer the session's).
#[test]
fn only_the_run_time_type_name_names_the_member() {
    for (session, owner) in [
        (
            type_name::<nika_session::AuthoringRound>(),
            type_name::<member::authoring::AuthoringRound>(),
        ),
        (
            type_name::<nika_session::IntelligenceCensus>(),
            type_name::<member::intelligence::IntelligenceCensus>(),
        ),
        (
            type_name::<nika_session::ReasonError>(),
            type_name::<member::reasoner::ReasonError>(),
        ),
        (
            type_name::<nika_session::turn::TurnAct>(),
            type_name::<member::turn::TurnAct>(),
        ),
    ] {
        assert_eq!(session, owner);
        assert!(owner.starts_with("nika_session_intelligence::"), "{owner}");
    }
}
