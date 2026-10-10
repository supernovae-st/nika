// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The faces of a run in view beside its standing and graph: the outputs its
//! settlement carried, the files it reported writing as they are now, and
//! what its journal proves. Each face renders only what was already
//! acquired, outside the frame; what it lacks it names, never invents.

use nika_display::run_story::Outputs;
use nika_display::state::TaskState;
use nika_display::theme::Role;
use nika_tui_view::{Availability, Canvas, Content, Meta};
use ratatui::text::{Line, Span};

use super::LiveRun;
use crate::session::acquire::{Fetched, Proven};
use crate::visual::role;
use crate::workspace::text::{marks, wrap};

/// The most files the run reported writing that the files face lists.
pub const FILES_READ: usize = 8;

/// The faces of a run in view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunFace {
    /// Its standing, its graph and its tasks.
    #[default]
    Run,
    /// The outputs its settlement carried.
    Outputs,
    /// The files it reported writing, as they are now.
    Files,
    /// What its journal proves, in the verifier's words.
    Proof,
}

impl RunFace {
    /// Every face, in the order the title lists them.
    pub const ALL: [Self; 4] = [Self::Run, Self::Outputs, Self::Files, Self::Proof];

    /// The face in words.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Outputs => "outputs",
            Self::Files => "files",
            Self::Proof => "proof",
        }
    }
}

/// What a face asks the conversation's host to acquire before it shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Want {
    /// A file the run reported writing, read now.
    File(String),
    /// The run's journal, captured and verified.
    Proof,
    /// The journal of the child run `task` called, by the relation its
    /// settle frame named: read by the host only if it kept that relation.
    Child {
        /// The task whose settle frame named the relation.
        task: String,
        /// The relation as the fold had it when asked.
        relation: nika_display::run_story::ChildRun,
    },
}

/// Rows of words, each wrapped within `cells`, in its role.
pub(super) fn lines_of(
    rows: &[(String, Role)],
    cells: usize,
    ascii: bool,
    color: bool,
) -> Vec<Line<'static>> {
    let (_, cut) = marks(ascii);
    let mut out = Vec::new();
    for (row, tone) in rows {
        for part in wrap(row, cells, cut) {
            out.push(Line::from(Span::styled(part, role::style(*tone, color))));
        }
    }
    out
}

impl LiveRun {
    /// The files the leg's own fold reports written (task, path): the
    /// `nika:write` tasks that succeeded with a path as their output.
    #[must_use]
    pub fn written(&self) -> Vec<(String, String)> {
        let rows = self.view.rows().iter().filter(|r| r.state == TaskState::Ok);
        let writes = rows.filter(|r| {
            let note = r.started_note.as_deref().unwrap_or(&r.note);
            note.contains("nika:write")
        });
        writes
            .filter_map(|r| {
                let path = serde_json::from_str::<String>(r.output_json.as_deref()?).ok()?;
                (!path.is_empty()).then(|| (r.id.clone(), path))
            })
            .take(FILES_READ)
            .collect()
    }

    /// What `face` needs acquired that is not yet. A leg kept from an
    /// earlier session asks its journal first, whatever the face: its
    /// tasks, outputs, files and children come from that reading alone.
    #[must_use]
    pub fn wants(&self, face: RunFace) -> Vec<Want> {
        if self.kept.is_some() && self.proven.is_none() {
            return vec![Want::Proof];
        }
        match face {
            RunFace::Files => (self.written().into_iter())
                .filter(|(_, path)| !self.fetched.iter().any(|f| f.path() == path))
                .map(|(_, path)| Want::File(path))
                .collect(),
            RunFace::Proof
                if self.proven.is_none() && (self.settled.is_some() || self.kept.is_some()) =>
            {
                vec![Want::Proof]
            }
            _ => Vec::new(),
        }
    }

