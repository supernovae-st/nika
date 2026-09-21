// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a plan needs before any structure exists: the material it consumes, the
//! files it writes, the endpoints it posts to, the code rule it runs.
//!
//! Every value here is either copied from the intent (a path literal, a URL) or
//! answered by the human through a stable question. A path literal is a single
//! path-shaped token; prose never becomes a path, a directory is never one file,
//! a placeholder is a question. A verb whose target names a local file is a write
//! to that file, never a POST. A file the request names that nothing reads or
//! writes is a question, never a silent drop.

use super::paths::{self, PathShape, Structured};
use super::plan::{Effect, EffectPolicy, EffectVerb, Op, Plan, Step};
use super::rules;
use super::shape;
use super::support::{admit_directory, admit_endpoint, admit_model, admit_policy, answer, reject};
use super::{CompileOutcome, CompileRequest, DiagnosticKind, QuestionType};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const MODEL_LABEL: &str = "Which explicit runtime provider/model should run the language steps (extract, classify, draft)?";
const STATE_LABEL: &str = "Which JSON file keeps the identifiers already processed, so the same event never triggers a second action?";
const SEARCH_LABEL: &str = "Which local directory holds the documents to search?";
const URL_LABEL: &str = "Which exact URL should be fetched?";
const SOURCE_PATHS_LABEL: &str = "Which exact local files should be read? Answer a JSON array of file paths, one per file, without prose.";

/// One value a plan may need: not needed, asked and pending, or bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Need<T> {
    Absent,
    Pending,
    Bound(T),
}

impl<T> Need<T> {
    pub(super) const fn settled(&self) -> bool {
        !matches!(self, Self::Pending)
    }
    pub(super) const fn bound(&self) -> Option<&T> {
        match self {
            Self::Bound(value) => Some(value),
            Self::Absent | Self::Pending => None,
        }
    }
    fn from_step(step: Option<&Step>, bind: impl FnOnce(&Step) -> Option<T>) -> Self {
        match step {
            None => Self::Absent,
            Some(step) => bind(step).map_or(Self::Pending, Self::Bound),
        }
    }
}

/// Where the document comes from, once the read step is settled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Source {
    /// The document is the text supplied with each invocation.
    Item,
    File(String),
    Files(Vec<String>),
    Glob(String),
}

/// One endpoint effect whose bindings were all admitted.
pub(super) struct Wired {
    pub slug: String,
    pub gated: bool,
    pub endpoint: Value,
    pub host: String,
    pub policy: Option<Value>,
    pub verb: EffectVerb,
    pub target: String,
    /// The effect carries material the plan already holds, unchanged ("post it to
    /// `<url>`", "send the report to `<url>`"): a webhook message, not an action payload.
    pub carry: bool,
}

/// A lookup that selects one record by a literal identifier: the identifier and the
/// field that holds it, both constants of the workflow.
pub(super) struct ById {
    /// `const.<slug>_id`, the identifier verbatim.
    pub id_key: String,
    pub id: String,
    /// `const.<slug>_id_field`, the record field holding the identifier (answered).
    pub field_key: String,
    pub field: Value,
}

/// The settled lookup: its directory constant and file, and how the record is selected
/// (a literal identifier, or the `record_id` input of each invocation).
pub(super) struct Lookup {
    /// `const.<slug>_directory`.
    pub key: String,
    pub directory: Value,
    pub by_id: Option<ById>,
}

/// The code rule of a compute step: the jq expression the human answered, or the rule
/// synthesized from the words the request states over the parsed records.
pub(super) enum RuleBinding {
    Answered(Value),
    Synthesized(rules::Rule),
}

/// One local file the request writes.
pub(super) struct WriteEffect {
    pub stem: String,
    pub path: String,
    pub gated: bool,
    pub target: String,
    /// The one classify category the write's clause names ("the bugs to ./bugs.json"):
    /// the write carries the records routed to it.
    pub category: Option<String>,
    /// The facet of the fetched page the write's clause names ("the page title to
    /// ./title.txt"): the write carries the fetch's own mode, never a draft.
    pub facet: Option<super::network::Facet>,
}

