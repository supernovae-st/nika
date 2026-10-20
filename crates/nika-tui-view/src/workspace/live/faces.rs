// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The faces of a run in view beside its standing and graph: the outputs its
//! settlement carried, the files it reported writing as they are now, and
//! what its journal proves. Each face renders only what was already
//! acquired, outside the frame; what it lacks it names, never invents.

use crate::{Availability, Canvas, Content, Meta};
use nika_display::run_story::Outputs;
use nika_display::state::TaskState;
use nika_display::theme::Role;
use ratatui::text::{Line, Span};

use super::LiveRun;
use super::acquired::{Fetched, Proven};
use crate::lang::{Voice, fill};
use crate::visual::role;
use crate::workspace::text::{marks, wrap};

/// The most files the run reported writing that the files face lists.
pub const FILES_READ: usize = 8;

/// The tool whose output is the path it wrote.
const WRITE: &str = "nika:write";

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

    /// [`Self::label`] in `voice`: the name its tab is painted with.
    #[must_use]
    pub fn label_in(self, voice: crate::lang::Voice) -> &'static str {
        voice.say(self.label())
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

/// The paths a `nika:write` task's output names: its one path, or each path
/// a fan-out of it wrote, in item order; nothing else is read as a path.
fn paths(output: Option<&str>) -> Vec<String> {
    let path = |value: serde_json::Value| match value {
        serde_json::Value::String(path) if !path.is_empty() => Some(path),
        _ => None,
    };
    match output.and_then(|json| serde_json::from_str(json).ok()) {
        Some(serde_json::Value::Array(items)) => items.into_iter().filter_map(path).collect(),
        Some(value) => path(value).into_iter().collect(),
        None => Vec::new(),
    }
}

