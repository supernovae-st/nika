// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What the person may say of a result after the conversation's candidate ran (NIK-174). A run of
//! the current candidate that succeeded, started its very bytes as one execution and completed
//! writing at least one artifact the project holds earns one optional offer at the free prompt,
//! never a question that takes the next line: « What would you like to adjust in this result? »,
//! with dimensions read from the candidate's own tasks, « Nothing to change » and « Later », those
//! two standing alone. Picked dimensions reach the author, which asks only what is still missing
//! and proposes an amended revision; a later revision of that same request, run successfully,
//! then offers one neutral opinion with no default, bound to the adjustment it follows. Another
//! request's result never inherits that adjustment.
//!
//! The offer is answered only by its identity, and only while nothing else waits (a host lists
//! it there too); a line typed at the prompt, a run's review or the next observed run closes it
//! unanswered. A rerun of bytes already offered offers nothing again, even after a reopen: the
//! history keeps the bytes offered and the records, never a live offer. Each record is bound to
//! its scope (request, revision, started bytes, execution, artifacts with their witnesses) and
//! authorizes nothing: Save, Run, sending and activation stay the person's own acts.

use std::collections::BTreeSet;
use std::path::Path;

use nika_event::source_id::sha256_hex;
use nika_fs::OwnedDir;
use nika_schema::raw::{RawAction, RawInvokeTarget};
use nika_schema::{FileId, ParseMode};
use nika_trace::run_view::RunFacts;
use serde::{Deserialize, Serialize};

use super::{Lang, SessionRuntime, TurnOutcome};
use crate::change::Witness;
use crate::outcome::{QuestionId, Refusal, RefusalClass};
use crate::work::{
    AskedQuestion, Feedback as Projected, FeedbackArtifact, FeedbackKind, FeedbackOffer,
    FeedbackRecord, FeedbackScope, Offer,
};

/// Words the person reads, in English then in French ([`Lang::pick`]).
type Said = (&'static str, &'static str);
/// The adjustment offer's question.
const ADJUST: Said = (
    "What would you like to adjust in this result? Optional: your own words work too.",
    "Que voulez-vous ajuster dans ce résultat ? Facultatif : vos propres mots conviennent aussi.",
);
/// The opinion offer's question.
const OPINION: Said = (
    "How is this corrected result for your use? Optional: it changes nothing by itself.",
    "Que vaut ce résultat corrigé pour votre usage ? Facultatif : cela ne change rien en soi.",
);
/// Why an answer that names no offered choice was refused.
const NOT_OFFERED: Said = (
    "this answer names no offered choice · nothing was recorded · the offer waits · words typed \
     at the prompt reach the conversation instead",
    "cette réponse ne nomme aucun choix proposé · rien n'a été enregistré · l'offre attend · les \
     mots tapés à l'invite vont à la conversation",
);
/// Why a set mixing a standing-alone choice with others was refused.
const ALONE: Said = (
    "« Nothing to change » and « Later » stand alone · nothing was recorded · the offer waits",
    "« Rien à changer » et « Plus tard » se choisissent seuls · rien n'a été enregistré · l'offre \
     attend",
);
/// Why an answer to an offer that no longer waits was refused.
const GONE: Said = (
    "this offer no longer waits · nothing was recorded",
    "cette offre n'attend plus · rien n'a été enregistré",
);
/// The keys that stand alone in an offer.
const STANDING: [&str; 2] = ["nothing", "later"];

/// The adjustment the person picked, until a later revision of its request ran: that request,
/// the revision it was picked on, and the identity of its answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Lineage {
    request: u64,
    revision: u64,
    answer: String,
}

/// What the history keeps: the records, the started bytes already offered (answered or not) and
/// the adjustment still awaiting its amended run. Evidence, never a live offer or an authority.
#[derive(Default, Serialize, Deserialize)]
struct Kept {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    recorded: Vec<FeedbackRecord>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    offered: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    adjusted: Option<Lineage>,
}

/// The offer waiting, the records, and what this conversation already offered.
#[derive(Default)]
pub(super) struct Feedback {
    offer: Option<FeedbackOffer>,
    recorded: Vec<FeedbackRecord>,
    /// The started bytes an offer was made for: one offer each, whatever reruns them.
    offered: BTreeSet<String>,
    /// The adjustment picked last, until a later revision of its request ran.
    adjusted: Option<Lineage>,
}