/// The settled bindings of one plan.
pub(super) struct Bindings {
    pub model: Option<Value>,
    /// The directory file of a lookup and how its record is selected.
    pub lookup: Need<Lookup>,
    pub search: Need<Value>,
    pub fetch: Need<Value>,
    pub read: Need<Source>,
    pub rule: Need<RuleBinding>,
    pub dedup: Need<Value>,
    pub wired: Vec<Wired>,
    pub writes: Vec<WriteEffect>,
    pub effects_pending: bool,
    /// Constraints the structure consumed (a concurrency bound), kept out of prompts.
    pub consumed: Vec<String>,
    pub max_parallel: Option<u32>,
    /// Whether `inputs.item` is declared: the request is invoked per item, or it
    /// supplies no other material.
    pub item: bool,
    /// The operations that run once per read item of a fan-out and fold back in item
    /// order: a draft the request distributes over the files, an extract whose fields it
    /// scopes to each item.
    pub per_item: Vec<Op>,
    /// The classify runs once per parsed record of one structured source: the request
    /// classifies each record, and a write naming a category carries its records.
    pub classify_per_record: bool,
}

impl Bindings {
    /// The corpus is several files read in a bounded fan-out.
    pub(super) fn fan_out(&self) -> bool {
        matches!(self.read, Need::Bound(Source::Files(_) | Source::Glob(_)))
    }
    /// The draft runs once per read item and is folded back in item order.
    pub(super) fn draft_per_item(&self) -> bool {
        self.per_item.contains(&Op::Draft)
    }
    /// The extract runs once per read item and folds into one record per item.
    pub(super) fn extract_per_item(&self) -> bool {
        self.per_item.contains(&Op::Extract)
    }
    /// Whether at least one effect waits on a human gate.
    pub(super) fn gated(&self) -> bool {
        self.writes.iter().any(|w| w.gated) || self.wired.iter().any(|w| w.gated)
    }
    /// The rule the compiler synthesized from the request, when the compute step has one.
    fn synthesized(&self) -> Option<&rules::Rule> {
        match &self.rule {
            Need::Bound(RuleBinding::Synthesized(rule)) => Some(rule),
            _ => None,
        }
    }
    /// The rule joins several parsed sources on a column: each read file is parsed apart.
    pub(super) fn joins(&self) -> bool {
        self.synthesized().is_some_and(rules::Rule::joins)
    }
    /// The rule runs over the lines of a text source: the source is decoded into lines.
    pub(super) fn rule_over_lines(&self) -> bool {
        self.synthesized().is_some_and(rules::Rule::lines)
    }
    /// Whether a structured source must be decoded for code: a code rule, an endpoint
    /// payload or a structured write consumes the parsed records; a prompt never does.
    pub(super) fn parses(&self) -> bool {
        !matches!(self.rule, Need::Absent)
            || self.classify_per_record
            || !self.wired.is_empty()
            || self
                .writes
                .iter()
                .any(|w| Structured::of(&w.path).is_some())
    }
    pub(super) fn ready(&self, plan: &Plan) -> bool {
        (!uses_model(plan) || self.model.is_some())
            && self.lookup.settled()
            && self.search.settled()
            && self.fetch.settled()
            && self.read.settled()
            && self.rule.settled()
            && self.dedup.settled()
            && !self.effects_pending
    }
}

pub(super) fn uses_model(plan: &Plan) -> bool {
    plan.steps.iter().any(|s| {
        matches!(
            s.op,
            Op::Extract | Op::Classify | Op::Draft | Op::Validate | Op::Explore
        )
    })
}

fn source_slug(op: Op, detail: &str) -> String {
    let slug = super::lexicon::slug(detail);
    let base = if matches!(
        slug.as_str(),
        "client"
            | "clients"
            | "customer"
            | "customers"
            | "customer_record"
            | "customer_history"
            | "historique_client"
            | "historique"
    ) {
        "customer".to_owned()
    } else {
        slug
    };
    match op {
        Op::Lookup => format!("{base}_directory"),
        _ => base,
    }
}

fn effect_slug(effect: &Effect) -> String {
    match effect.verb {
        EffectVerb::Other => {
            let slug = super::lexicon::slug(&effect.target);
            if slug == "item" {
                "effect".to_owned()
            } else {
                slug
            }
        }
        verb => verb.word().to_owned(),
    }
}

