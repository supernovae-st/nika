// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Session's capabilities behind the conversation's tools ([`Desk`]): the project read
//! through a held descriptor, the strict parser, the oracle facade, the language pages this
//! engine embeds, the model choice over this machine's routes, the pinned knowledge, the guarded
//! page observer, the document operations and the judge. The desk owns what it reads, so a
//! run's tools never borrow the Session they serve; none of it writes, runs or sends a workflow.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use nika_cli_host::Theme;
use nika_cli_host::oracle::{AuditOptions, Lanes};
use nika_compile_fidelity::fidelity::resolution::Resolution;
use nika_compile_seats::foundry::{ComponentCatalog, document};
use nika_fs::OwnedDir;
use nika_onboard::compile::copy::native;
use nika_onboard::compile::room::JqHelper;
use nika_onboard::compile::{CompileOutcome, CompileStatus};
use nika_onboard::knowledge::Snapshot;
use nika_onboard::knowledge::pin::KnowledgePin;
use nika_providers::InferenceAdmission;
use nika_providers::probe::ProviderProbe;
use nika_session_agent::conversation::{Citations, Conversation};
use serde_json::{Value, json};

use crate::authoring::{AuthoringContext, AuthoringSeat};
use crate::trial::{Observed, Observer, Trial};

use super::proof::Judged;
use super::tools::Desk;
use crate::change::{ProjectChangeSet, Witness};

/// The largest project file a conversation reads.
const READ_CAP: u64 = 256 * 1024;
/// The lines a read answers when it names no limit.
const READ_LINES: usize = 400;
/// The characters a reference page answers before it says it was cut.
const PAGE_CHARS: usize = 24_000;
/// Plain text for every page an intelligence reads.
const PLAIN: Theme = Theme::new(false, false, false);

/// What the desk reads, owned: the project root, this machine's provider probes, the pinned
/// knowledge release and the conversation's verifier.
pub(crate) struct SessionDesk {
    pub(crate) root: PathBuf,
    /// Exact workflow last saved by this conversation, reused only while its witness matches.
    pub(crate) saved: Option<(String, String)>,
    pub(crate) probes: Vec<ProviderProbe>,
    pub(crate) knowledge: Option<KnowledgePin>,
    pub(crate) verifier: Verifier,
    /// The pinned release, admitted once for the turn when a tool first reads it.
    pub(crate) admitted: Option<Snapshot>,
    /// Where the bytes each verification judged are kept (the history directory); none without
    /// history.
    pub(crate) judged: Option<OwnedDir>,
    /// The Run ceiling a proposal of this turn binds beside its bytes, when the person stated
    /// it: the verifier carries there what only the words stating it ask.
    pub(crate) ceiling: Ceiling,
}

/// The Run ceiling a proposal of this turn binds beside its bytes when the person stated it:
/// the amount the work's own words admitted, wherever the candidate lands, and the one saved
/// with the workflow this conversation saved, which a revision landing there keeps.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Ceiling {
    /// The amount the work's own words admitted.
    pub(crate) stated: Option<f64>,
    /// The workflow this conversation saved, and the amount saved with it.
    pub(crate) saved: Option<(String, f64)>,
}

impl Ceiling {
    /// The ceiling a proposal landing at `at` binds, when the person stated it.
    pub(crate) fn at(&self, at: &str) -> Option<f64> {
        let kept = (self.saved.as_ref()).filter(|(path, _)| path == at);
        self.stated.or_else(|| kept.map(|(_, usd)| *usd))
    }
}

/// The schedule `nika.yaml` declares for the workflow at `at`, as the verifier reads one beside
/// a candidate (`{"message", "excerpt", "zone"}`): the person's own words that state it, quoted
/// as typed, the zone its cadence follows, and the file that declares it. None when nothing
/// declares one, or the person's words state none.
pub(crate) fn declared_beside(root: &std::path::Path, at: &str, stated: &str) -> Option<Value> {
    let entry = crate::runtime::schedule::declared_arm_entry(root, std::path::Path::new(at))?;
    let zone = (entry.cadence.strip_prefix("TZ="))
        .and_then(|rest| rest.split_whitespace().next())?
        .to_owned();
    let excerpt = (stated.split('\n')).find_map(|line| {
        let words = crate::runtime::schedule::stated_cadence(line)?;
        verbatim(line, &words)
    })?;
    Some(json!({"message": "nika.yaml", "excerpt": excerpt, "zone": zone}))
}