impl Feedback {
    /// The offer closes unanswered: the person acted otherwise.
    pub(super) fn close(&mut self) {
        self.offer = None;
    }

    /// Whether `id` names the offer waiting.
    pub(super) fn asks(&self, id: &QuestionId) -> bool {
        (self.offer.as_ref()).is_some_and(|offer| offer.question.id == *id)
    }

    /// The offer and the records, as the work snapshot carries them.
    pub(super) fn projected(&self) -> Projected {
        Projected::new(self.offer.clone(), self.recorded.clone())
    }

    /// What the conversation's history keeps: `None` while nothing was offered.
    pub(super) fn kept(&self) -> Option<serde_json::Value> {
        if self.recorded.is_empty() && self.offered.is_empty() && self.adjusted.is_none() {
            return None;
        }
        let kept = Kept {
            recorded: self.recorded.clone(),
            offered: self.offered.clone(),
            adjusted: self.adjusted.clone(),
        };
        serde_json::to_value(kept).ok()
    }

    /// What a reopened conversation kept: the records as evidence, the bytes already offered and
    /// the adjustment still awaiting its amended run; never a live offer. An unreadable value
    /// keeps nothing.
    pub(super) fn restored(kept: Option<&serde_json::Value>) -> Self {
        let kept: Kept = (kept.cloned())
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        let mut offered = kept.offered;
        offered.extend((kept.recorded.iter()).map(|record| record.scope.source_sha256.clone()));
        Self {
            offer: None,
            recorded: kept.recorded,
            offered,
            adjusted: kept.adjusted,
        }
    }
}

/// Whether `picked` is one standing-alone choice: nothing to adjust now.
fn alone(picked: &[String]) -> bool {
    picked.iter().any(|key| STANDING.contains(&key.as_str()))
}

impl SessionRuntime {
    /// After a run the session observed (`facts`): any offer about an earlier result closes, then
    /// a successful run of the conversation's current candidate earns its offer, once for its
    /// bytes: the opinion when it is a later revision of the request whose adjustment the person
    /// picked, the adjustment otherwise. The offer is the work snapshot's, which a host shows as
    /// its own question; the run's words never repeat it. Nothing for any other run, or once
    /// those bytes were offered.
    pub(super) fn feedback_after(&mut self, facts: &RunFacts) {
        self.feedback.close();
        let Some((scope, source)) = self.feedback_scope(facts) else {
            return;
        };
        if self.feedback.offered.contains(&scope.source_sha256) {
            return;
        }
        let follows = match &self.feedback.adjusted {
            // Another request's result: the adjustment picked never reached it.
            Some(lineage) if lineage.request != scope.request => {
                self.feedback.adjusted = None;
                None
            }
            Some(lineage) if scope.revision > lineage.revision => Some(lineage.answer.clone()),
            // The revision the adjustment was picked on: nothing amended it yet.
            Some(_) => return,
            None => None,
        };
        let lang = self.language;
        let (kind, (en, fr), options, multiple) = match follows {
            None => (
                FeedbackKind::Adjust,
                ADJUST,
                dimensions(&source, lang),
                true,
            ),
            Some(_) => (FeedbackKind::Opinion, OPINION, opinions(lang), false),
        };
        let text = lang.pick(en, fr);
        let witness = format!(
            "nika-session feedback 2\n{}\n{scope:?}\n{follows:?}",
            kind.key()
        );
        let id = QuestionId::new(Witness::of(witness.as_bytes()).0, &self.questions.asker());
        let question =
            AskedQuestion::new(id, kind.key(), text).with_options(options, true, multiple);
        self.feedback.offered.insert(scope.source_sha256.clone());
        if follows.is_some() {
            self.feedback.adjusted = None;
        }
        self.feedback.offer = Some(FeedbackOffer::new(kind, question, scope, follows));
    }