/// A verb whose target names one local file is a write to that file, never a POST.
fn file_write(effect: &Effect) -> Option<String> {
    match effect.verb {
        EffectVerb::Write
        | EffectVerb::Merge
        | EffectVerb::Create
        | EffectVerb::Update
        | EffectVerb::Publish
        | EffectVerb::Other => paths::single_file(&effect.target),
        _ => None,
    }
}

/// A numeric concurrency bound stated as a constraint ("at most 2 at a time").
pub(super) fn parallel_bound(constraint: &str) -> Option<u32> {
    let lower = constraint.to_lowercase();
    let concurrent = [
        "at a time",
        "at once",
        "in parallel",
        "concurrently",
        "simultaneously",
        "à la fois",
        "en parallèle",
        "en même temps",
        "a la vez",
        "al mismo tiempo",
    ]
    .iter()
    .any(|phrase| lower.contains(phrase));
    if !concurrent {
        return None;
    }
    let numbers: Vec<u32> = lower
        .split(|c: char| !c.is_ascii_digit())
        .filter(|w| !w.is_empty())
        .filter_map(|w| w.parse().ok())
        .collect();
    match numbers.as_slice() {
        [n] if *n > 0 => Some(*n),
        _ => None,
    }
}

/// Every binding the plan needs, answered or asked. Sources first, because the
/// item and the rule's input shape depend on them.
pub(super) fn bind(
    plan: &Plan,
    intent: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
) -> Bindings {
    let model = if uses_model(plan) {
        recognized.insert("model".to_owned());
        answer(request, out, "model", MODEL_LABEL, true).and_then(|m| admit_model(out, m))
    } else {
        None
    };
    let lookup = Need::from_step(plan.step(Op::Lookup), |step| {
        resolve_lookup(step, request, out, recognized)
    });
    let search = Need::from_step(plan.step(Op::Search), |_| {
        recognized.insert("const.search_root".to_owned());
        answer(request, out, "const.search_root", SEARCH_LABEL, true)
    });
    let fetch = Need::from_step(plan.step(Op::Fetch), |_| {
        if let Some(url) = plan.bindings.iter().find(|b| b.role == "url") {
            Some(json!(url.literal))
        } else {
            recognized.insert("const.source_url".to_owned());
            answer(request, out, "const.source_url", URL_LABEL, true)
        }
    });
    let read = Need::from_step(plan.step(Op::Read), |step| {
        resolve_read(step, request, out, recognized)
    });
    let dedup = if plan.obligation("dedup") {
        recognized.insert("const.state_file".to_owned());
        answer(request, out, "const.state_file", STATE_LABEL, true)
            .map_or(Need::Pending, Need::Bound)
    } else {
        Need::Absent
    };
    let fan_out = matches!(read, Need::Bound(Source::Files(_) | Source::Glob(_)));
    // A literal lookup is material of its own: the record it selects is the corpus.
    let literal_lookup = matches!(&lookup, Need::Bound(l) if l.by_id.is_some());
    let has_corpus = matches!(read, Need::Bound(Source::File(_)))
        || fan_out
        || !matches!(fetch, Need::Absent)
        || literal_lookup;
    let item = !has_corpus
        || matches!(read, Need::Bound(Source::Item))
        || plan.has(Op::Search)
        || (plan.trigger.is_some() && !fan_out);
    let mut consumed = Vec::new();
    let mut max_parallel = None;
    if fan_out {
        for constraint in &plan.constraints {
            if let Some(bound) = parallel_bound(constraint) {
                consumed.push(constraint.clone());
                max_parallel = Some(bound);
                break;
            }
        }
    }
    // The request distributes its draft over the files: the fan-in realizes the order
    // and the headings itself, so those instructions leave the prompts.
    let distributed = shape::per_item(intent, plan);
    let mut per_item = Vec::new();
    if distributed && fan_out && plan.has(Op::Draft) {
        per_item.push(Op::Draft);
    }
    if fan_out && shape::per_item_extract(plan) {
        per_item.push(Op::Extract);
    }
    let classify_per_record = matches!(&read, Need::Bound(Source::File(path)) if Structured::of(path).is_some())
        && shape::per_record_classify(plan);
    if per_item.contains(&Op::Draft) {
        for constraint in &plan.constraints {
            if shape::structural(constraint) && !consumed.contains(constraint) {
                consumed.push(constraint.clone());
            }
        }
    }
    let mut b = Bindings {
        model,
        lookup,
        search,
        fetch,
        read,
        rule: Need::Absent,
        dedup,
        wired: Vec::new(),
        writes: Vec::new(),
        effects_pending: false,
        consumed,
        max_parallel,
        item,
        per_item,
        classify_per_record,
    };
    b.rule = Need::from_step(plan.step(Op::Compute), |step| {
        // A rule the request states over a parsed source is code the compiler writes;
        // an explicit answer still wins, and anything outside the grammar is asked.
        if !request.answers.contains_key("const.rule_expression")
            && let Some(rule) = synthesized_rule(plan, step, intent, &b)
        {
            return Some(RuleBinding::Synthesized(rule));
        }
        recognized.insert("const.rule_expression".to_owned());
        let label = rule_label(plan, &b, &step.detail);
        answer(request, out, "const.rule_expression", &label, true).map(RuleBinding::Answered)
    });
    bind_effects(plan, distributed, request, out, recognized, &mut b);
    bind_named_outputs(plan, request, out, recognized, &mut b);
    b
}

