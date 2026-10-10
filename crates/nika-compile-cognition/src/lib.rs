// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The seats' doors of the Compile core (ADR-140): a model proposes a private semantic plan
//! (the COLD door: decoded, merged, composed, assembled), writes the `.nika` itself (the native
//! door: parsed, checked, judged by the fidelity laws, repaired, replayed), or sketches its
//! structure first and fills typed holes (the sketch door); the verified transform, the
//! knowledge door (the Foundry snapshot recalled per intent) and the bounded decision seats.
//! Above the deterministic compiler, never below it: this crate depends on `nika-compile` and
//! reads its stated [`nika_compile::surface`]; `nika-compile` never depends back. ONE
//! architectural unit in several workspace members (D-2026-07-09-N1 · ADR-137 · ADR-138 ·
//! ADR-140); the public surface of the unit is re-exported by `nika-onboard` at the paths every
//! caller reads.
//!
//! The moved code keeps its paths: the reader's modules and the core's surface are bound at
//! this root under the names the doors have always used (`crate::plan`, `crate::edit`, …).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use nika_compile_reader::{
    columns, gates, hot, lexicon, objects, paths, plan, rule_tokens, rules, shape, structure, text,
    unknowns,
};

// The proposal merge's word tables, at the path the merge has always read them (ADR-145).
use nika_compile_clauses::words;

use nika_compile_fidelity::{fidelity, sketch};

use nika_compile::surface::{
    admit_hot, finding, initial, lexical_rest_is_explicit, literal_answer, native_apply, parse,
    plan_record, question, record_ledger, record_retrieval, record_route, replay, replay_judged,
    unresolved,
};
use nika_compile::{
    AuthoringCognition, AuthoringPolicy, AuthoringReceipt, CompileDiagnostic, CompileError,
    CompileOutcome, CompileQuestion, CompileRequest, CompileStatus, DiagnosticKind, HotPolicy,
    NativeMode, QuestionType, Strategy, compile, intent_sha256, revise_intent,
};

/// The core's literal projection, at the path the doors read it.
mod edit {
    pub(crate) use nika_compile::surface::literal_projection;
}
/// The core's obligation ledger.
mod ledger {
    pub(crate) use nika_compile::surface::Ledger;
}
/// The core's assembler entry.
mod assemble {
    pub(crate) use nika_compile::surface::assemble::{assemble, unfed};
}
/// The core's bounded support clauses.
mod support {
    pub(crate) use nika_compile::surface::support::{assemble, resolve};
}
/// The core's retrieval index.
mod retrieve {
    pub(crate) use nika_compile::retrieve;
}
/// The core's request input shape.
mod types {
    pub(crate) use nika_compile::surface::{EditChange, Input};
}

#[cfg(test)]
mod laws {
    pub(crate) use nika_compile::surface::{LINES, SELECT_BY_FIELD};
}

/// The authority over a seat's requests (the bound a door states, the counters that enforce it,
/// the account a receipt keeps) is owned by the size-cap member below the seats' doors since
/// ADR-152: `nika_compile_seats::authority`. This path is kept and names the very same items.
#[doc(inline)]
pub use nika_compile_seats::authority;
mod cognition;
/// The bounded decision seats ([`decide`]) and the rehearsal port ([`rehearse`]), the two
/// capabilities a host lends a preparation, are owned by the size-cap member below the seats'
/// doors since 2026-10-07 (ADR-146): `nika_compile_seats::{decide, rehearse}`. These paths are
/// kept and name the very same items (types, traits, functions, constants).
#[doc(inline)]
pub use nika_compile_seats::{decide, rehearse};
mod predicate;

pub use cognition::{
    Cognition, NoProvider, compile_with_cognition, compile_with_cognition_composed,
    compile_with_cognition_rehearsed, compile_with_provider,
};

/// A host's scoped observation of its compile's authoring answers as received (slice C).
pub mod observe {
    pub use nika_providers::authoring::observe::{
        ActivitySink, Answered, AuthoringObservation, CallActivity, CallState, Failure, Sink,
        TextBlocks, observe_activity, observe_authoring,
    };
}