    /// The result `facts` record, when it is a success of the conversation's current candidate:
    /// its terminal frame says succeeded, its one start names the candidate's own bytes, one
    /// execution ran it, and every artifact it completed writing is a regular file the project
    /// holds under its root, reached through no link and witnessed now. With it, the candidate's
    /// bytes.
    fn feedback_scope(&self, facts: &RunFacts) -> Option<(FeedbackScope, String)> {
        if !self.agent_leads() || facts.terminal() != Some("succeeded") {
            return None;
        }
        let [Some(started)] = facts.starts() else {
            return None;
        };
        let [execution] = facts.executions() else {
            return None;
        };
        let (request, revision, source) = self.agent.as_ref()?.current_candidate()?;
        if sha256_hex(source.as_bytes()) != *started {
            return None;
        }
        let writes = facts.completed_writes();
        if writes.is_empty() {
            return None;
        }
        let artifacts = (writes.iter())
            .map(|path| self.artifact(path))
            .collect::<Option<Vec<_>>>()?;
        let execution = execution.uuid.to_string();
        let scope = FeedbackScope::new((request, revision), started.clone(), execution, artifacts);
        Some((scope, source))
    }

    /// The artifact a run completed writing at `path`, as the project holds it now: a regular file
    /// under the project root, each component opened below the root's descriptor and never through
    /// a link, its bytes read whole from that same descriptor within the bound a proposal's basis
    /// reads a project file by ([`super::fresh::FILE_BOUND`]). `None` otherwise, and past that
    /// bound, where no witness would cover the whole content. A reread now, never the bytes the
    /// run wrote, which the run's own evidence keeps.
    fn artifact(&self, path: &str) -> Option<FeedbackArtifact> {
        let relative = Path::new(path.trim_start_matches("./"));
        let root = OwnedDir::open(&self.snapshot.root).ok()?;
        let mut file = root.open_relative(relative).ok()?;
        let read = nika_fs::read_capped(&mut file, super::fresh::FILE_BOUND).ok()?;
        (!read.over).then(|| FeedbackArtifact::new(path, Witness::of(&read.bytes).0))
    }