/// A per-item request whose written target is a placeholder ("./out/`<name>`.md") asks for
/// one file per item; the compiler emits one written file per request and never lowers
/// that to a single guessed path.
fn refuse_per_item_placeholder(effect: &Effect, out: &mut CompileOutcome) -> bool {
    let placeholder = paths::literals(&effect.target)
        .into_iter()
        .find_map(|shape| match shape {
            PathShape::Placeholder(path) => Some(path),
            _ => None,
        });
    let Some(placeholder) = placeholder else {
        return false;
    };
    super::finding(
        out,
        DiagnosticKind::Unknown,
        "intent",
        format!(
            "The request writes one file per item (`{placeholder}`), but a compiled workflow writes one file per request; a single guessed path would drop the per-item files. Name the one file the per-item results merge into, or one request per item."
        ),
    );
    super::question(
        out,
        "intent.clarification",
        "Supply a complete replacement request that names the one file receiving the per-item results, or one request per item. It explicitly replaces the earlier intent.",
        QuestionType::Text,
    );
    true
}

/// The lookup step settles its directory: a detail naming one JSON file and an
/// identifier binds the file itself and asks only which field holds the identifier (the
/// record is selected at run time); any other detail asks for the JSON directory file and
/// reads the record keyed by each invocation's `record_id`.
fn resolve_lookup(
    step: &Step,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
) -> Option<Lookup> {
    if let Some(literal) = shape::lookup_by_identifier(&step.detail) {
        let field_key = format!("const.{}_id_field", literal.slug);
        recognized.insert(field_key.clone());
        let label = format!(
            "Which field of each record in `{}` holds the identifier `{}` (for example `id`)? Answer the field name as a JSON string.",
            literal.file, literal.id
        );
        let field = answer(request, out, &field_key, &label, true)?;
        return Some(Lookup {
            key: format!("const.{}_directory", literal.slug),
            directory: json!(literal.file),
            by_id: Some(ById {
                id_key: format!("const.{}_id", literal.slug),
                id: literal.id,
                field_key,
                field,
            }),
        });
    }
    let key = format!("const.{}", source_slug(Op::Lookup, &step.detail));
    recognized.insert(key.clone());
    let label = format!(
        "Which JSON file maps record ids to the records for `{}`? No external system is connected by the compiler.",
        step.detail.trim()
    );
    let directory =
        answer(request, out, &key, &label, true).and_then(|v| admit_directory(out, v))?;
    Some(Lookup {
        key,
        directory,
        by_id: None,
    })
}