/// The person's own bytes in `line` for `words` (lowered, as the Session's schedule reader gives
/// them): the run of the line's words that read as them, quoted as typed, so the verifier
/// anchors exactly the schedule's words and nothing else of the line.
fn verbatim<'a>(line: &'a str, words: &str) -> Option<&'a str> {
    let wanted: Vec<&str> = words.split_whitespace().collect();
    let separator = |c: char| c.is_whitespace() || matches!(c, '!' | ',' | '?' | ';');
    let mut tokens: Vec<(usize, usize, String)> = Vec::new();
    let mut start = None;
    for (at, c) in line.char_indices().chain([(line.len(), ' ')]) {
        match (separator(c), start) {
            (false, None) => start = Some(at),
            (true, Some(from)) => {
                let trimmed = line[from..at].trim_end_matches('.');
                let folded = trimmed.replace('\u{2019}', "'").to_lowercase();
                if !folded.is_empty() {
                    tokens.push((from, from + trimmed.len(), folded));
                }
                start = None;
            }
            _ => {}
        }
    }
    (0..tokens.len()).find_map(|first| {
        let run = tokens.get(first..first + wanted.len())?;
        let same = (run.iter().zip(&wanted)).all(|(token, word)| token.2 == *word);
        same.then(|| line.get(run.first()?.0..run.last()?.1))
            .flatten()
    })
}

/// The person's words that state `usd` as the work's ceiling, beside that amount
/// (`{"excerpt", "usd"}`): each of their lines read as the Session reads it, the directives of
/// the lines stating exactly `usd`, those side by side quoted together (QA gauntlet q09: « …
/// chaque exécution doit coûter au maximum 0,10 $ (budget max 0,10 $ par run) »), and those of
/// the earlier lines stating another ceiling, which the last line stating `usd` supersedes (live
/// gauntlet r09, 2026-10-17: with « plafond 0,001 $ » bound, the 0,10 $ of the first request was
/// asked of the plan). None when no line states `usd`; a ceiling stated after it is not beside.
pub(crate) fn ceiling_words(stated: &str, usd: f64) -> Option<Value> {
    let lines: Vec<(&str, Option<f64>)> = (stated.split('\n'))
        .map(|line| {
            let amount = (nika_onboard::compile::money::directives(line).ok())
                .and_then(|found| found.money.amount);
            (line, amount)
        })
        .collect();
    let last =
        (lines.iter()).rposition(|(_, amount)| amount.map(f64::to_bits) == Some(usd.to_bits()))?;
    let mut said: Vec<&str> = Vec::new();
    for (line, amount) in &lines[..=last] {
        if amount.is_none() {
            continue;
        }
        let Ok(found) = nika_onboard::compile::money::directives(line) else {
            continue;
        };
        let mut spans = found.found.iter().map(|directive| directive.span.clone());
        let Some(mut run) = spans.next() else {
            continue;
        };
        for span in spans {
            let between = line.get(run.end..span.start).unwrap_or("·");
            if between
                .chars()
                .all(|c| c.is_whitespace() || c.is_ascii_punctuation())
            {
                run.end = span.end;
            } else {
                said.extend(quoted(line, run));
                run = span;
            }
        }
        said.extend(quoted(line, run));
    }
    let excerpt = said.join(" · ");
    (!excerpt.trim().is_empty()).then(|| json!({"excerpt": excerpt, "usd": usd}))
}

/// The words of `line` in `span`, with the bracket that closes an aside they open, or both
/// brackets of an aside they fill.
fn quoted(line: &str, span: std::ops::Range<usize>) -> Option<&str> {
    let (mut start, mut end) = (span.start, span.end);
    let words = line.get(start..end)?;
    let open = words.matches('(').count() > words.matches(')').count();
    let closed = line.get(end..).is_some_and(|after| after.starts_with(')'));
    if open && closed {
        end += 1;
    } else if !open && closed && line.get(..start).is_some_and(|b| b.ends_with('(')) {
        start -= 1;
        end += 1;
    }
    line.get(start..end)
}

/// The workflow file the person's request names for a new workflow, read against `source` from
/// their words since the request began without the lines they wrote in reply to Nika's questions
/// (`Conversation::replies_in`), bound or not: one reading for the verifier and the proposal.
pub(super) fn requested(
    conversation: &Conversation,
    citations: &Citations,
    source: &str,
) -> Option<String> {
    let stated = citations.stated(conversation.since());
    let replies = conversation.replies_in(citations);
    nika_compile_fidelity::fidelity::requested_workflow_answered(&stated, source, &replies)
}