    /// The answer `line` to the offer `id` names, once its caller found nothing else waiting:
    /// exactly its keys (a JSON list where several may be checked, one key otherwise), « Nothing
    /// to change » and « Later » alone, or refused with the offer kept. Recorded with the offer's
    /// scope; picked dimensions then reach the author, which asks only what is still missing and
    /// proposes the amended revision. Nothing is saved, run, sent or activated from it.
    pub(super) fn feedback_answer(&mut self, id: &QuestionId, line: &str) -> TurnOutcome {
        let waiting = (self.feedback.offer.as_ref()).filter(|offer| offer.question.id == *id);
        let picked = match waiting.map(|offer| picked(&offer.question, line)) {
            None => return self.feedback_refused(id, RefusalClass::StaleRevision, GONE),
            Some(Err(why)) => return self.feedback_refused(id, RefusalClass::NotAllowed, why),
            Some(Ok(picked)) => picked,
        };
        let Some(offer) = self.feedback.offer.take() else {
            return self.feedback_refused(id, RefusalClass::StaleRevision, GONE);
        };
        let lang = self.language;
        let named = (offer.question.options.iter())
            .filter(|option| picked.contains(&option.key))
            .map(|option| option.label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let standing = alone(&picked);
        self.feedback
            .recorded
            .push(FeedbackRecord::new(&offer, picked));
        if offer.kind != FeedbackKind::Adjust || standing {
            let noted = lang.pick(
                "Noted: {named} · it changes nothing by itself",
                "Noté : {named} · cela ne change rien en soi",
            );
            return TurnOutcome::Facts(noted.replace("{named}", &named));
        }
        self.feedback.adjusted = Some(Lineage {
            request: offer.scope.request,
            revision: offer.scope.revision,
            answer: id.as_str().to_owned(),
        });
        let artifacts = (offer.scope.artifacts.iter())
            .map(|artifact| artifact.path.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let note = format!(
            "The person read the result of the last run ({artifacts}) and chose to adjust: \
             {named}. Ask only the precision still missing for each (none when their words \
             already say it), then amend the candidate keeping every other obligation of the \
             request, and propose the revision. Never save or run: the person decides."
        );
        (self.agent_note(&note)).unwrap_or_else(|| {
            let noted = lang.pick(
                "Noted: {named} · say what should change",
                "Noté : {named} · dites ce qui doit changer",
            );
            TurnOutcome::Facts(noted.replace("{named}", &named))
        })
    }

    /// A refusal of an answer to the offer `id`, recorded as that answer's act.
    fn feedback_refused(
        &mut self,
        id: &QuestionId,
        class: RefusalClass,
        (en, fr): Said,
    ) -> TurnOutcome {
        let refusal = TurnOutcome::Refusal(Refusal::new(class, self.language.pick(en, fr)));
        self.answer_refused(Some(id.clone()), refusal)
    }
}

/// The dimensions a result of `source` may be adjusted on, read from its own tasks: the level of
/// detail and the length of what a model writes, and the topics and the diversity of the sources
/// it writes from when it reads or fetches them; the presentation of what it writes; then
/// « Nothing to change » and « Later », standing alone, each said in `lang`. A result no model
/// writes is computed: it is offered its presentation only (live gauntlet wave 7, 2026-10-19:
/// « Topics covered · Diversity of sources » after a bank-statement total, a CSV cleaning and a
/// site check). Bytes the parser refuses offer only the presentation.
pub(super) fn dimensions(source: &str, lang: Lang) -> Vec<Offer> {
    let tasks = (nika_schema::parse(source, FileId::new(0), ParseMode::Strict))
        .map(|workflow| workflow.tasks)
        .unwrap_or_default();
    let (mut model, mut reads) = (false, false);
    for task in &tasks {
        match &task.value.action {
            RawAction::Infer(_) | RawAction::Agent(_) => model = true,
            RawAction::Invoke(invoke) => {
                if let RawInvokeTarget::Tool(tool) = &invoke.target {
                    reads |= matches!(tool.value.as_str(), "nika:fetch" | "nika:read");
                }
            }
            _ => {}
        }
    }
    let offer = |key: &str, (en, fr): Said| Offer::new(key, lang.pick(en, fr), false);
    let mut offers = Vec::new();
    if model {
        offers.push(offer("depth", ("Level of detail", "Niveau de détail")));
        offers.push(offer("length", ("Length", "Longueur")));
    }
    if model && reads {
        offers.push(offer("topics", ("Topics covered", "Sujets couverts")));
        offers.push(offer(
            "sources",
            ("Diversity of sources", "Diversité des sources"),
        ));
    }
    offers.push(offer(
        "presentation",
        ("Presentation of the result", "Présentation du résultat"),
    ));
    offers.push(offer("nothing", ("Nothing to change", "Rien à changer")).exclusive());
    offers.push(offer("later", ("Later", "Plus tard")).exclusive());
    offers
}

/// The neutral opinion's choices, said in `lang`: none recommended, « Later » standing alone.
pub(super) fn opinions(lang: Lang) -> Vec<Offer> {
    let offer = |key: &str, (en, fr): Said| Offer::new(key, lang.pick(en, fr), false);
    vec![
        offer("suitable", ("Suitable for this use", "Adapté à cet usage")),
        offer(
            "changes",
            ("Still needs changes", "Demande encore des changements"),
        ),
        offer("wrong", ("Wrong result", "Résultat faux")),
        offer("unevaluated", ("Not evaluated yet", "Pas encore évalué")),
        offer("later", ("Later", "Plus tard")).exclusive(),
    ]
}

/// The keys `line` picks among `question`'s options, in their order: exactly offered keys, each
/// once (a JSON list where several may be checked, or one key), a standing-alone one alone. The
/// host's native control sends exactly these; words typed at the prompt never reach here.
pub(super) fn picked(question: &AskedQuestion, line: &str) -> Result<Vec<String>, Said> {
    let line = line.trim();
    let keys: Vec<String> = if line.starts_with('[') {
        serde_json::from_str(line).map_err(|_| NOT_OFFERED)?
    } else {
        vec![line.to_owned()]
    };
    let offered = |key: &String| question.options.iter().any(|option| option.key == *key);
    let unique = (keys.iter().enumerate()).all(|(at, key)| !keys[..at].contains(key));
    if keys.is_empty()
        || !unique
        || !keys.iter().all(offered)
        || (keys.len() > 1 && !question.multi_select)
    {
        return Err(NOT_OFFERED);
    }
    let standing = |key: &String| {
        (question.options.iter()).any(|option| option.key == *key && option.exclusive)
    };
    if keys.len() > 1 && keys.iter().any(standing) {
        return Err(ALONE);
    }
    Ok((question.options.iter())
        .filter(|option| keys.contains(&option.key))
        .map(|option| option.key.clone())
        .collect())
}