/// The read step settles where the document comes from: the supplied item when it
/// names no path, one file, several files, a glob; a directory or a placeholder is a
/// stable question, never a path literal.
fn resolve_read(
    step: &Step,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
) -> Option<Source> {
    let mut files = Vec::new();
    let mut globs = Vec::new();
    let mut directories = Vec::new();
    let mut placeholders = Vec::new();
    for shape in paths::literals(&step.detail) {
        match shape {
            PathShape::File(p) => files.push(p),
            PathShape::Glob(p) => globs.push(p),
            PathShape::Directory(p) => directories.push(p),
            PathShape::Placeholder(p) => placeholders.push(p),
        }
    }
    if files.is_empty() && globs.is_empty() && directories.is_empty() && placeholders.is_empty() {
        return Some(Source::Item);
    }
    if placeholders.is_empty() && directories.is_empty() {
        match (files.len(), globs.as_slice()) {
            (1, []) => return files.pop().map(Source::File),
            (2.., []) => return Some(Source::Files(files)),
            (0, [glob]) => return Some(Source::Glob(glob.clone())),
            _ => {}
        }
    }
    if placeholders.is_empty()
        && files.is_empty()
        && globs.is_empty()
        && let [directory] = directories.as_slice()
    {
        return resolve_directory(directory, request, out, recognized);
    }
    let key = "const.source_paths";
    recognized.insert(key.to_owned());
    let value = answer(request, out, key, SOURCE_PATHS_LABEL, false)?;
    let files: Option<Vec<String>> = value.as_array().and_then(|items| {
        items
            .iter()
            .map(|item| match item.as_str().and_then(paths::token) {
                Some(PathShape::File(file)) => Some(file),
                _ => None,
            })
            .collect()
    });
    match files {
        Some(mut files) if !files.is_empty() => Some(if files.len() == 1 {
            Source::File(files.remove(0))
        } else {
            Source::Files(files)
        }),
        _ => {
            reject(
                out,
                key,
                SOURCE_PATHS_LABEL,
                false,
                "Answer a nonempty JSON array of exact file paths; a directory, a glob or a placeholder is not one file.",
            );
            None
        }
    }
}

/// A directory is never read as one file: the human names the glob under it.
fn resolve_directory(
    directory: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
) -> Option<Source> {
    let key = "const.source_glob";
    recognized.insert(key.to_owned());
    let label = format!(
        "Which glob selects the files to read under `{directory}` (for example {}/*.md)? Each match is read as one document; a directory is never read as one file.",
        directory.trim_end_matches('/')
    );
    let value = answer(request, out, key, &label, true)?;
    match value.as_str().and_then(paths::token) {
        Some(PathShape::Glob(glob)) => Some(Source::Glob(glob)),
        Some(PathShape::File(file)) => Some(Source::File(file)),
        _ => {
            reject(
                out,
                key,
                &label,
                true,
                "Name a glob such as ./dir/*.md or one exact file; a bare directory or a placeholder is not readable.",
            );
            None
        }
    }
}

/// The rule a compute step states in words, when the corpus is what the rule can run over
/// and every part of the detail is in the grammar: one structured file for a filter, an
/// aggregate, a grouping, a sort, a top-N or a projection over its parsed records; one text
/// file for a removal of duplicate lines; several structured files of one format for a join.
fn synthesized_rule(plan: &Plan, step: &Step, intent: &str, b: &Bindings) -> Option<rules::Rule> {
    // A validated rule stated for this very step first (the semantic frontend's typed
    // predicate, or a promoted constraint: meaning before syntax), then the closed grammar
    // over the whole detail. A detail the plan joined from several clauses (` ; `) must
    // parse whole: one recorded rule for one of its parts would silently drop the others.
    let detail = step.detail.trim();
    let whole = !detail.contains(" ; ");
    // A rule recorded for this very step stands for it when it is the only rule (the seat's
    // paraphrase beside the promoted constraint of the same rule); two recorded rules on a
    // joined detail are synthesized whole, so neither stands for the other.
    let stated = plan
        .rules
        .iter()
        .find(|rule| rule.text() == step.evidence || rule.text() == detail)
        .filter(|_| whole || plan.rules.len() == 1)
        .cloned()
        .or_else(|| rules::synthesize(detail, &super::columns::columns_hint(intent)))
        .or_else(|| {
            (whole && plan.rules.len() == 1)
                .then(|| plan.rules.first().cloned())
                .flatten()
        })?;
    match &b.read {
        Need::Bound(Source::File(path)) if Structured::of(path).is_some() => {
            (!stated.joins()).then_some(stated)
        }
        Need::Bound(Source::File(_)) => stated.over_lines(),
        Need::Bound(Source::Files(files)) if joined_format(files).is_some() => {
            stated.joins().then_some(stated)
        }
        _ => None,
    }
}

/// The one structured format several read files share, when they do: what a join parses,
/// one array of records per file.
pub(super) fn joined_format(files: &[String]) -> Option<Structured> {
    let mut formats = files.iter().map(|file| Structured::of(file));
    let first = formats.next()??;
    formats.all(|format| format == Some(first)).then_some(first)
}