/// Reuse a known workflow's destination only when its on-disk bytes are still observed.
/// A revision updates that workflow instead of creating a numbered duplicate. A new workflow lands
/// at the file the person's request names for it (`requested`), whatever its `nika:`, else where
/// its `nika:` puts it.
pub(super) fn landing(
    root: &std::path::Path,
    source: &str,
    (saved, read): (Option<&(String, String)>, &BTreeMap<String, Witness>),
    requested: Option<&str>,
) -> Result<String, String> {
    let id = crate::review::workflow_id(source);
    let mut known: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    if let Some((path, witness)) = saved {
        known.entry(path.clone()).or_default().push(witness);
    }
    for (path, witness) in read {
        if std::path::Path::new(path)
            .extension()
            .is_some_and(|extension| extension == "nika")
        {
            known
                .entry(path.trim_start_matches("./").to_owned())
                .or_default()
                .push(&witness.0);
        }
    }
    let project = OwnedDir::open(root).map_err(|e| e.to_string())?;
    // The workflow actually saved by this conversation takes precedence over a
    // reference or copy read later, even when that file sorts before it.
    let mut ordered: Vec<_> = known.into_iter().collect();
    ordered.sort_by_key(|(path, _)| saved.is_none_or(|(saved, _)| saved != path));
    let mut observed_match = None;
    for (path, seen) in ordered {
        let parts: Vec<&str> = path.split('/').collect();
        let Some((name, dirs)) = parts.split_last() else {
            continue;
        };
        let Ok(Some(text)) = project.read_capped_below(dirs, name, READ_CAP, "too large to revise")
        else {
            continue;
        };
        if crate::review::workflow_id(&text) != id {
            continue;
        }
        if !seen.contains(&Witness::of(text.as_bytes()).0.as_str()) {
            return Err(format!(
                "`{path}` changed since this conversation saved or read it: read it again and preserve those changes before proposing"
            ));
        }
        if saved.is_some_and(|(saved, _)| saved == &path) {
            return Ok(path);
        }
        if observed_match.replace(path).is_some() {
            return Err("several observed workflows have this name; identify the workflow to revise before proposing".to_owned());
        }
    }
    if let Some(path) = observed_match {
        return Ok(path);
    }
    // The change set witnesses it as any destination: absent, a create; present, an update the
    // person's consent covers.
    if let Some(path) = requested {
        return Ok(path.to_owned());
    }
    crate::review::destination(root, source)
        .map(|path| path.display().to_string())
        .ok_or_else(|| "the candidate has no representable destination in the project".to_owned())
}

/// The most of one judged candidate the Session keeps.
const JUDGED_BYTES: usize = 256 * 1024;

/// Keep the bytes a verification judged, redacted and bounded, as `verified/<sha256>.nika` under
/// `dir`: the name is the digest the verify record carries, so a hold can be checked against its
/// exact bytes once the candidate is rewritten. Kept once; best effort, as no verdict waits on it.
pub(super) fn keep_judged(dir: &OwnedDir, source: &str) {
    let name = format!("{}.nika", nika_compile::surface::sha256(source));
    let (text, _) = nika_cli_host::context_redaction::redact(source);
    let mut end = text.len().min(JUDGED_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if let Ok(kept) = dir.create_below(&["verified"])
        && !kept.exists(&name).unwrap_or(true)
    {
        let _ = kept.write_once(&name, text.get(..end).unwrap_or_default());
    }
}

/// The judge a conversation's document faces: the seat and decision model a compile round of
/// the same request would face, the account the line's dispatch admitted, and what the
/// conversation's verifications kept.
pub(crate) struct Verifier {
    pub(crate) seat: AuthoringSeat,
    pub(crate) context: AuthoringContext,
    pub(crate) account: Option<InferenceAdmission>,
    pub(crate) kept: Arc<Mutex<Verified>>,
    /// What a trial is lent: the jq helper, and the observer of the pages it runs on.
    pub(crate) trial: (Option<JqHelper>, Observer),
}

/// What a conversation's verifications keep across its lines: every verdict that declined its
/// bytes (a judge is never asked again on bytes it rejected), the last verdict that made a
/// document ready (its key and scope), so `propose` after `verify` asks nothing, the pages its
/// trials ran on (observed once each) and the trial of the last ready document with its words.
#[derive(Default)]
pub(crate) struct Verified {
    verdicts: Vec<Value>,
    /// The key of the last ready verdict, its scope and the doubt it stated.
    ready: Option<(String, Judged, Option<String>)>,
    /// The doubt the verifier stated over the last ready document, for its proposal to state.
    doubt: Option<String>,
    /// The project files the author read through the desk, by the path a workflow names, each
    /// with the witness of the bytes it read: what its proposal's basis keeps.
    read: BTreeMap<String, Witness>,
    observed: Observed,
    /// The trial of the last ready document, or why it came to no decision, with its words.
    tried: Option<(Result<native::Preview, String>, String)>,
}

/// The verdicts `verifier` shares with its conversation, or why they cannot be read.
fn kept(verifier: &Verifier) -> Result<MutexGuard<'_, Verified>, String> {
    (verifier.kept.lock()).map_err(|_| "the conversation's verdicts cannot be read".to_owned())
}

