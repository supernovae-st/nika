// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The candidate the conversation proposes, as the renderer's host adapter
//! folded it (its `session::candidate`) from its Session's borrowed
//! projection (`nika_session::runtime::Candidate`): the identity a consent
//! names, every change a yes would land (what it creates or replaces, where,
//! over which witnessed bytes), what the Session's audit says each audited
//! workflow reaches when it runs (under its own path when it lands several),
//! what a `save & run` of it would run when the Session's typed method admits
//! one, the words of the rehearsal proof bound to that identity, and the look
//! of the exact pending bytes of ONE workflow (their witness, the check
//! facade's verdict of them alone, their graph); the changes whose bytes are
//! not shown are counted, never passed off as shown. Every fact of one
//! [`Proposed`] belongs to one identity: the fields are private, and a new
//! identity is a new fold.
//!
//! The pending bytes are observed once, when the Session proposed them; they
//! are not a file and are never read again from disk. Nothing here reads a
//! file or a clock, and drawing calls nothing here: the shell renders a face
//! before the frame. Opening the candidate grants nothing: the `yes` typed
//! under `Save? ›` is the only consent, and it answers this identity or none.
//! While a consent can name it, the conversation's card reviews it
//! ([`Review`]).

use crate::Face;
use nika_display::theme::Role;
use nika_session::work::{DocumentRevision, ScheduleWords};
use nika_session::{Lang, ProposalId};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::inspect::{Inspected, title_row_in};
use super::lang::say;
use super::text::{HANG, fold, hang, marks, own, twins, wrap};
use crate::visual::role;

/// The cells each effect after the first stands in under the review's one
/// `when it runs` heading.
const HUNG: usize = 2;

/// How the faces' witness row says a key turns the face, while the object holds the keys.
const FACES: &str = "Left/Right change the face";

/// The same, while the box holds them: the key that brings them first.
const FACES_AWAY: &str = "F6 then Left/Right change the face";

/// What a yes answers while a proposal waits: nothing is saved yet and
/// nothing has run, in `lang`.
fn answers(sep: &str, lang: Lang, ascii: bool) -> String {
    let words = "what a yes answers{sep}not saved{sep}nothing has run on your files";
    say(lang, ascii, words).replace("{sep}", sep)
}

/// Whether a component witness names admitted held reuse: `expanded` (its
/// nodes, as bound) and `invoked` (its calling task, as receipted) are two
/// distinct admitted states; `revised`, `absent`, `unreadable` and
/// `unwitnessed` need attention.
#[must_use]
pub fn admitted(witness: &str) -> bool {
    matches!(witness, "expanded" | "invoked")
}

/// Whether a compile record states an ordinary making of its bytes: a
/// revision by operations, or a creation (`written`, `composed`) with no
/// base. A replacement or a record of another kind needs attention.
fn usual(record: &DocumentRevision) -> bool {
    match record.mode.as_str() {
        "operations" => true,
        "written" | "composed" => record.base_sha256.is_none(),
        _ => false,
    }
}

/// The role a fact wears: a `plain` one its own, any other attention.
fn tone(plain: bool, role: Role) -> Role {
    if plain { role } else { Role::Warn }
}

/// What a `save & run` of a candidate runs once its save checked clean, as
/// the Session's typed method admits it (`ProjectChangeSet::save_run`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunAfter {
    /// The one workflow the candidate saves, at the run admission's own
    /// spending.
    Saved,
    /// The run the request itself carried, in words: its workflow, the
    /// ceiling it states and the names of its inputs, never their values.
    Asked(String),
}