/// The input object a code rule will receive, named fact by fact, so the question
/// describes exactly what the answer runs over.
fn rule_label(plan: &Plan, b: &Bindings, detail: &str) -> String {
    let mut shape: Vec<(&str, String)> = Vec::new();
    if b.item {
        shape.push(("item", "the incoming item text".to_owned()));
    }
    if !matches!(b.lookup, Need::Absent) {
        shape.push(("record", "the looked-up record".to_owned()));
    }
    match &b.read {
        Need::Bound(Source::File(path)) => {
            shape.push(("document", format!("the raw text of {path}")));
            if let Some(format) = Structured::of(path) {
                shape.push(("records", parsed_about(path, format)));
            }
        }
        Need::Bound(Source::Files(_) | Source::Glob(_)) => shape.push((
            "document",
            "the texts of the read files, each under a `## <path>` heading".to_owned(),
        )),
        Need::Pending => shape.push(("document", "the raw text of the read file".to_owned())),
        Need::Bound(Source::Item) | Need::Absent => {}
    }
    if !matches!(b.search, Need::Absent) {
        shape.push(("hits", "the grep hits over the search root".to_owned()));
    }
    if !matches!(b.fetch, Need::Absent) {
        shape.push(("page", "the fetched page text".to_owned()));
    }
    for step in &plan.steps {
        match step.op {
            Op::Compute => break,
            Op::Extract => shape.push((
                "fields",
                "the extracted {name, value, anchor} array".to_owned(),
            )),
            Op::Classify => shape.push(("category", "the chosen category".to_owned())),
            Op::Validate => shape.push(("validation", "the {valid, issues} verdict".to_owned())),
            Op::Draft => shape.push(("draft", "the drafted text".to_owned())),
            Op::Explore => shape.push(("exploration", "the agent's final answer".to_owned())),
            Op::Read | Op::Fetch | Op::Lookup | Op::Search => {}
        }
    }
    let names = shape
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    let about = shape
        .iter()
        .map(|(name, about)| format!("`.{name}` is {about}"))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "Which jq expression implements `{}` over the input object {{{names}}}? {about}. It runs as code; a model never decides it.",
        detail.trim()
    )
}

pub(super) fn parsed_about(path: &str, format: Structured) -> String {
    match format {
        Structured::Json => format!("{path} decoded from JSON"),
        Structured::Csv => format!("the rows of {path} as an array of objects keyed by header"),
        Structured::Yaml | Structured::Toml => format!("{path} decoded as JSON"),
    }
}

/// Whether an effect is wanted: prohibited ones are omitted, undecided ones asked.
fn wanted(
    effect: &Effect,
    slug: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
) -> Option<bool> {
    match effect.policy {
        EffectPolicy::Forbidden => {
            super::finding(
                out,
                DiagnosticKind::Applied,
                slug,
                format!(
                    "Prohibited effect omitted by instruction: {}",
                    effect.evidence
                ),
            );
            Some(false)
        }
        EffectPolicy::Conflict => Some(false),
        EffectPolicy::Automatic | EffectPolicy::HumanFirst => Some(true),
        EffectPolicy::Undecided => {
            let key = format!("effect.{slug}.include");
            recognized.insert(key.clone());
            let label = format!(
                "Include this effect the request leaves undecided: `{}`? Answer true or false.",
                effect.target.trim()
            );
            match answer(request, out, &key, &label, false) {
                Some(Value::Bool(true)) => Some(true),
                Some(Value::Bool(false)) => {
                    super::finding(
                        out,
                        DiagnosticKind::Applied,
                        slug,
                        "Undecided effect excluded by explicit answer.",
                    );
                    Some(false)
                }
                Some(_) => {
                    super::finding(out, DiagnosticKind::Missed, &key, "Answer true or false.");
                    super::question(
                        out,
                        &key,
                        "Include this undecided effect? Answer true or false.",
                        QuestionType::Literal,
                    );
                    None
                }
                None => None,
            }
        }
    }
}