impl Verified {
    /// The files the author read, each with the witness of the bytes it read.
    pub(crate) fn read(&self) -> &BTreeMap<String, Witness> {
        &self.read
    }

    /// The doubt the verifier stated over the last ready document: its proposal states it, so
    /// the person reads it before consenting.
    pub(crate) fn doubt(&self) -> Option<&str> {
        self.doubt.as_deref()
    }

    /// The trial of the last ready document, or why it came to no decision, and its words, taken
    /// once by the proposal of it.
    pub(crate) fn take_tried(&mut self) -> Option<(Result<native::Preview, String>, String)> {
        self.tried.take()
    }

    /// Keep every verdict `out` recorded that declined its bytes, once, and the latest reading
    /// of the request its verification audited as covered: the next verification of the same
    /// words, answers, model and prompts reuses that reading with no call, rather than reading
    /// and auditing them again (live gauntlet wave 6 r04, 2026-10-18: the same reading, asked
    /// again at every verification, held most of the session's reasoning). It is kept in this
    /// process's memory only, never on disk: a reopened conversation reads and audits anew. A
    /// reading kept here is never a verdict: no rejection or repair reads it, and the judge
    /// still judges every candidate.
    fn keep(&mut self, out: &CompileOutcome) {
        let decision = out.provenance.decision.as_ref();
        let attempts = decision.and_then(|decision| decision["semantic_verification"].as_array());
        for attempt in attempts.into_iter().flatten() {
            if attempt["declined"] == true && !self.verdicts.contains(attempt) {
                self.verdicts.push(attempt.clone());
            }
        }
        let reading = (decision.map(|decision| &decision["request_interpretation"]))
            .filter(|record| record["audit"]["status"] == "covered" && record["valid"] == true);
        if let Some(record) = reading {
            let read = |kept: &Value| kept["declined"] == false && kept["reading_only"] == true;
            self.verdicts.retain(|kept| !read(kept));
            self.verdicts
                .push(json!({"declined": false, "reading_only": true,
                "request_interpretation": record}));
        }
    }
}

/// What the author repairs from when a document is not ready: each finding of the verification
/// (the laws' refusals, the verifier's defects, doubts and held words) and each open question.
/// The doubt a ready verdict states with its proposal (`verify_doubt`), when it states one.
pub(super) fn stated_doubt(out: &CompileOutcome) -> Option<String> {
    (out.diagnostics.iter())
        .find(|d| d.target == "verify_doubt")
        .map(|d| d.message.clone())
}

/// What the catalog documents of `model`'s output: the reasoning efforts it takes and its
/// output limit, which the `models` tool shows beside each offer.
pub(super) fn model_caps(model: &str) -> Option<Value> {
    nika_compile::surface::output_caps(model)
}

/// Why bytes the engine's own check refuses are not shown: the check's findings, each one, and
/// what the author does about them.
fn failing(audit: &crate::change::WorkflowAudit) -> String {
    let mut lines = vec![format!(
        "the check of these bytes fails (`{}`): the proposal is not shown, so nothing can be saved or run; repair each finding in the candidate, then propose again",
        audit.path.display()
    )];
    lines.extend(audit.findings.iter().map(|finding| format!("- {finding}")));
    lines.join("\n")
}

fn findings(out: &CompileOutcome) -> String {
    let mut lines: Vec<String> = (out.diagnostics.iter())
        .map(|d| format!("{}: {}", d.target, d.message))
        .collect();
    lines.extend((out.questions.iter()).map(|q| format!("question `{}`: {}", q.key, q.label)));
    if lines.is_empty() {
        lines.push("the verifier did not settle the candidate".to_owned());
    }
    lines.join("\n")
}