    /// Which reading of the leg is current (a result of an earlier one is
    /// never applied).
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// A file acquired for the files face.
    pub(crate) fn fetched(&mut self, fetched: Fetched) {
        self.revision += 1;
        self.fetched.retain(|f| f.path() != fetched.path());
        self.fetched.push(fetched);
    }

    /// Forget what was acquired: the faces read the files and the journal
    /// again, as they are now (a kept leg's tasks with it).
    pub(crate) fn forget(&mut self) {
        self.revision += 1;
        self.generation += 1;
        self.fetched.clear();
        self.proven = None;
        self.forget_history();
    }

    /// The Proof acquired for the proof face; for a leg kept from an earlier
    /// session, the events its verified journal records are folded anew by
    /// the live rules (its execution's only, within the window).
    pub(crate) fn proven(&mut self, proven: Proven) {
        self.revision += 1;
        self.forget_history();
        if self.kept.is_some() {
            for event in proven.events().unwrap_or_default() {
                if event.execution == self.execution {
                    self.event(event);
                } else {
                    self.foreign += 1;
                }
            }
        }
        self.proven = Some(proven);
    }

    /// A kept leg's fold emptied: what an earlier reading of its journal
    /// lent is gone, each child relation it named counted as dropped.
    fn forget_history(&mut self) {
        if self.kept.is_none() {
            return;
        }
        for row in self.view.rows() {
            if self.view.child(&row.id).is_some() {
                let epoch = self.relations.entry(row.id.clone()).or_default();
                *epoch = epoch.saturating_add(1);
            }
        }
        self.view = nika_display::state::RunView::new();
        self.seen.clear();
        (self.events, self.foreign, self.repeated) = (0, 0, 0);
        (self.beyond, self.disorder) = (0, 0);
    }

    /// The journal a kept leg's tasks come from, when one was read and lent
    /// its events.
    pub(super) fn history(&self) -> Option<&Proven> {
        self.kept.as_ref()?;
        self.proven.as_ref().filter(|p| p.events().is_some())
    }