/// Effects: a local file target is a write task; anything else is a POST to an
/// explicit endpoint. Policy decides whether a human gate dominates it.
fn bind_effects(
    plan: &Plan,
    distributed: bool,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
    b: &mut Bindings,
) {
    for effect in &plan.effects {
        let slug = effect_slug(effect);
        match wanted(effect, &slug, request, out, recognized) {
            Some(true) => {}
            Some(false) => continue,
            None => {
                b.effects_pending = true;
                continue;
            }
        }
        let gated = effect.policy == EffectPolicy::HumanFirst;
        if effect.verb == EffectVerb::Write || file_write(effect).is_some() {
            if distributed
                && file_write(effect).is_none()
                && refuse_per_item_placeholder(effect, out)
            {
                b.effects_pending = true;
                continue;
            }
            let path = file_write(effect)
                .or_else(|| ask_write_path(effect, &slug, request, out, recognized, b));
            let Some(path) = path else {
                b.effects_pending = true;
                continue;
            };
            if let Some(existing) = b.writes.iter_mut().find(|w| w.path == path) {
                existing.gated |= gated;
                continue;
            }
            // The clause's prose names the category ("the bugs to ./bugs.json"), else the
            // file's own name does ("them to ./bugs.json and ./features.json"): a stated
            // literal, never the other file's name riding the same excerpt.
            let prose = effect
                .evidence
                .split_whitespace()
                .filter(|word| paths::token(word).is_none())
                .collect::<Vec<_>>()
                .join(" ");
            let category = plan.step(Op::Classify).and_then(|s| {
                category_named(&s.categories, &prose)
                    .or_else(|| category_named(&s.categories, &paths::stem(&path)))
            });
            let facet = plan
                .has(Op::Fetch)
                .then(|| super::network::write_facet(&effect.evidence, &path))
                .flatten();
            b.writes.push(WriteEffect {
                stem: paths::stem(&path),
                path,
                gated,
                target: effect.target.clone(),
                category,
                facet,
            });
            continue;
        }
        match bind_endpoint(effect, &slug, request, out, recognized) {
            Some((endpoint, host, policy)) => b.wired.push(Wired {
                slug,
                gated,
                endpoint,
                host,
                policy,
                verb: effect.verb,
                target: effect.target.clone(),
                carry: super::network::carries(effect, plan),
            }),
            None => b.effects_pending = true,
        }
    }
}

/// The one category of a classify step that a write's clause names ("the bugs to
/// ./bugs.json" names `bug`), singular or plural; none when the clause names none or
/// several of them.
fn category_named(categories: &[String], text: &str) -> Option<String> {
    let folded = shape::fold(text);
    let words: Vec<&str> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let named: Vec<&String> = categories
        .iter()
        .filter(|category| {
            let c = shape::fold(category);
            words.iter().any(|w| {
                *w == c
                    || w.strip_suffix('s') == Some(c.as_str())
                    || w.strip_suffix("es") == Some(c.as_str())
            })
        })
        .collect();
    match named.as_slice() {
        [only] => Some((*only).clone()),
        _ => None,
    }
}

/// A write whose target names no single file asks for its exact path.
fn ask_write_path(
    effect: &Effect,
    slug: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
    b: &Bindings,
) -> Option<String> {
    let key = if b.writes.is_empty() {
        "const.output_path".to_owned()
    } else {
        format!("const.{slug}_path")
    };
    recognized.insert(key.clone());
    let label = format!(
        "Which exact file path should receive `{}`? One path-shaped token (for example ./out/result.md), no prose; a directory is not a file.",
        effect.target.trim()
    );
    match answer(request, out, &key, &label, true).and_then(|v| v.as_str().and_then(paths::token)) {
        Some(PathShape::File(path)) => Some(path),
        Some(_) => {
            reject(
                out,
                &key,
                &label,
                true,
                "Name one exact file, not a directory, a glob or a placeholder.",
            );
            None
        }
        None => None,
    }
}

/// The first http(s) URL a phrase names, trailing punctuation stripped.
fn literal_url(text: &str) -> Option<String> {
    text.split_whitespace()
        .map(|word| word.trim_end_matches(['.', ',', ';', ')', ']', '"', '\'', '>']))
        .map(|word| word.trim_start_matches(['(', '[', '"', '\'', '<']))
        .find(|word| word.starts_with("http://") || word.starts_with("https://"))
        .map(str::to_owned)
}

