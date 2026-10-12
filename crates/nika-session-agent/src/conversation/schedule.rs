// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The schedule a proposal carries beside its candidate (`propose`'s `schedule`): the person's
//! words for when the work repeats, cited, and the zone those words follow. A workflow never
//! holds its schedule, and saving declares nothing.

use nika_session_change::work::AskedQuestion;
use serde_json::Value;

use super::answer::recommended;
use super::{Citations, Conversation, in_line, typed};

impl Conversation {
    /// `propose`'s `schedule`, as words and a zone: the person's words that say when the work
    /// repeats, from a line of theirs, and the IANA zone they follow, which their own words name
    /// or a choice the proposal shows (`open`) offers as its default. A zone they would not see
    /// is never assumed. `None` when the proposal carries no schedule.
    ///
    /// # Errors
    ///
    /// Why the schedule cannot be shown: words the cited line does not hold, or a zone the person
    /// would not see.
    pub fn schedule(
        &self,
        citations: &Citations,
        args: &Value,
        open: &[AskedQuestion],
    ) -> Result<Option<(String, String)>, String> {
        let schedule = &args["schedule"];
        if schedule.is_null() {
            return Ok(None);
        }
        let message = schedule["message"].as_str().unwrap_or_default();
        let excerpt = schedule["excerpt"].as_str().unwrap_or_default();
        let cited = (citations.get(message)).is_some_and(|line| in_line(line, excerpt, None));
        if excerpt.trim().is_empty() || !cited {
            return Err(format!(
                "the schedule cites `{message}` for « {excerpt} »: cite the person's line and their own words that say when it runs"
            ));
        }
        let zone = schedule["timezone"].as_str().unwrap_or_default().trim();
        let stated = typed(&citations.stated(self.since), zone);
        let offered = (open.iter().filter_map(recommended))
            .flat_map(|offer| &offer.values)
            .any(|value| value.value == zone);
        if zone.is_empty() || !(stated || offered) {
            return Err(format!(
                "the zone `{zone}` is neither in the person's words nor the default of a choice this proposal shows: show it in `open` as a choice whose recommended option is their zone, with the others beside it"
            ));
        }
        Ok(Some((excerpt.to_owned(), zone.to_owned())))
    }
}