    /// Where the tasks in view come from, in words.
    pub(super) fn source_words(&self) -> &'static str {
        if self.history().is_some() {
            "as its journal recorded it"
        } else {
            "as the stream folded it"
        }
    }

    /// Where the task outputs in view were carried, in words.
    pub(super) fn medium(&self) -> &'static str {
        if self.history().is_some() {
            "in its journal"
        } else {
            "on the stream"
        }
    }

    /// For a kept leg, why its journal lends nothing yet, or nothing at all.
    pub(super) fn history_missing(&self) -> Option<String> {
        self.kept.as_ref()?;
        let Some(proven) = &self.proven else {
            return Some(
                "Click this run in the project list to read and verify its saved journal."
                    .to_owned(),
            );
        };
        if proven.events().is_some() {
            return None;
        }
        let why = (proven.why())
            .or(proven.projection_why())
            .unwrap_or("its journal lends nothing");
        Some(format!("nothing it records is shown: {why}"))
    }

    /// The outputs face: what the settlement carried, each output shown by
    /// the viewers (masked when it looks secret); unknown before a settlement.
    pub(super) fn outputs_body(&self, canvas: Canvas) -> Vec<Line<'static>> {
        let dim = |text: &str| {
            lines_of(
                &[(text.to_owned(), Role::Dim)],
                usize::from(canvas.width),
                canvas.ascii,
                canvas.color,
            )
        };
        if let Some(why) = self.history_missing() {
            return dim(&why);
        }
        let (outputs, whence) = match (self.history(), &self.settled) {
            (Some(journal), _) => match self.view.workflow_outputs() {
                Some(outputs) => (outputs, format!("its journal {}", journal.trace())),
                None => return dim("its journal holds no terminal frame: its outputs are unknown"),
            },
            (None, Some(settled)) => (&settled.outputs, "its settlement".to_owned()),
            (None, None) => return dim("no settlement yet: the outputs are unknown, never empty"),
        };
        let value = match outputs {
            Outputs::Kept(value) => value,
            Outputs::TooLarge { bytes } => {
                return dim(&format!(
                    "the outputs map held {bytes} bytes: only its size was recorded, not its payload"
                ));
            }
            Outputs::Withheld => {
                return dim(
                    "the outputs map was withheld whole when the run closed: neither its keys nor its values were recorded",
                );
            }
            Outputs::Unreadable => {
                return dim("the outputs map recorded here cannot be read: it is not shown");
            }
            _ if self.history().is_some() => {
                return dim(
                    "this journal records no workflow outputs map (an older engine, or a close that kept none): unknown, never empty",
                );
            }
            _ => return dim("the settlement carried no outputs"),
        };
        let Some(map) = value.as_object() else {
            return self.output("outputs", &value.to_string(), (&whence, canvas));
        };
        if map.is_empty() {
            return dim(&format!("{whence} records an empty outputs map"));
        }
        map.iter()
            .flat_map(|(key, item)| self.output(key, &item.to_string(), (&whence, canvas)))
            .collect()
    }

    /// One output, rendered by the viewers from its JSON text, named after
    /// where it was recorded.
    fn output(
        &self,
        name: &str,
        json: &str,
        (whence, canvas): (&str, Canvas),
    ) -> Vec<Line<'static>> {
        let mut meta = Meta::new(name);
        meta.format = Some("json".to_owned());
        meta.provenance = Some(format!("{} · {whence}", self.label()));
        meta.protected = true;
        meta.availability = Availability::Present;
        let rendered = nika_tui_view::artifact(Content::Text(json), &meta, canvas);
        let cells = usize::from(canvas.width);
        let mut lines = lines_of(
            &[(name.to_owned(), Role::Strong)],
            cells,
            canvas.ascii,
            canvas.color,
        );
        lines.extend(rendered.head(canvas));
        lines.extend(rendered.lines);
        lines.push(Line::default());
        lines
    }

    /// The files face: each file the run reported writing, as read now. The
    /// run left no digest of what it wrote: these are today's bytes, never
    /// called unchanged nor changed since the run.
    pub(super) fn files_body(&self, canvas: Canvas) -> Vec<Line<'static>> {
        let cells = usize::from(canvas.width);
        let row = |tone, text: String| lines_of(&[(text, tone)], cells, canvas.ascii, canvas.color);
        if let Some(why) = self.history_missing() {
            return row(Role::Dim, why);
        }
        let written = self.written();
        if written.is_empty() {
            let why = if self.history().is_some() {
                "its journal reports writing no file (nika:write)"
            } else {
                "the run reported writing no file (nika:write); the trace holds its other effects"
            };
            return row(Role::Dim, why.to_owned());
        }
        let (sep, _) = marks(canvas.ascii);
        let mut lines = Vec::new();
        for (task, path) in written {
            lines.extend(row(
                Role::Strong,
                format!("{path}{sep}reported written by `{task}`"),
            ));
            let Some(fetched) = self.fetched.iter().find(|f| f.path() == path) else {
                lines.extend(row(Role::Dim, "not read yet".to_owned()));
                continue;
            };
            let mut meta = Meta::new(path.clone());
            meta.provenance =
                Some("read now · the run left no digest of the bytes it wrote".to_owned());
            meta.producer = Some(task.clone());
            meta.protected = true;
            let content = match (fetched.bytes(), fetched.missing()) {
                (Some(bytes), _) => {
                    meta.availability = Availability::Present;
                    Content::Bytes(bytes)
                }
                (None, true) => {
                    meta.availability = Availability::Missing;
                    Content::Unread
                }
                (None, false) => {
                    let why = fetched.why().unwrap_or("unknown");
                    lines.extend(row(Role::Warn, format!("not read{sep}{why}")));
                    continue;
                }
            };
            if let Some(witness) = fetched.witness() {
                let short: String = witness.chars().take(12).collect();
                lines.extend(row(Role::Dim, format!("these bytes {short}, read now")));
            }
            let rendered = nika_tui_view::artifact(content, &meta, canvas);
            lines.extend(rendered.head(canvas));
            lines.extend(rendered.lines);
            lines.push(Line::default());
        }
        lines
    }

    /// The proof face: whether the captured journal is this run's, then the
    /// verifier's verdict over those bytes, verbatim; never the run's status.
    pub(super) fn proof_body(&self, cells: usize, ascii: bool, color: bool) -> Vec<Line<'static>> {
        let (sep, _) = marks(ascii);
        let mut rows: Vec<(String, Role)> = Vec::new();
        let Some(proven) = &self.proven else {
            let why = if self.settled.is_some() || self.kept.is_some() {
                "the journal is captured and verified when this face opens"
            } else {
                "no settlement yet: there is no journal to verify"
            };
            rows.push((why.to_owned(), Role::Dim));
            return lines_of(&rows, cells, ascii, color);
        };
        rows.push((
            format!("journal {}{sep}as the settlement named it", proven.trace()),
            Role::Strong,
        ));
        if let Some(why) = proven.why() {
            rows.push((format!("not verified{sep}{why}"), Role::Warn));
            return lines_of(&rows, cells, ascii, color);
        }
        let tone = verdict(proven, sep, &mut rows);
        if proven.unbound().is_empty() {
            rows.push((
                format!("this run's journal{sep}its execution, source and receipt match"),
                tone,
            ));
        } else {
            rows.push((format!("not bound to this run{sep}no badge"), Role::Warn));
            for why in proven.unbound() {
                rows.push((format!("  {why}"), Role::Warn));
            }
        }
        if let Some(chain) = proven.verdict().and_then(|d| d.get("chain")) {
            let text = |key| {
                chain
                    .get(key)
                    .map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned))
            };
            let head: String = text("head").unwrap_or_default().chars().take(12).collect();
            let (shape, events) = (
                text("headline").unwrap_or_default(),
                text("events").unwrap_or_default(),
            );
            rows.push((
                format!("chain {shape}{sep}{events} events{sep}head {head}"),
                Role::Dim,
            ));
        }
        let verdict_lines = (proven.verdict())
            .and_then(|d| d.get("lines"))
            .and_then(serde_json::Value::as_array)
            .map(|lines| {
                lines
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for line in verdict_lines {
            rows.push((line, Role::Dim));
        }
        let terminal = proven
            .terminal()
            .unwrap_or("none: the journal holds no terminal frame");
        rows.push((format!("terminal frame{sep}{terminal}"), Role::Dim));
        rows.push((
            "a verified journal records what happened; it never proves the work was right"
                .to_owned(),
            Role::Dim,
        ));
        lines_of(&rows, cells, ascii, color)
    }
}

/// The rows of `proven`'s verdict: the witness of the captured bytes, then
/// the verifier's tier and exit in the tone (returned) an exit other than 0
/// warns with.
pub(super) fn verdict(proven: &Proven, sep: &str, rows: &mut Vec<(String, Role)>) -> Role {
    if let Some(witness) = proven.witness() {
        let short: String = witness.chars().take(12).collect();
        rows.push((
            format!("captured bytes {short}{sep}the verdict below is theirs"),
            Role::Dim,
        ));
    }
    let tier = proven.tier().unwrap_or("unknown").to_uppercase();
    let exit = proven
        .exit()
        .map_or_else(|| "unknown".to_owned(), |e| e.to_string());
    let tone = if proven.exit() == Some(0) {
        Role::Accent
    } else {
        Role::Warn
    };
    rows.push((
        format!("verdict{sep}{tier}{sep}exit {exit}{sep}the verifier's, over these bytes"),
        tone,
    ));
    tone
}
