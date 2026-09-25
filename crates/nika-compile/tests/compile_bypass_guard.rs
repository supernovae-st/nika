// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A bypass phrase inside a prohibition is a gate, not an approval bypass (sealed-v3
//! treatment lane, 2026-09-22: sv3-03 « rien ne doit partir sans mon accord »).
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile};

mod common;

/// sv3-03's second sentence: « rien ne doit partir sans mon accord » forbids the send
/// until the approval; it does not skip one.
#[test]
fn a_bypass_phrase_inside_a_prohibition_is_a_gate_not_a_bypass() {
    let gate = "Lis ./station/remontees.csv, compte les remontées dont l'état est « fermée » et écris ce nombre seul dans ./out/fermees.txt. Demandez-moi confirmation avant tout envoi ; rien ne doit partir sans mon accord.";
    let out = compile(&CompileRequest::create(gate)).unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("approval-bypass wording")),
        "{out:#?}"
    );
    let bypass = "Consulte le client, prépare une réponse et crédite le compte sans mon accord.";
    let out = compile(&CompileRequest::create(bypass)).unwrap();
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("approval-bypass wording")),
        "{out:#?}"
    );
}

/// The reality check's T6 (2026-09-22): an English request ending with « ask me before
/// anything is sent » states a gate; the bare word « yesterday » in « yesterday's support
/// tickets » is not an approval reused from yesterday. « approved yesterday » still is.
#[test]
fn a_time_word_beside_no_approval_word_is_not_a_bypass() {
    let gated = "Every morning, read yesterday's support tickets in ./tickets.json, group the open ones by topic, draft a short brief, and ask me before anything is sent.";
    let out = compile(&CompileRequest::create(gated)).unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("approval-bypass wording")),
        "{out:#?}"
    );
    let bypass = "Read ./tickets.json and send the brief to the team; it was approved yesterday.";
    let out = compile(&CompileRequest::create(bypass)).unwrap();
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("approval-bypass wording")),
        "{out:#?}"
    );
}