/// The candidate under review, fixed at its fold.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Proposed {
    id: ProposalId,
    aside: bool,
    /// The compiler's draft while its question waits: no proposal yet, nothing to consent.
    draft: bool,
    changes: Vec<String>,
    effects: Option<Vec<String>>,
    /// How its workflow was revised over the complete document and which components it holds,
    /// each with whether it needs attention: `(words, warn)`.
    revision: Vec<(String, bool)>,
    /// The typed compile record the Session binds to these exact bytes, kept
    /// only where it names the shown workflow ([`Self::recording`]).
    record: Option<DocumentRevision>,
    /// Where each audited workflow reaches as declared, and whether that leaves this machine
    /// (or cannot be told): `(words, outside)`.
    world: Vec<(String, bool)>,
    /// What a `save & run` of it runs, when the Session's typed method admits one.
    after: Option<RunAfter>,
    /// The schedule the declaration it lands says, in the person's words as the Session gave
    /// them ([`Self::scheduling`]).
    schedule: Option<ScheduleWords>,
    rehearsed: Option<String>,
    unshown: usize,
    look: Inspected,
    /// The language the person writes, which the view's own words follow.
    lang: Lang,
    /// The object holds the keys ([`Proposed::keys_here`]).
    here: bool,
}

impl PartialEq for Proposed {
    /// Two folds are the same candidate when every fact they show is the
    /// same: the identity, the standing, the changes, the reach, what a
    /// `save & run` would run, the rehearsal, and the bytes at the same path.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.aside == other.aside
            && self.draft == other.draft
            && self.changes == other.changes
            && self.effects == other.effects
            && self.revision == other.revision
            && self.record == other.record
            && self.world == other.world
            && self.after == other.after
            && self.schedule == other.schedule
            && self.rehearsed == other.rehearsed
            && self.unshown == other.unshown
            && self.look == other.look
            && self.lang == other.lang
    }
}

impl Eq for Proposed {}

impl Proposed {
    /// The candidate `id` (set `aside` while a revision's question waits)
    /// over the look of its exact pending bytes.
    #[must_use]
    pub fn new(id: ProposalId, aside: bool, look: Inspected) -> Self {
        Self {
            id,
            aside,
            draft: false,
            changes: Vec::new(),
            effects: None,
            revision: Vec::new(),
            record: None,
            world: Vec::new(),
            after: None,
            schedule: None,
            rehearsed: None,
            unshown: 0,
            look,
            lang: Lang::En,
            here: true,
        }
    }

    /// With its faces' key cue said from where the keys are: `here` (the object
    /// holds them) names `Left/Right`, else `F6 then Left/Right`, the key that
    /// brings them from the box first; its look's cues follow. The object holds
    /// them by default.
    #[must_use]
    pub fn keys_here(mut self, here: bool) -> Self {
        self.here = here;
        self.look = self.look.keys_here(here);
        self
    }