impl SessionDesk {
    /// What a verdict covers beside the bytes: the Run ceiling the proposal binds rides with
    /// them, as a schedule does, with the person's words that state it; and a schedule
    /// `nika.yaml` already declares for the workflow it lands on rides beside a revision, as one
    /// proposed with a new workflow does (live gauntlet r02 B16, 2026-10-17: a repair was judged
    /// to miss « every morning at 7h »).
    fn presented(&self, presentation: &Value, at: &str, stated: &str) -> Value {
        let mut presentation = presentation.clone();
        if let Some(money) = (self.ceiling.at(at)).and_then(|usd| ceiling_words(stated, usd)) {
            presentation["money"] = money;
        }
        if presentation.get("schedule").is_none()
            && let Some(schedule) = declared_beside(&self.root, at, stated)
        {
            presentation["schedule"] = schedule;
        }
        presentation
    }

    /// The pinned release, admitted once for this turn.
    fn release(&mut self) -> Result<&Snapshot, String> {
        if self.admitted.is_none() {
            let pin =
                (self.knowledge.as_ref()).ok_or("the knowledge release is off in this Session")?;
            self.admitted = Some(pin.reopen().map_err(|e| e.to_string())?);
        }
        (self.admitted.as_ref()).ok_or_else(|| "the knowledge release was not admitted".to_owned())
    }
}

/// `text` cut at `PAGE_CHARS`, saying so.
fn page(text: &str) -> String {
    if text.chars().count() <= PAGE_CHARS {
        return text.to_owned();
    }
    let cut: String = text.chars().take(PAGE_CHARS).collect();
    format!("{cut}\n… (cut at {PAGE_CHARS} characters: ask for a narrower query)")
}

/// The JSON nodes of `node` whose key is `query`, case aside, each with its path.
fn named(node: &Value, query: &str, at: &str, out: &mut Vec<Value>) {
    match node {
        Value::Object(map) => {
            for (key, value) in map {
                let path = format!("{at}/{key}");
                if key.eq_ignore_ascii_case(query) {
                    out.push(json!({"path": path, "definition": value}));
                }
                named(value, query, &path, out);
            }
        }
        Value::Array(items) => {
            for (k, value) in items.iter().enumerate() {
                named(value, query, &format!("{at}/{k}"), out);
            }
        }
        _ => {}
    }
}

/// The visible names a project folder holds, sorted, a folder's with its `/`, at most 100.
fn visible_entries(folder: &OwnedDir) -> Vec<String> {
    let directories = folder.directory_names().unwrap_or_default();
    let mut names: Vec<String> = (folder.names().unwrap_or_default().into_iter())
        .filter(|name| !name.starts_with('.'))
        .map(|name| {
            if directories.contains(&name) {
                format!("{name}/")
            } else {
                name
            }
        })
        .collect();
    names.sort();
    names.truncate(100);
    names
}

impl Desk for SessionDesk {
    fn read(
        &mut self,
        path: &str,
        offset: Option<u64>,
        limit: Option<u64>,
    ) -> Result<String, String> {
        let parts: Vec<&str> = (path.trim().trim_start_matches("./").split('/'))
            .filter(|c| !c.is_empty() && *c != ".")
            .collect();
        let hidden = parts.iter().any(|c| c.starts_with('.') || *c == "..");
        // The project root itself is listed (« . », « ./ »).
        if parts.is_empty() && !path.trim().starts_with('/') {
            let root = OwnedDir::open(&self.root).map_err(|e| e.to_string())?;
            let entries = visible_entries(&root);
            return Ok(json!({"path": path, "directory": true, "entries": entries}).to_string());
        }
        let Some((name, dirs)) = parts
            .split_last()
            .filter(|_| !path.starts_with('/') && !hidden)
        else {
            return Err(format!(
                "`{path}` is not a visible file inside the project: name it relative to the root"
            ));
        };
        let root = OwnedDir::open(&self.root).map_err(|e| e.to_string())?;
        let over = format!(
            "`{path}` is larger than {READ_CAP} bytes; offset and limit cannot help, since the \
             whole file is read before they apply: author from a smaller file, or let the \
             workflow read and filter this one when it runs"
        );
        // A folder is listed, never refused as a file that is not regular, and a file that is not
        // there names what its folder holds (live gauntlet r01, 2026-10-16: DeepSeek guessed
        // fifteen names in `releves/` before it found the two exports).
        if let Ok(folder) = root.open_below(&parts) {
            let entries = visible_entries(&folder);
            return Ok(json!({"path": path, "directory": true, "entries": entries}).to_string());
        }
        let text = (root.read_capped_below(dirs, name, READ_CAP, &over))
            .map_err(|e| format!("`{path}` cannot be read: {e}"))?
            .ok_or_else(|| {
                let folder = format!("./{}", dirs.join("/"));
                let held = root.open_below(dirs).map(|dir| visible_entries(&dir));
                match held {
                    Ok(entries) if entries.is_empty() => {
                        format!("`{path}` does not exist in the project; `{folder}` is empty")
                    }
                    Ok(entries) => format!(
                        "`{path}` does not exist in the project; `{folder}` holds: {}",
                        entries.join(", ")
                    ),
                    Err(_) => format!("`{path}` does not exist in the project"),
                }
            })?;
        // The bytes read, kept for the basis of a proposal that reads this file.
        let named = format!("./{}", parts.join("/"));
        kept(&self.verifier)?
            .read
            .insert(named, Witness::of(text.as_bytes()));
        let first = usize::try_from(offset.unwrap_or(1).max(1)).unwrap_or(usize::MAX) - 1;
        let count = limit.map_or(READ_LINES, |n| usize::try_from(n).unwrap_or(usize::MAX));
        let lines: Vec<&str> = text.lines().skip(first).take(count).collect();
        let total = text.lines().count();
        let window = crate::broker::redact(&lines.join("\n")).0;
        Ok(
            json!({"path": path, "first_line": first + 1, "lines": lines.len(),
            "total_lines": total, "text": window})
            .to_string(),
        )
    }