impl LiveRun {
    /// The files the leg's own fold reports written (task, path): the
    /// `nika:write` tasks that succeeded, each path their output names. A
    /// task is a write when its start note names the tool, or when the bytes
    /// the run names declare it (a fan-out's note counts items, and a resumed
    /// leg's cache hit names no tool: its file was written before the gate,
    /// by the same run).
    #[must_use]
    pub fn written(&self) -> Vec<(String, String)> {
        let declared = (self.binding == super::Binding::Same)
            .then(|| self.look.as_ref().and_then(super::Inspected::graph))
            .flatten();
        let writes_by_definition = |id: &str| {
            declared.is_some_and(|(doc, _)| {
                (doc.nodes.iter()).any(|node| node.id == id && node.tool.as_deref() == Some(WRITE))
            })
        };
        let rows = self.view.rows().iter().filter(|r| r.state == TaskState::Ok);
        let writes = rows.filter(|r| {
            let note = r.started_note.as_deref().unwrap_or(&r.note);
            note.contains(WRITE) || writes_by_definition(&r.id)
        });
        writes
            .flat_map(|r| {
                let task = r.id.clone();
                paths(r.output_json.as_deref())
                    .into_iter()
                    .map(move |path| (task.clone(), path))
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
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// A file acquired for the files face.
    pub fn fetched(&mut self, fetched: Fetched) {
        self.revision += 1;
        self.fetched.retain(|f| f.path() != fetched.path());
        self.fetched.push(fetched);
    }

    /// Forget what was acquired: the faces read the files and the journal
    /// again, as they are now (a kept leg's tasks with it).
    pub fn forget(&mut self) {
        self.revision += 1;
        self.generation += 1;
        self.fetched.clear();
        self.proven = None;
        self.forget_history();
    }

    /// The Proof acquired for the proof face; for a leg kept from an earlier
    /// session, the events its verified journal records are folded anew by
    /// the live rules (its execution's only, within the window).
    pub fn proven(&mut self, proven: Proven) {
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
    #[must_use]
    pub fn history(&self) -> Option<&Proven> {
        self.kept.as_ref()?;
        self.proven.as_ref().filter(|p| p.events().is_some())
    }

    /// Where the tasks in view come from, in words.
    #[must_use]
    pub fn source_words(&self) -> &'static str {
        if self.history().is_some() {
            "as its journal recorded it"
        } else {
            "as the stream folded it"
        }
    }

    /// Where the task outputs in view were carried, in words.
    #[must_use]
    pub fn medium(&self) -> &'static str {
        if self.history().is_some() {
            "in its journal"
        } else {
            "on the stream"
        }
    }

    /// For a kept leg, why its journal lends nothing yet, or nothing at all, in `voice`.
    #[must_use]
    pub fn history_missing(&self, voice: Voice) -> Option<String> {
        self.kept.as_ref()?;
        let Some(proven) = &self.proven else {
            let click = "Click this run in the project list to read and verify its saved journal.";
            return Some(voice.say(click).to_owned());
        };
        if proven.events().is_some() {
            return None;
        }
        let why = (proven.why())
            .or(proven.projection_why())
            .unwrap_or_else(|| voice.say("its journal lends nothing"));
        Some(
            voice
                .say("nothing it records is shown: {why}")
                .replace("{why}", why),
        )
    }

    /// The outputs face: what the settlement carried, each output shown by
    /// the viewers (masked when it looks secret); unknown before a settlement.
    #[must_use]
    pub fn outputs_body(&self, canvas: Canvas, voice: Voice) -> Vec<Line<'static>> {
        let shown = |text: String| {
            lines_of(
                &[(text, Role::Dim)],
                usize::from(canvas.width),
                canvas.ascii,
                canvas.color,
            )
        };
        let dim = |text: &str| shown(voice.say(text).to_owned());
        if let Some(why) = self.history_missing(voice) {
            return shown(why);
        }
        let (outputs, whence) = match (self.history(), &self.settled) {
            (Some(journal), _) => match self.view.workflow_outputs() {
                Some(outputs) => {
                    let whence = voice.say("its journal {trace}");
                    (outputs, whence.replace("{trace}", journal.trace()))
                }
                None => return dim("its journal holds no terminal frame: its outputs are unknown"),
            },
            (None, Some(settled)) => (&settled.outputs, voice.say("its settlement").to_owned()),
            (None, None) => return dim("no settlement yet: the outputs are unknown, never empty"),
        };
        let value = match outputs {
            Outputs::Kept(value) => value,
            Outputs::TooLarge { bytes } => {
                let words = voice.say(
                    "the outputs map held {bytes} bytes: only its size was recorded, not its payload",
                );
                return shown(words.replace("{bytes}", &bytes.to_string()));
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
            let name = voice.say("outputs");
            return self.output(name, &value.to_string(), (&whence, canvas));
        };
        if map.is_empty() {
            let words = voice.say("{whence} records an empty outputs map");
            return shown(words.replace("{whence}", &whence));
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
        // The detail keeps the run's hex, as its trace names it.
        let named = (self.hex()).map_or_else(|| self.label(), |hex| format!("run {hex}"));
        meta.provenance = Some(format!("{named} · {whence}"));
        meta.protected = true;
        meta.availability = Availability::Present;
        let rendered = crate::artifact(Content::Text(json), &meta, canvas);
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
    #[must_use]
    pub fn files_body(&self, canvas: Canvas, voice: Voice) -> Vec<Line<'static>> {
        let cells = usize::from(canvas.width);
        let row = |tone, text: String| lines_of(&[(text, tone)], cells, canvas.ascii, canvas.color);
        if let Some(why) = self.history_missing(voice) {
            return row(Role::Dim, why);
        }
        let written = self.written();
        if written.is_empty() {
            let why = if self.history().is_some() {
                "its journal reports writing no file (nika:write)"
            } else {
                "the run reported writing no file (nika:write); the trace holds its other effects"
            };
            return row(Role::Dim, voice.say(why).to_owned());
        }
        let (sep, _) = marks(canvas.ascii);
        let mut lines = Vec::new();
        for (task, path) in written {
            let by = voice.say("{path}{sep}reported written by `{task}`");
            let by = fill(by, &[("{path}", &path), ("{sep}", sep), ("{task}", &task)]);
            lines.extend(row(Role::Strong, by));
            let Some(fetched) = self.fetched.iter().find(|f| f.path() == path) else {
                lines.extend(row(Role::Dim, voice.say("not read yet").to_owned()));
                continue;
            };
            let mut meta = Meta::new(path.clone());
            let whence = "read now · the run left no digest of the bytes it wrote";
            meta.provenance = Some(voice.say(whence).to_owned());
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
                    let why = fetched.why().unwrap_or_else(|| voice.say("unknown"));
                    let words = voice.say("not read{sep}{why}");
                    let words = fill(words, &[("{sep}", sep), ("{why}", why)]);
                    lines.extend(row(Role::Warn, words));
                    continue;
                }
            };
            // Whose bytes these are, without their witness's hex: it names nothing a person
            // acts on here.
            if fetched.witness().is_some() {
                let these = voice.say("these bytes, read now").to_owned();
                lines.extend(row(Role::Dim, these));
            }
            let rendered = crate::artifact(content, &meta, canvas);
            lines.extend(rendered.head(canvas));
            lines.extend(rendered.lines);
            lines.push(Line::default());
        }
        lines
    }

    /// The proof face: whether the captured journal is this run's, then the
    /// verifier's verdict over those bytes, verbatim; never the run's status.
    pub fn proof_body(&self, cells: usize, voice: Voice, color: bool) -> Vec<Line<'static>> {
        let ascii = voice.ascii;
        let (sep, _) = marks(ascii);
        let said = |words: &str| fill(voice.say(words), &[("{sep}", sep)]);
        let mut rows: Vec<(String, Role)> = Vec::new();
        let Some(proven) = &self.proven else {
            let why = if self.settled.is_some() || self.kept.is_some() {
                "the journal is captured and verified when this face opens"
            } else {
                "no settlement yet: there is no journal to verify"
            };
            rows.push((voice.say(why).to_owned(), Role::Dim));
            return lines_of(&rows, cells, ascii, color);
        };
        let named = said("journal {trace}{sep}as the settlement named it");
        rows.push((named.replace("{trace}", proven.trace()), Role::Strong));
        if let Some(why) = proven.why() {
            rows.push((
                said("not verified{sep}{why}").replace("{why}", why),
                Role::Warn,
            ));
            return lines_of(&rows, cells, ascii, color);
        }
        let tone = verdict(proven, sep, &mut rows, voice);
        if proven.unbound().is_empty() {
            let matched = said("this run's journal{sep}its execution, source and receipt match");
            rows.push((matched, tone));
        } else {
            rows.push((said("not bound to this run{sep}no badge"), Role::Warn));
            for why in proven.unbound() {
                rows.push((format!("  {}", voice.say(why)), Role::Warn));
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
            let chain = voice.say("chain {shape}{sep}{events} events{sep}head {head}");
            let chain = fill(
                chain,
                &[
                    ("{shape}", &shape),
                    ("{sep}", sep),
                    ("{events}", &events),
                    ("{head}", &head),
                ],
            );
            rows.push((chain, Role::Dim));
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
        let terminal = (proven.terminal())
            .unwrap_or_else(|| voice.say("none: the journal holds no terminal frame"));
        let terminal = said("terminal frame{sep}{terminal}").replace("{terminal}", terminal);
        rows.push((terminal, Role::Dim));
        let limit = "a verified journal records what happened; it never proves the work was right";
        rows.push((voice.say(limit).to_owned(), Role::Dim));
        lines_of(&rows, cells, ascii, color)
    }
}

/// The rows of `proven`'s verdict in `voice`: the witness of the captured bytes, then the
/// verifier's tier and exit in the tone (returned) an exit other than 0 warns with.
pub(super) fn verdict(
    proven: &Proven,
    sep: &str,
    rows: &mut Vec<(String, Role)>,
    voice: Voice,
) -> Role {
    if let Some(witness) = proven.witness() {
        let short: String = witness.chars().take(12).collect();
        let words = voice.say("captured bytes {short}{sep}the verdict below is theirs");
        rows.push((
            fill(words, &[("{short}", &short), ("{sep}", sep)]),
            Role::Dim,
        ));
    }
    let unknown = voice.say("unknown");
    let tier = proven.tier().unwrap_or(unknown).to_uppercase();
    let exit = (proven.exit()).map_or_else(|| unknown.to_owned(), |e| e.to_string());
    let tone = if proven.exit() == Some(0) {
        Role::Accent
    } else {
        Role::Warn
    };
    let words =
        voice.say("verdict{sep}{tier}{sep}exit {exit}{sep}the verifier's, over these bytes");
    let words = fill(
        words,
        &[("{sep}", sep), ("{tier}", &tier), ("{exit}", &exit)],
    );
    rows.push((words, tone));
    tone
}