    /// The faces' key cue, from where the keys are.
    const fn faces(&self) -> &'static str {
        if self.here { FACES } else { FACES_AWAY }
    }

    /// With the view's own words (its review's headings, its faces' first
    /// rows, its identity on the review's border) said in `lang`: English by
    /// default. The Session's facts and every value keep their words.
    #[must_use]
    pub fn speaking(mut self, lang: Lang) -> Self {
        self.lang = lang;
        // Its faces are the look's: they speak the same language.
        self.look = self.look.speaking(lang);
        self
    }

    /// The compiler's draft while its question waits, before any proposal: set aside, it names
    /// no consent, lands nothing and was neither audited nor rehearsed.
    #[must_use]
    pub fn drafted(mut self) -> Self {
        self.aside = true;
        self.draft = true;
        self
    }

    /// With every change it lands, in words.
    #[must_use]
    pub fn changing(mut self, changes: Vec<String>) -> Self {
        self.changes = changes;
        self
    }

    /// With what the Session's audit says its workflow reaches when it runs;
    /// `None` when the Session audited no workflow of it (a project file).
    #[must_use]
    pub fn reaching(mut self, effects: Option<Vec<String>>) -> Self {
        self.effects = effects;
        self
    }

    /// With how its workflow was revised over the complete document, as the compile record
    /// states it, and whether each row needs attention.
    #[must_use]
    pub fn revising(mut self, revision: Vec<(String, bool)>) -> Self {
        self.revision = revision;
        self
    }

    /// With the typed compile record bound to its shown workflow's bytes:
    /// the check face details it (its `reuse` rows).
    #[must_use]
    pub fn recording(mut self, record: Option<DocumentRevision>) -> Self {
        self.record = record;
        self
    }

    /// With where each audited workflow reaches as the check declares it, and whether that
    /// leaves this machine (or cannot be told).
    #[must_use]
    pub fn declaring(mut self, world: Vec<(String, bool)>) -> Self {
        self.world = world;
        self
    }

    /// With what a `save & run` of it runs once its save checked clean, as
    /// the Session's typed method admits it (`None`: the method refuses).
    #[must_use]
    pub fn running(mut self, after: Option<RunAfter>) -> Self {
        self.after = after;
        self
    }

    /// With how many of its changes have bytes the faces do not show.
    #[must_use]
    pub fn unshown(mut self, changes: usize) -> Self {
        self.unshown = changes;
        self
    }

    /// The same candidate with the schedule the declaration it lands says (`nika.yaml`), in the
    /// person's words as the Session gave them: the card says when it runs, what a missed run
    /// does and the most each run may cost, never the cron; the file view keeps the bytes (live
    /// gauntlet 5, r01 B6 and r08 B14: « cadence: "TZ=Indian/Mauritius 0 20 * * *" » was the
    /// card's only word of it).
    #[must_use]
    pub fn scheduling(mut self, schedule: Option<ScheduleWords>) -> Self {
        self.schedule = schedule;
        self
    }

    /// With the words of the rehearsal proof bound to its identity.
    #[must_use]
    pub fn rehearsed(mut self, words: Option<String>) -> Self {
        self.rehearsed = words;
        self
    }

    /// The identity a consent names.
    #[must_use]
    pub fn id(&self) -> &ProposalId {
        &self.id
    }

    /// Whether a revision's question set it aside (not consentable meanwhile).
    #[must_use]
    pub fn aside(&self) -> bool {
        self.aside
    }

    /// Whether it is the compiler's draft at a question rather than a proposal.
    #[must_use]
    pub fn draft(&self) -> bool {
        self.draft
    }

    /// Where its workflow would land, relative to the project's root.
    #[must_use]
    pub fn path(&self) -> &str {
        self.look.path()
    }

    /// The witness of its exact pending bytes (blake3, hex).
    #[must_use]
    pub fn witness(&self) -> Option<&str> {
        self.look.witness()
    }

    /// The look of its pending bytes (kept when the next fold has the same).
    #[must_use]
    pub fn look(&self) -> &Inspected {
        &self.look
    }

    /// First preview: the observed graph when available, otherwise the exact source.
    #[must_use]
    pub fn initial_face(&self) -> Face {
        if self.look.graph().is_some() {
            Face::Graph
        } else {
            Face::Source
        }
    }

    /// How the aside and the conversation panel name it. A draft lands
    /// nowhere yet: it takes the title its object shows, never the path it
    /// holds until a proposal says where, and says it is unsaved without one.
    #[must_use]
    pub fn label(&self) -> String {
        self.label_in(false)
    }

    /// [`Self::label`] in its language ([`Self::speaking`]) and the glyph
    /// column in use: its path and title as they are.
    #[must_use]
    pub fn label_in(&self, ascii: bool) -> String {
        if !self.draft {
            return say(self.lang, ascii, "proposal {path}").replace("{path}", self.path());
        }
        let title = self.look.title();
        if title == self.path() {
            say(self.lang, ascii, "draft (unsaved)").to_owned()
        } else {
            say(self.lang, ascii, "draft {title}").replace("{title}", title)
        }
    }

    /// The conversation's review of it while a consent can name it (neither
    /// the compiler's draft nor set aside): only what changes the decision,
    /// every change it lands, how it was revised, the run a `save & run`
    /// would ask when the request carried one, what the workflow reaches
    /// when it runs (one `when it runs` heading, each further effect hung
    /// under it) and where, and its rehearsal. Every fact once, each with its
    /// role; the identity a consent names rides the card's border, and the
    /// standing has the status row.
    #[must_use]
    pub fn review(&self, ascii: bool) -> Option<Review> {
        if self.draft || self.aside {
            return None;
        }
        let (sep, _) = marks(ascii);
        let runs = self.after.is_some();
        let mut review =
            Review::new(self.id.clone(), self.facts(sep, true, ascii), runs).named(self.path());
        review.lang = self.lang;
        Some(review)
    }

    /// The facts above every face: the identity and what a yes answers, the
    /// changes, what the workflow reaches, the rehearsal; each with its role.
    fn head(&self, ascii: bool) -> Vec<(String, Role)> {
        let (sep, _) = marks(ascii);
        let said = |en: &str| say(self.lang, ascii, en).replace("{sep}", sep);
        if self.draft {
            // Before any proposal: no identity, no audit, no rehearsal to state.
            let first = said("draft{sep}the compiler's question waits{sep}not a proposal yet");
            return vec![(first, Role::Strong)];
        }
        let standing = if self.aside {
            said("set aside while the revision's question waits{sep}not consentable now")
        } else {
            answers(sep, self.lang, ascii)
        };
        // Named by where it lands, never by its identity's hex (live gauntlet 4: « proposition
        // b5973f37d7a7 »): the Session's whole words (F2) keep the identity.
        let first = (said("proposal {path}{sep}{standing}"))
            .replace("{standing}", &standing)
            .replace("{path}", self.path());
        let mut rows = vec![(first, Role::Strong)];
        let facts = self.facts(sep, false, ascii).into_iter();
        rows.extend(facts.map(|(words, role, _)| (words, role)));
        rows
    }

    /// The facts under the standing, in their order, each with its role and
    /// the cells its first row stands in: every change, the changes whose
    /// bytes no face shows, how the workflow was revised, the run its own
    /// request carried, what it reaches when it runs (`grouped`: each effect
    /// after the first hung under one `when it runs` heading), where it
    /// reaches as declared, and its rehearsal.
    fn facts(&self, sep: &str, grouped: bool, ascii: bool) -> Vec<(String, Role, usize)> {
        let said = |en: &str| say(self.lang, ascii, en).replace("{sep}", sep);
        // A heading of the view's, then the Session's own words after it.
        let headed = |heading: &str, words: &str| format!("{}{sep}{words}", said(heading));
        let mut rows: Vec<(String, Role, usize)> = Vec::new();
        rows.extend(self.changes.iter().map(|c| (c.clone(), Role::Accent, 0)));
        if self.unshown > 0 {
            let unshown = said(
                "{n} more change(s) whose bytes these faces do not show{sep}`/show` prints every byte",
            );
            rows.push((
                unshown.replace("{n}", &self.unshown.to_string()),
                Role::Warn,
                0,
            ));
        }
        rows.extend(self.revision.iter().map(|(words, warn)| {
            let role = if *warn { Role::Warn } else { Role::Dim };
            // The card chooses among a row's whole forms; the faces' head shows the first.
            let first = words.split(OR).next().filter(|_| !grouped);
            (first.unwrap_or(words).to_owned(), role, 0)
        }));
        if let Some(RunAfter::Asked(asked)) = &self.after {
            rows.push((asked.clone(), Role::Accent, 0));
        }
        if let Some(schedule) = &self.schedule {
            // The Session's words, set in after the view's heading; a grouped card hangs the
            // missed run and the ceiling under the cadence.
            let when = format!("{}{sep}{}", schedule.workflow, schedule.cadence);
            rows.push((headed("schedule", &when), Role::Accent, 0));
            let hang = if grouped { HUNG } else { 0 };
            rows.push((schedule.missed.clone(), Role::Dim, hang));
            rows.push((schedule.ceiling.clone(), Role::Dim, hang));
        }
        let runs = "when it runs";
        match self.effects.as_deref() {
            // A declaration runs nothing itself: its schedule says what runs, and when.
            None if self.schedule.is_some() => {}
            None => rows.push((
                headed(
                    runs,
                    &said("not a workflow the Session audited: it runs nothing"),
                ),
                Role::Dim,
                0,
            )),
            Some([]) => rows.push((
                headed(runs, &said("nothing outside the process")),
                Role::Dim,
                0,
            )),
            Some(effects) => {
                for (at, effect) in effects.iter().enumerate() {
                    // The check's own rows, said in French for a French person as the Session's
                    // proposal says them (live gauntlet wave 8, 2026-10-19, r04: « à l'exécution ·
                    // writes ./digest/*.md » and « pauses for a human answer » on a French card).
                    // Under the ASCII column its French phrase loses its accents, unless its
                    // own values are not ASCII: then it stays as the check wrote it.
                    let french = nika_display::check_render::review::effect_row_fr;
                    let effect = match (self.lang, ascii) {
                        (Lang::Fr, false) => french(effect),
                        (Lang::Fr, true) if effect.is_ascii() => fold(&french(effect)),
                        _ => effect.clone(),
                    };
                    let row = if grouped && at > 0 {
                        (effect, Role::Dim, HUNG)
                    } else {
                        (headed(runs, &effect), Role::Dim, 0)
                    };
                    rows.push(row);
                }
            }
        }
        rows.extend(self.world.iter().map(|(words, outside)| {
            let role = if *outside { Role::Warn } else { Role::Dim };
            (headed("reaches, as declared", words), role, 0)
        }));
        match &self.rehearsed {
            Some(words) => rows.extend(
                words
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| (headed("rehearsal", l.trim()), Role::Dim, 0)),
            ),
            None => rows.push((
                headed("rehearsal", &said("none bound to this identity")),
                Role::Dim,
                0,
            )),
        }
        rows
    }

    /// One face of the candidate on a region `width` cells wide: its title
    /// row, the facts above every face (wrapped, never cut), the witness of
    /// the pending bytes, then the viewer's face of those bytes. Pure.
    #[must_use]
    pub fn face_lines(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        self.face_lines_in(face, width, ascii, color, false)
    }

    /// Short previews keep exact graph rows beside the same proposal facts.
    #[must_use]
    pub fn face_lines_in(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
        compact: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        self.face_with(self.head(ascii), face, width, ascii, color, compact)
    }

    /// The face while the conversation reviews this candidate
    /// ([`Self::review`]): the review is the one home of the facts above
    /// every face and the object's header the one home of its bytes, so the
    /// title names the workflow, the first row says whose exact bytes these
    /// are (the key that turns the face where it fits whole), and the face
    /// follows with no second witness row.
    #[must_use]
    pub fn reviewed_face_lines(
        &self,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
        compact: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let (title, mut body) = self.face_with(Vec::new(), face, width, ascii, color, compact);
        let (sep, _) = marks(ascii);
        let own = self.own_bytes(ascii);
        let said = format!("{own}{sep}{}", say(self.lang, ascii, self.faces()));
        // The face's own witness rows, exactly as they are painted, give way to the first rows:
        // the same words, whole, the keys at their end kept.
        let witness = super::inspect::witness_rows(&said, width, ascii, color);
        let found = (!witness.is_empty())
            .then(|| (body.windows(witness.len())).position(|rows| rows == witness.as_slice()))
            .flatten();
        if let Some(at) = found {
            body.drain(at..at + witness.len());
        }
        for (at, row) in witness.into_iter().enumerate() {
            body.insert(at, row);
        }
        (title, body)
    }

    /// Whose exact pending bytes the faces show: the candidate's own, said
    /// without their witness's hex (live gauntlet 4: « ces octets
    /// 7594d366fc7c »), which names nothing a person acts on.
    fn own_bytes(&self, ascii: bool) -> String {
        let words = if self.draft {
            "these bytes, the draft's own"
        } else {
            "these bytes, the proposal's own"
        };
        say(self.lang, ascii, words).to_owned()
    }

    /// The title row, `facts` (wrapped, never cut), the witness of the
    /// pending bytes, then the viewer's face of those bytes. The title names
    /// the workflow alone, the first row standing for what it is; a draft
    /// says it is one, never where it would land. Pure.
    fn face_with(
        &self,
        facts: Vec<(String, Role)>,
        face: Face,
        width: u16,
        ascii: bool,
        color: bool,
        compact: bool,
    ) -> (Line<'static>, Vec<Line<'static>>) {
        let (sep, cut) = marks(ascii);
        let name = if self.draft {
            (say(self.lang, ascii, "draft{sep}{title}").replace("{sep}", sep))
                .replace("{title}", self.look.title())
        } else {
            self.look.title().to_owned()
        };
        let title = title_row_in(&name, face, width, (ascii, color), self.lang);
        let mut body = Vec::new();
        for (row, tone) in facts {
            for part in wrap(&twins(&row, ascii), usize::from(width), cut) {
                body.push(Line::from(Span::styled(part, role::style(tone, color))));
            }
        }
        let said = format!(
            "{}{sep}{}",
            self.own_bytes(ascii),
            say(self.lang, ascii, self.faces())
        );
        body.extend(
            self.look
                .judged_lines_in(face, width, ascii, color, &said, compact),
        );
        if face == Face::Check {
            body.extend(self.reuse(usize::from(width), ascii, color));
        }
        (title, body)
    }

    /// The compile record bound to these bytes, as one detail of the check
    /// face: how they were made, then each admitted component they hold, what
    /// these bytes show of it now ([`admitted`] or attention), its version,
    /// its release and admitted-file digests and its bindings. Typed facts
    /// only, each row whole ([`hang`]) in the glyph column in use.
    fn reuse(&self, width: usize, ascii: bool, color: bool) -> Vec<Line<'static>> {
        let Some(record) = &self.record else {
            return Vec::new();
        };
        let short = |sha: &str| sha.chars().take(12).collect::<String>();
        // Each phrase is the view's, said in its language; the record's own
        // values (its mode, digests, parts and words) are set in after.
        let said = |en: &'static str| -> &'static str { say(self.lang, ascii, en) };
        let (mode, these) = (&record.mode, short(&record.candidate_sha256));
        let mut making = Vec::new();
        if let Some(base) = &record.base_sha256 {
            making.push(said("over the base, sha256 {base}").replace("{base}", &short(base)));
        }
        if !record.changed.is_empty() {
            let parts = record.changed.join(", ");
            making.push(said("changed · {parts}").replace("{parts}", &parts));
        }
        if !record.preservation.is_empty() {
            let how = &record.preservation;
            making.push(said("preservation · {how}").replace("{how}", how));
        }
        let heading = said("compile record · {mode} · binds these bytes, sha256 {these}")
            .replace("{these}", &these)
            .replace("{mode}", mode);
        let mut sections = vec![(heading, tone(usual(record), Role::Strong), making)];
        for component in &record.components {
            let version = component.version.as_deref().unwrap_or(said("unversioned"));
            let (id, witness) = (&component.id, &component.witness);
            let (release, file) = (&component.release, &component.file_sha256);
            let pairs = [("release {sha}", release), ("file {sha}", file)];
            let digests: Vec<String> = (pairs.into_iter())
                .filter_map(|(label, sha)| {
                    Some(said(label).replace("{sha}", &short(sha.as_ref()?)))
                })
                .collect();
            // The version stands on its own row: a release name fits whole there.
            let mut facts = vec![version.to_owned()];
            if !digests.is_empty() {
                facts.push(digests.join(" · "));
            }
            let bound = component.bindings.iter();
            facts.extend(bound.map(|b| format!("{} = {}", b.path, b.value)));
            let worn = tone(admitted(witness), Role::Good);
            sections.push((format!("{id} · {witness}"), worn, facts));
        }
        let mut lines = vec![Line::default()];
        let dim = role::style(Role::Dim, color);
        for (title, worn, rows) in sections {
            let style = role::style(worn, color);
            let hung = hang(&twins(&title, ascii), 0, width);
            lines.extend(hung.into_iter().map(|row| Line::styled(row, style)));
            let below = rows.iter().flat_map(|r| hang(&twins(r, ascii), 2, width));
            lines.extend(below.map(|row| Line::styled(row, dim)));
        }
        lines
    }
}