    fn parse(&mut self, source: &str) -> Result<(), String> {
        let file = nika_schema::FileId::new(0);
        (nika_schema::parse(source, file, nika_schema::ParseMode::Strict))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn report(&mut self, source: &str, inspect: bool) -> Result<String, String> {
        let options = AuditOptions::new(None, None);
        let audit =
            nika_cli_host::oracle::audit_source(source, "candidate.nika", None, None, options)
                .map_err(|e| format!("the candidate does not parse: {e}"))?;
        if inspect {
            let ids: Vec<&str> = (audit.wf.tasks.iter())
                .map(|t| t.value.id.value.as_str())
                .collect();
            let waves: Vec<Vec<&str>> = (audit.report.waves.iter())
                .map(|wave| wave.iter().filter_map(|k| ids.get(*k).copied()).collect())
                .collect();
            let nodes = document::nodes(source).unwrap_or(Value::Null);
            return Ok(page(&json!({"waves": waves, "nodes": nodes}).to_string()));
        }
        let lanes = Lanes::new(false, false);
        let (wf, report, skills, verdict) =
            (&audit.wf, &audit.report, &audit.skills, &audit.verdict);
        let json = nika_cli_host::oracle::audit_json(wf, report, skills, verdict, lanes)?;
        Ok(page(&Value::Object(json).to_string()))
    }

    fn explain(&mut self, code: &str) -> Result<String, String> {
        let out =
            nika_cli_host::explain::run_for(code, PLAIN, nika_cli_host::explain::Door::Oracle);
        if out.code == 0 {
            Ok(page(&out.text))
        } else {
            Err(out.text)
        }
    }

    fn language(&mut self, topic: &str, query: Option<&str>) -> Result<String, String> {
        let query = query.map(str::trim).filter(|q| !q.is_empty());
        match (topic, query) {
            ("schema", None) => Ok(page(nika_pack::schema_json())),
            ("schema", Some(query)) => {
                let schema: Value =
                    serde_json::from_str(nika_pack::schema_json()).map_err(|e| e.to_string())?;
                let mut found = Vec::new();
                named(&schema, query, "", &mut found);
                Ok(page(&json!({"query": query, "found": found}).to_string()))
            }
            ("catalog", _) => Ok(page(&nika_cli_host::catalog::run(true, PLAIN).text)),
            ("examples", None) => Ok(json!({"examples": nika_pack::example_slugs()}).to_string()),
            ("examples", Some(slug)) => {
                if let Some(body) = nika_pack::example(slug) {
                    return Ok(page(body));
                }
                let near: Vec<String> = (nika_pack::example_slugs().into_iter())
                    .filter(|s| s.contains(slug))
                    .collect();
                Ok(json!({"query": slug, "examples": near}).to_string())
            }
            ("template", None) => Ok(json!({"templates": nika_pack::template_names()}).to_string()),
            ("template", Some(name)) => nika_pack::template(name)
                .map(page)
                .ok_or_else(|| format!("no template `{name}`: ask `template` alone for the list")),
            (other, _) => Err(format!(
                "no language topic `{other}`: schema, catalog, examples or template"
            )),
        }
    }

    fn models(&mut self, role: &str, ask: &Value) -> Result<String, String> {
        use nika_providers::model_choice::{
            Delegation, ModelAsk, ModelChoice, ModelInventory, ModelOffer, ModelRole,
        };
        let role = match role {
            "run" => ModelRole::Run,
            "author" => ModelRole::Author,
            "decision" => ModelRole::Decision,
            other => return Err(format!("no model role `{other}`: run, author or decision")),
        };
        let offer = |o: &ModelOffer| {
            let mut offered = json!({"model": o.model, "via": o.route.access,
                "class": o.route.class.as_str(), "configured": o.route.configured,
                "fix": o.route.fix_var, "output_usd_per_million": o.output_usd_per_million});
            if let Some(caps) = model_caps(&o.model) {
                offered["reasoning_efforts"] = caps["reasoning_efforts"].clone();
                offered["max_output_tokens"] = caps["max_output_tokens"].clone();
            }
            offered
        };
        let offers = |list: &[ModelOffer]| list.iter().map(offer).collect::<Vec<Value>>();
        let inventory = ModelInventory::from_probes(&self.probes);
        let text = |key: &str| ask[key].as_str().map(str::to_owned);
        let mut asked = ModelAsk::new();
        asked.provider = text("provider");
        asked.model = text("model");
        asked.via = text("via");
        asked.protocol =
            (ask["protocol"].as_str()).and_then(nika_types::access::AccessProtocol::parse);
        asked.delegate =
            (ask["delegate"].as_str() == Some("strongest")).then_some(Delegation::Strongest);
        if [&asked.provider, &asked.model, &asked.via]
            .iter()
            .all(|f| f.is_none())
            && asked.protocol.is_none()
            && asked.delegate.is_none()
        {
            return Ok(json!({"offers": offers(inventory.offers(role))}).to_string());
        }
        let choice = match inventory.choose(role, &asked) {
            ModelChoice::Exact(chosen) => json!({"exact": offer(&chosen)}),
            ModelChoice::Delegated {
                chosen,
                rule,
                among,
            } => json!({"delegated": offer(&chosen), "rule": rule, "among": offers(&among)}),
            ModelChoice::Choose(among) => json!({"choose": offers(&among)}),
            ModelChoice::Unsupported { why, alternatives } => {
                json!({"unsupported": why, "alternatives": offers(&alternatives)})
            }
            other => json!({"unread": format!("{other:?}")}),
        };
        Ok(choice.to_string())
    }

    fn knowledge(&mut self, query: Option<&str>, skill: Option<&str>) -> Result<String, String> {
        let pin =
            (self.knowledge.as_ref()).ok_or("the knowledge release is off in this Session")?;
        let holdout = pin.exclude_corpus.clone();
        let release = json!({"version": pin.version, "digest": pin.digest});
        let snapshot = self.release()?;
        let words = skill.or(query).unwrap_or_default();
        let pack = (snapshot.pack(words, holdout.as_deref())).map_err(|e| e.to_string())?;
        let references: Vec<Value> = (pack.references.iter())
            .filter(|r| skill.is_none_or(|s| r.kind == "skill" && r.id.contains(s)))
            .map(|r| json!({"kind": r.kind, "id": r.id, "text": r.text}))
            .collect();
        Ok(page(
            &json!({"release": release, "references": references}).to_string(),
        ))
    }

    fn compose(&mut self, source: &str, operations: &[Value]) -> Result<String, String> {
        // The release the Session pinned, lent as executable components as a compile round lends
        // it, its held-out corpus out of reach: a pin that no longer reopens lends nothing.
        let holdout = (self.knowledge.as_ref()).and_then(|pin| pin.exclude_corpus.clone());
        let snapshot = self.release().ok();
        let catalogue = snapshot.map(|snapshot| snapshot.catalogue(holdout.as_deref()));
        let lent = (catalogue.as_ref()).map(|catalogue| catalogue as &dyn ComponentCatalog);
        (document::apply(source, (operations, None), lent, &[]))
            .map(|applied| applied.source)
            .map_err(|refused| refused.join("\n"))
    }

    fn verify(
        &mut self,
        source: &str,
        (stated, answers, original): (&str, &BTreeMap<String, String>, Option<&str>),
        selections: (&[Resolution], &[Resolution]),
        presentation: &Value,
        requested: Option<&str>,
    ) -> Result<Judged, String> {
        // A verdict stands only for the same bytes, words, selections and full presentation.
        let rows = |s: &[Resolution]| s.iter().map(Resolution::to_json).collect::<Vec<_>>();
        let (authored, host) = (rows(selections.0), rows(selections.1));
        let read = kept(&self.verifier)?.read.clone();
        let at = landing(&self.root, source, (self.saved.as_ref(), &read), requested)?;
        let presentation = &self.presented(presentation, &at, stated);
        let said = json!([
            source,
            stated,
            authored,
            host,
            answers,
            original,
            at,
            presentation
        ]);
        let key = blake3::hash(said.to_string().as_bytes())
            .to_hex()
            .to_string();
        let declined = {
            let mut kept = kept(&self.verifier)?;
            if let Some((done, judged, doubt)) = kept.ready.clone()
                && done == key
            {
                kept.doubt = doubt;
                return Ok(judged.cached());
            }
            kept.verdicts.clone()
        };
        // The engine's own check of these exact bytes comes before any judge: bytes it refuses
        // are never shown to the person (no Save, no Run is offered for them). A verdict is kept
        // only for bytes that passed it.
        let set = ProjectChangeSet::workflow_at(&self.root, "verify", &at, source.to_owned())
            .map_err(|e| e.to_string())?;
        let audit = set.audits.first().ok_or("the candidate was not audited")?;
        if !audit.clean {
            return Err(failing(audit));
        }
        // Tried where nothing leaves: the pages its public GET sources answer, observed once for
        // the conversation, lent to the observed room as a replay trial of these bytes.
        let verifier = &self.verifier;
        let (jq, observe) = &verifier.trial;
        let lent = (self.root.clone(), jq.clone());
        let Trial { scoped, words, .. } = {
            let mut kept = kept(verifier)?;
            crate::trial::prepare(source, stated, lent, &mut kept.observed, observe)
        };
        // The same verification a compile round of the request faces: the files its words name
        // observed, the laws, the trial, then the whole-request verdict; ready only on that
        // verdict, and never on a failed trial.
        let out = crate::authoring::verify_presented_in(
            &verifier.seat,
            &verifier.context,
            (stated, answers, original),
            (source, selections, presentation),
            declined,
            verifier.account.as_ref(),
            Some(&scoped),
        )
        .map_err(|e| e.to_string())?;
        if let Some(dir) = &self.judged {
            keep_judged(dir, source);
        }
        let (_, tried) = scoped.finish().into_parts();
        kept(verifier)?.keep(&out);
        if out.status != CompileStatus::Ready {
            return Err(findings(&out));
        }
        // A ready document whose trial came to no decision is never shown in silence: its
        // proposal says the rehearsal did not run, and why (live gauntlet wave 10, 2026-10-20:
        // the world before the trial was refused, the account dropped, and the proposal read
        // checked with no word of any rehearsal).
        let tried = tried.and_then(|tried| {
            tried.ok_or_else(|| "no rehearsal of these exact bytes came to its end".to_owned())
        });
        kept(verifier)?.tried = Some((tried, words));
        let reach = json!({"effects": audit.effects, "world": audit.world});
        let scope = blake3::hash(reach.to_string().as_bytes())
            .to_hex()
            .to_string();
        let doubt = stated_doubt(&out);
        let mut kept = kept(&self.verifier)?;
        let judged = Judged::ready(scope, source, &out);
        kept.ready = Some((key, judged.clone(), doubt.clone()));
        kept.doubt = doubt;
        Ok(judged)
    }

    fn judged_now(
        &mut self,
        source: &str,
        (stated, answers): (&str, &BTreeMap<String, String>),
        selections: (&[Resolution], &[Resolution]),
    ) -> Vec<String> {
        let options = AuditOptions::new(None, None);
        let audited =
            nika_cli_host::oracle::audit_source(source, "candidate.nika", None, None, options);
        let mut found: Vec<String> = match audited {
            Ok(audit) => (audit.report.findings.iter())
                .map(|f| format!("{}: {}", f.code.as_deref().unwrap_or(f.kind), f.message))
                .collect(),
            Err(e) => vec![format!("the candidate does not parse: {e}")],
        };
        let observed = nika_cli_host::compile::observe::world(&self.root, stated);
        let laws = nika_compile_cognition::judge_document_answered(
            stated,
            source,
            selections,
            answers,
            observed.as_ref(),
        );
        found.extend(laws);
        found
    }

    fn model_facts(&mut self, model: &str) -> Option<nika_session_change::work::ModelFacts> {
        nika_session_intelligence::reasoner::agent_model::model_facts(&self.probes, model)
    }
}

#[cfg(test)]
mod tests;
