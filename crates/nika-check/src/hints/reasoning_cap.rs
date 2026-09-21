// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `reasoning-cap` hint's fold proof — the analyzer's judge
//! (`nika_check_analyzer::reasoning_cap_hints`) reaches `report.hints`
//! through `hints::scan_hints` (the zero-consumer law: a judge nobody
//! folds is invisible). Its own laws and near-misses are pinned beside
//! the judge, in the analyzer's `thinking` module.

#[cfg(test)]
mod tests {
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn hints_of(yaml: &str) -> Vec<crate::Hint> {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let report = crate::check(&wf);
        assert!(report.is_clean(), "a hint is never a refusal: {report:?}");
        report.hints
    }

    /// The product matrix's `openai/gpt-5-mini` × `max_tokens: 1200`
    /// rides `hints[]` as `reasoning-cap`, the file stays clean, `nika
    /// explain` has its row, and the compiler's own 4096 carries none.
    #[test]
    fn a_tight_cap_on_a_reasoning_seat_rides_hints_and_stays_clean() {
        let tight = hints_of(
            "nika: w\nmodel: openai/gpt-5-mini\npermits: {}\ntasks:\n  draft:\n    infer:\n      \
             prompt: hi\n      max_tokens: 1200\n",
        );
        let hint = tight
            .iter()
            .find(|h| h.kind == "reasoning-cap")
            .expect("the reasoning-cap hint rides the report");
        assert_eq!(hint.task, "draft");
        assert!(
            hint.advice.contains("openai/gpt-5-mini") && hint.advice.contains("4096"),
            "{}",
            hint.advice
        );
        assert!(
            crate::hint_help("reasoning-cap").is_some(),
            "`nika explain reasoning-cap` has a row"
        );

        let comfortable = hints_of(
            "nika: w\nmodel: openai/gpt-5-mini\npermits: {}\ntasks:\n  draft:\n    infer:\n      \
             prompt: hi\n      max_tokens: 4096\n",
        );
        assert!(
            !comfortable.iter().any(|h| h.kind == "reasoning-cap"),
            "{comfortable:?}"
        );
    }
}
