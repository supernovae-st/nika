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
use nika_compile_seats::foundry::document;
use nika_fs::OwnedDir;
use nika_onboard::compile::{CompileOutcome, CompileStatus};
use nika_onboard::knowledge::pin::KnowledgePin;
use nika_providers::InferenceAdmission;
use nika_providers::probe::ProviderProbe;
use serde_json::{Value, json};

use crate::authoring::{AuthoringContext, AuthoringSeat};

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
    pub(crate) probes: Vec<ProviderProbe>,
    pub(crate) knowledge: Option<KnowledgePin>,
    pub(crate) verifier: Verifier,
}

/// The judge a conversation's document faces: the seat and decision model a compile round of
/// the same request would face, the account the line's dispatch admitted, and what the
/// conversation's verifications kept.
pub(crate) struct Verifier {
    pub(crate) seat: AuthoringSeat,
    pub(crate) context: AuthoringContext,
    pub(crate) account: Option<InferenceAdmission>,
    pub(crate) kept: Arc<Mutex<Verified>>,
}

/// What a conversation's verifications keep across its lines: every verdict that declined its
/// bytes (a judge is never asked again on bytes it rejected) and the last verdict that made a
/// document ready (its key and scope), so `propose` after `verify` asks nothing.
#[derive(Default)]
pub(crate) struct Verified {
    verdicts: Vec<Value>,
    ready: Option<(String, String)>,
    /// The project files the author read through the desk, by the path a workflow names, each
    /// with the witness of the bytes it read: what its proposal's basis keeps.
    read: BTreeMap<String, Witness>,
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

    /// Keep every verdict `out` recorded that declined its bytes, once.
    fn keep(&mut self, out: &CompileOutcome) {
        let attempts = (out.provenance.decision.as_ref())
            .and_then(|decision| decision["semantic_verification"].as_array());
        for attempt in attempts.into_iter().flatten() {
            if attempt["declined"] == true && !self.verdicts.contains(attempt) {
                self.verdicts.push(attempt.clone());
            }
        }
    }
}

/// What the author repairs from when a document is not ready: each finding of the verification
/// (the laws' refusals, the verifier's defects, doubts and held words) and each open question.
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
        let Some((name, dirs)) = parts
            .split_last()
            .filter(|_| !path.starts_with('/') && !hidden)
        else {
            return Err(format!(
                "`{path}` is not a visible file inside the project: name it relative to the root"
            ));
        };
        let root = OwnedDir::open(&self.root).map_err(|e| e.to_string())?;
        let over = format!("`{path}` is larger than {READ_CAP} bytes");
        let text = (root.read_capped_below(dirs, name, READ_CAP, &over))
            .map_err(|e| format!("`{path}` cannot be read: {e}"))?
            .ok_or_else(|| format!("`{path}` does not exist in the project"))?;
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
            json!({"model": o.model, "via": o.route.access, "class": o.route.class.as_str(),
                "configured": o.route.configured, "fix": o.route.fix_var,
                "output_usd_per_million": o.output_usd_per_million})
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
        let snapshot = pin.reopen().map_err(|e| e.to_string())?;
        let words = skill.or(query).unwrap_or_default();
        let pack =
            (snapshot.pack(words, pin.exclude_corpus.as_deref())).map_err(|e| e.to_string())?;
        let references: Vec<Value> = (pack.references.iter())
            .filter(|r| skill.is_none_or(|s| r.kind == "skill" && r.id.contains(s)))
            .map(|r| json!({"kind": r.kind, "id": r.id, "text": r.text}))
            .collect();
        let release = json!({"version": pin.version, "digest": pin.digest});
        Ok(page(
            &json!({"release": release, "references": references}).to_string(),
        ))
    }

    fn observe(&mut self, url: &str) -> Result<String, String> {
        nika_session_intelligence::observe::observe(url).map(|seen| seen.to_string())
    }

    fn compose(&mut self, source: &str, operations: &[Value]) -> Result<String, String> {
        (document::apply(source, (operations, None), None, &[]))
            .map(|applied| applied.source)
            .map_err(|refused| refused.join("\n"))
    }

    fn verify(
        &mut self,
        source: &str,
        stated: &str,
        selections: (&[Resolution], &[Resolution]),
    ) -> Result<String, String> {
        // The same bytes against the same words and selections: the verdict stands.
        let rows = |s: &[Resolution]| s.iter().map(Resolution::to_json).collect::<Vec<_>>();
        let said = json!([source, stated, rows(selections.0), rows(selections.1)]);
        let key = blake3::hash(said.to_string().as_bytes())
            .to_hex()
            .to_string();
        let declined = {
            let kept = kept(&self.verifier)?;
            if let Some((done, scope)) = &kept.ready
                && *done == key
            {
                return Ok(scope.clone());
            }
            kept.verdicts.clone()
        };
        // The same verification a compile round of the request faces: the files its words name
        // observed, the laws, then the whole-request verdict; ready only on that verdict.
        let verifier = &self.verifier;
        let out = crate::authoring::verify_in(
            &verifier.seat,
            &verifier.context,
            stated,
            (source, selections),
            declined,
            verifier.account.as_ref(),
            None,
        )
        .map_err(|e| e.to_string())?;
        kept(verifier)?.keep(&out);
        if out.status != CompileStatus::Ready {
            return Err(findings(&out));
        }
        let path = (crate::review::destination(&self.root, source))
            .ok_or("the candidate has no representable destination in the project")?;
        let at = path.display().to_string();
        let set = ProjectChangeSet::workflow_at(&self.root, "verify", &at, source.to_owned())
            .map_err(|e| e.to_string())?;
        let audit = set.audits.first().ok_or("the candidate was not audited")?;
        let reach = json!({"effects": audit.effects, "world": audit.world});
        let scope = blake3::hash(reach.to_string().as_bytes())
            .to_hex()
            .to_string();
        kept(&self.verifier)?.ready = Some((key, scope.clone()));
        Ok(scope)
    }

    fn trial(&mut self, _source: &str) -> Result<String, String> {
        Err("the rehearsal room is not open to a conversation's candidate yet: check and verify judge it".to_owned())
    }

    fn model_facts(&mut self, model: &str) -> Option<nika_session_change::work::ModelFacts> {
        nika_session_intelligence::reasoner::agent_model::model_facts(&self.probes, model)
    }
}