/// Parts a fact's whole form from the shorter whole form its card shows
/// where a name in the first (a release) cannot stand whole on one row.
pub const OR: char = '\u{1f}';

/// The candidate a consent can name, as the conversation reviews it: its
/// identity, each fact a yes decides with its role and the cells its first
/// row stands in, and whether the Session's typed method admits a
/// `save & run` of it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Review {
    id: ProposalId,
    /// Where the proposal lands ([`Review::named`]): the name its border reads.
    path: Option<String>,
    facts: Vec<(String, Role, usize)>,
    runs: bool,
    /// The language its own words on the border follow ([`Proposed::speaking`]).
    lang: Lang,
}

impl Review {
    /// The review of the candidate `id` over its typed `facts` (words, role
    /// and the indent of the first row), `runs` when the Session's typed
    /// method admits a `save & run` of it.
    #[must_use]
    pub fn new(id: ProposalId, facts: Vec<(String, Role, usize)>, runs: bool) -> Self {
        Self {
            id,
            path: None,
            facts,
            runs,
            lang: Lang::En,
        }
    }

    /// The same review of the proposal landing at `path`: its border names it
    /// by that path rather than by its identity's hex.
    #[must_use]
    pub fn named(mut self, path: &str) -> Self {
        self.path = Some(path.to_owned());
        self
    }

