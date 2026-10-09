// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source compatibility for a consumer outside the native renderer crate.

use nika_tui::model::Waiting;

#[test]
fn legacy_question_construction_and_prompt_remain_source_compatible() {
    let waiting = Waiting::Question {
        key: "legacy".to_owned(),
    };
    assert_eq!(waiting.prompt(), "reply › ");
    assert_eq!(waiting.hint(), "answer the question above · cancel to stop");
}
