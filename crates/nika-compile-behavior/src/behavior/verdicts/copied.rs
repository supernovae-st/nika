// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The copy relation: a text file copied as is holds exactly the text the run consumed from its
//! source, byte for byte. Nothing is normalized: a line ending, a space, a final newline or a
//! literal that looks like a template is the source's own. The expected text is the run's own
//! receipt of its source, whole: no receipt is no observation, and a cut or a sample (how a host
//! records a source that is no text) proves nothing. A result read back cut (how a host records a
//! result that is no text) is not judged. Whether the run published the result is the presence's
//! question, settled before the content is read: a file the run did not write never passes, even
//! when it already holds the right text.

use super::{Coverage, Identity, Outcome, ReadBack, Run, Settle, Shown, identity, same_path};

/// The interpretations a judgment of a copy applies, for the preview.
pub(super) const ASSUMPTIONS: [&str; 2] = [
    "a copy holds exactly the text the run consumed from its source, byte for byte: no line \
     ending, space or character is normalized, and a literal is never interpreted",
    "the judgment carries text only: a source or a result cut, sampled or not text is never \
     certified",
];

/// The text the run consumed from `source`, whole: the exact text its copy must hold.
pub(super) fn source_text(
    source: &str,
    run: &Run,
    evidence: &mut Vec<Identity>,
) -> Result<String, Settle> {
    let Some(consumed) = run
        .consumed
        .iter()
        .find(|read| same_path(&read.path, source))
    else {
        return Err(Settle::Invalid(format!(
            "the host supplied no evidence of what the run consumed from {source}"
        )));
    };
    evidence.push(identity(
        &consumed.path,
        &consumed.text,
        consumed.coverage,
        None,
    ));
    if consumed.coverage != Coverage::Complete {
        return Err(Settle::Open(format!(
            "the evidence of {source} is {}: a copy is judged on exactly the text the run \
             consumed",
            consumed.coverage.word()
        )));
    }
    Ok(consumed.text.clone())
}

/// What the copy written at `path` shows against the text it must hold.
pub(super) fn shown(
    path: &str,
    expected: Option<&Result<String, Settle>>,
    output: &ReadBack,
) -> Shown {
    match expected {
        Some(Ok(_)) if output.truncated => (
            Outcome::Incomplete,
            format!("{path} was read back cut or as no text: the copy is not verified"),
        ),
        Some(Ok(text)) if output.text == *text => (
            Outcome::Passed,
            format!("{path} holds exactly the text of its source"),
        ),
        Some(Ok(text)) => (
            Outcome::Failed,
            format!(
                "{path} is no exact copy of its source: {}",
                difference(text, &output.text)
            ),
        ),
        Some(Err(Settle::Open(why))) => (
            Outcome::Incomplete,
            format!("the copy is not verified: {why}"),
        ),
        Some(Err(Settle::Invalid(why))) => (Outcome::InvalidHarness, why.clone()),
        Some(Err(Settle::Stop(_))) | None => (
            Outcome::Incomplete,
            "the source of the copy was not read".to_owned(),
        ),
    }
}

/// Where a written copy first departs from its source, in bytes.
fn difference(expected: &str, written: &str) -> String {
    let same = expected
        .bytes()
        .zip(written.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    format!(
        "it holds {} bytes and the source {}; they first differ at byte {same}",
        written.len(),
        expected.len()
    )
}