    /// The identity a consent names.
    #[must_use]
    pub fn id(&self) -> &ProposalId {
        &self.id
    }

    /// Whether the Session's typed method admits a `save & run` of it: the
    /// decision row names that word only then.
    #[must_use]
    pub fn runs(&self) -> bool {
        self.runs
    }

    /// The rows its card paints `width` cells wide, in the glyph column in
    /// use: each fact in its longest form whose every word stands whole on a
    /// row ([`OR`]), from its own indent, every further row hung under it
    /// ([`hang`]). Measuring, painting and scrolling read these same rows.
    #[must_use]
    pub fn lines(&self, color: bool, ascii: bool, width: u16) -> Vec<Line<'static>> {
        let room = usize::from(width).saturating_sub(HANG);
        let whole = |form: &&str| form.split(' ').all(|word| word.width() <= room);
        (self.facts.iter())
            .flat_map(|(words, tone, indent)| {
                let style = role::style(*tone, color);
                let form = (words.split(OR).find(&whole)).or_else(|| words.split(OR).next_back());
                let words = twins(form.unwrap_or_default(), ascii);
                let rows = hang(&words, *indent, usize::from(width));
                rows.into_iter().map(move |row| Line::styled(row, style))
            })
            .collect()
    }

    /// The proposal a consent answers, by where it lands, and the key that
    /// reads the Session's whole words, in the longest whole form `room`
    /// cells hold, else the shortest: a border carries it only where it fits
    /// whole, a row of its own wraps it. A shorter form gives up the word
    /// « proposal » (the card says what it reviews) before what `F2` reads.
    /// Those whole words keep the identity a consent names; its hex names
    /// nothing a person acts on here (live gauntlet 4: « proposition
    /// 9e029c32b75d · F2 : tous les mots »). A review named by no path reads
    /// its identity.
    #[must_use]
    pub fn foot(&self, room: usize, ascii: bool) -> String {
        let path = self.path.clone().unwrap_or_else(|| self.id.to_string());
        let said = |form: &str| say(self.lang, ascii, form).replace("{path}", &path);
        let [whole, cued, bare] = [
            said("proposal {path} · F2: whole words"),
            said("{path} · F2: whole words"),
            format!("{path} · F2"),
        ]
        .map(|form| own(&form, ascii));
        [whole, cued]
            .into_iter()
            .find(|form| form.width() <= room)
            .unwrap_or(bare)
    }
}