/// An explicit endpoint, its host, and literal policy data when money moves.
fn bind_endpoint(
    effect: &Effect,
    slug: &str,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
) -> Option<(Value, String, Option<Value>)> {
    let endpoint_key = format!("const.{slug}_endpoint");
    recognized.insert(endpoint_key.clone());
    let label = format!(
        "Which HTTPS endpoint accepts a JSON POST to `{}`? No credentials or permission are inferred.",
        effect.target.trim()
    );
    // A URL the request itself names is the endpoint; the question is asked only when the
    // request names none (an answer still wins when the operator supplies one).
    let literal = literal_url(&effect.target).or_else(|| literal_url(&effect.evidence));
    let endpoint = match literal {
        Some(url) if !request.answers.contains_key(&endpoint_key) => Some(Value::String(url)),
        _ => answer(request, out, &endpoint_key, &label, true),
    };
    let policy = if effect.verb.moves_money() {
        let key = format!("const.{slug}_policy");
        recognized.insert(key.clone());
        if let Some(literal) = &effect.policy_literal {
            if request.answers.contains_key(&key) {
                answer(
                    request,
                    out,
                    &key,
                    "Supply explicit policy data, not approval.",
                    false,
                )
                .and_then(|p| admit_policy(out, p))
            } else {
                Some(Value::String(literal.clone()))
            }
        } else {
            let value = answer(
                request,
                out,
                &key,
                &format!(
                    "What cap, currency and eligibility criteria must apply before `{}`? Supply literal policy data; this is not approval.",
                    effect.target.trim()
                ),
                false,
            );
            if let Some(q) = out.questions.iter_mut().find(|q| q.key == key) {
                "Business policy is never invented by a model or the compiler; an authoring answer does not approve a runtime proposal.".clone_into(&mut q.why);
            }
            value.and_then(|p| admit_policy(out, p))
        }
    } else {
        None
    };
    let endpoint = endpoint?;
    let host = admit_endpoint(out, &endpoint_key, &label, &endpoint)?;
    if effect.verb.moves_money() && policy.is_none() {
        return None;
    }
    Some((endpoint, host, policy))
}

/// A file the request names that nothing reads or writes is never dropped in
/// silence: when the plan writes files, the human decides whether it is one more.
fn bind_named_outputs(
    plan: &Plan,
    request: &CompileRequest,
    out: &mut CompileOutcome,
    recognized: &mut BTreeSet<String>,
    b: &mut Bindings,
) {
    if b.writes.is_empty() && !plan.effects.iter().any(|e| e.verb == EffectVerb::Write) {
        return;
    }
    let mut seen: BTreeSet<String> = b.writes.iter().map(|w| w.path.clone()).collect();
    for binding in plan.bindings.iter().filter(|b| b.role == "path") {
        let Some(PathShape::File(path)) = paths::token(&binding.literal) else {
            continue;
        };
        if !seen.insert(path.clone()) {
            continue;
        }
        let consumed = plan.steps.iter().any(|s| s.detail.contains(&path))
            || plan.effects.iter().any(|e| e.target.contains(&path));
        if consumed {
            continue;
        }
        let stem = paths::stem(&path);
        let key = format!("effect.write_{stem}.include");
        recognized.insert(key.clone());
        let what = if Structured::of(&path).is_some() {
            "latest computed or extracted result"
        } else {
            "latest drafted text"
        };
        let label = format!(
            "The request names `{path}` but no recognized effect writes it. Write the {what} there as one more file? Answer true or false."
        );
        match answer(request, out, &key, &label, false) {
            Some(Value::Bool(true)) => b.writes.push(WriteEffect {
                stem,
                target: path.clone(),
                path,
                gated: false,
                category: None,
                facet: None,
            }),
            Some(Value::Bool(false)) => super::finding(
                out,
                DiagnosticKind::Applied,
                &key,
                format!("`{path}` is not written, by explicit answer."),
            ),
            Some(_) => {
                super::finding(out, DiagnosticKind::Missed, &key, "Answer true or false.");
                super::question(out, &key, &label, QuestionType::Literal);
                b.effects_pending = true;
            }
            None => b.effects_pending = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_concurrency_bound_is_one_number_next_to_a_concurrency_phrase() {
        assert_eq!(
            parallel_bound("Process at most 2 products at a time"),
            Some(2)
        );
        assert_eq!(parallel_bound("traite 3 fichiers à la fois"), Some(3));
        assert_eq!(parallel_bound("Process at most 2 products"), None);
        assert_eq!(parallel_bound("2 of the 4 at a time"), None);
        assert_eq!(parallel_bound("one at a time"), None);
    }
}
