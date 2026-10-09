// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A question's answer act, lent by Session, never inferred from its final outcome.
//! The value is taken independently of the later compile outcome.

use crate::model::{Beat, Committed, Kind};
use nika_session::{QuestionId, work};

/// This reports a taken value only; strong-id waiting state owns input retention.
/// The full strong identity (including incarnation) and semantic key scope this act.
pub(super) fn read(
    answered: Option<&work::Answered>,
    id: &QuestionId,
    key: &str,
    label: &str,
) -> Option<Beat> {
    let answered = answered.filter(|answer| answer.question == *id)?;
    match &answered.act {
        work::AnswerAct::Bound {
            key: taken,
            value,
            reading,
            ..
        } if taken == key => {
            let how = match reading {
                work::ValueSource::AsTyped => Some("as you typed it"),
                work::ValueSource::OfferedKey => Some("one of the offered answers"),
                work::ValueSource::ModelRead => {
                    Some("a verbatim part of your reply, chosen by one reading call")
                }
                work::ValueSource::SeatDefault => {
                    Some("the offered default from your selected connection")
                }
                _ => None,
            };
            let mut words = format!("Answer taken · {label}\n«{value}»");
            if let Some(how) = how {
                words.push('\n');
                words.push_str(how);
            }
            Some(Beat::Say(Committed::new(Kind::Notice, words)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
