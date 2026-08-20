// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The intent layer of `nika new <plain words>` (P0-1 · P0-10 ·
//! audit UX 2026-07-30): before ANY template is chosen, the utterance is
//! read into an [`IntentContract`] — what the human asked for, which
//! capabilities that implies, what it forbids. The contract is extracted
//! DETERMINISTICALLY (a lexicon · zero LLM — the same floor discipline as
//! the BM25 routing it feeds) and the routing decision is taken AGAINST
//! it: a candidate template must carry every required capability, the top
//! score must clear an absolute floor, and it must outscore the runner-up
//! by a relative margin. Below that bar the honest answer is
//! [`RoutingOutcome::NeedsClarification`] — never a silent guess.
//!
//! Extraction limits (documented, by design): the lexicon knows a closed
//! vocabulary of sources, transforms, outputs and constraints (English +
//! French). An utterance outside that vocabulary simply yields emptier
//! contract fields — the contract says what it KNOWS, never more, and an
//! empty contract still routes on BM25 evidence alone. Accented French
//! words shatter on the ascii tokenizer (« réunion » → « r » · « union »),
//! so those entries are matched as phrases on the lowered utterance.

use nika_bm25::{BmIndex, BmParams};

/// The absolute confidence floor (P0-10), calibrated 2026-07-31 against
/// the 10 audit semantic cases + the pre-existing routing tests: the
/// highest score a WRONG confident top match reaches is 2.033 (« relance
/// les factures impayées… » → docker-report — the P0-1 reproduction);
/// the lowest score of an honest route kept green is 5.479 (« scrape a
/// website and summarize » → website-brief). 3.0 sits between. Governs
/// the SKELETON corpus (the wizard's world — scores are corpus-relative).
const TAU: f64 = 3.0;

/// The floor for the WHOLE-catalog corpus (49 docs — BM25 scores are
/// IDF-relative, so each corpus earns its own floor). Measured
/// 2026-07-31 with the `calibration_probe` (run it after any corpus
/// change): the highest vague-utterance top score is 4.949 (« make it
/// good » → etl-quarantine) once the contract gate has disqualified the
/// capability mismatches (« please answer my emails » tops at
/// model-bench 6.201 on raw rank, but the utterance requires Net and
/// model-bench carries none); the lowest honest route is 6.220
/// (« chase unpaid invoices every friday » → invoice-chaser). 5.5 sits
/// between with slack both ways.
const TAU_CATALOG: f64 = 5.5;

/// The relative margin: the winner must outscore the runner-up by 1.3×.
/// Below it the two best skeletons are a coin-flip (« compare three
/// competitors… » scores media-asset-pack 4.014 vs website-brief 3.988 ·
/// ratio 1.007) and the honest answer is to name both.
const MARGIN: f64 = 1.3;

/// Everyday intent words → the Nika vocabulary the template bodies
/// actually carry. Query-side only (documents are never expanded) — the
/// evidenced cheap recall upgrade at tiny corpus scale, in place of
/// embeddings (BM25 stays the ranker).
pub(crate) const ALIASES: &[(&str, &[&str])] = &[
    ("scrape", &["fetch", "read"]),
    ("crawl", &["fetch"]),
    ("download", &["fetch"]),
    ("http", &["fetch"]),
    ("url", &["fetch"]),
    ("website", &["fetch"]),
    ("page", &["fetch"]),
    ("api", &["fetch", "invoke"]),
    ("llm", &["infer"]),
    ("ai", &["infer"]),
    ("model", &["infer"]),
    ("prompt", &["infer"]),
    ("summarize", &["infer", "think"]),
    ("classify", &["infer"]),
    ("generate", &["infer"]),
    ("save", &["write", "persist", "state"]),
    ("shell", &["exec"]),
    ("command", &["exec"]),
    ("script", &["exec"]),
    ("build", &["exec"]),
    ("test", &["exec", "verify"]),
    ("deploy", &["ship", "act", "exec"]),
    ("release", &["ship", "act", "changelog"]),
    ("parallel", &["for_each", "fan", "merge"]),
    ("concurrent", &["for_each", "fan"]),
    ("batch", &["for_each", "items"]),
    ("each", &["for_each", "items"]),
    ("every", &["for_each", "items"]),
    ("loop", &["agent", "for_each"]),
    ("iterate", &["agent", "for_each"]),
    ("agentic", &["agent"]),
    ("autonomous", &["agent", "budgeted"]),
    ("review", &["gate", "verify"]),
    ("approve", &["gate", "human"]),
    ("approval", &["gate", "human"]),
    ("confirm", &["gate", "human"]),
    ("pipeline", &["chain", "gather", "think"]),
    ("sequence", &["chain"]),
    ("transform", &["jq", "process"]),
    ("json", &["jq"]),
    ("state", &["state", "diff", "delta"]),
    ("incremental", &["state", "diff", "delta"]),
];

/// Function words + Nika envelope keywords that carry zero routing signal —
/// stripped from the query so an all-boilerplate `--from` (`the` · `workflow`
/// · `template`) lists the set instead of spuriously routing (every template
/// shares `workflow:`/`tasks:`/… so those terms separate nothing).
pub(crate) const STOPWORDS: &[&str] = &[
    "a", "an", "and", "the", "to", "of", "in", "on", "for", "with", "that", "this", "then", "than",
    "into", "from", "by", "as", "at", "is", "are", "be", "it", "its", "or", "i", "me", "my", "we",
    "you", "no", "such", "nika", "workflow", "model", "vars", "tasks", "id", "template", "slot",
    "kebab", "case",
];

/// A capability a template can carry — or an utterance can demand. The
/// vocabulary is closed on purpose: every variant is DERIVABLE from a
/// template body (see [`template_capabilities`]) or from the extraction
/// lexicon, so neither side can drift into a value the other can't speak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capability {
    /// Reads the filesystem (`nika:read` · `nika:glob`).
    FsRead,
    /// Writes the filesystem (`nika:write`).
    FsWrite,
    /// Reaches the network (`nika:fetch` · a `net:` permit).
    Net,
    /// Launches a program (`exec:`).
    Exec,
    /// Runs a model (`infer:` · `agent:`).
    Llm,
    /// Asks a human (`nika:prompt`).
    HumanGate,
    /// Fans out over a collection (`for_each:`).
    FanOut,
    /// Touches a container runtime (`docker` in the body).
    Container,
    /// Ships to production — the thing an utterance can FORBID
    /// (« sans déployer »). No embedded template declares it in its
    /// meaningful body yet (the word lives in comments, which the
    /// derivation strips), so the variant is contract-side data today.
    Deploy,
}

impl Capability {
    /// The stable wire spelling (contract JSON · messages).
    // test-pinned: the variant wire form ships with `to_json` (the
    // contract's serializable surface, P0-1) — pinned by the ratchet
    // tests today, un-gated the day a prod display consumes it.
    #[cfg(test)]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::FsRead => "fs-read",
            Self::FsWrite => "fs-write",
            Self::Net => "net",
            Self::Exec => "exec",
            Self::Llm => "llm",
            Self::HumanGate => "human-gate",
            Self::FanOut => "fan-out",
            Self::Container => "container",
            Self::Deploy => "deploy",
        }
    }
}

/// What the utterance commits to — extracted BEFORE any template choice
/// (P0-1). Every field is a list of what the lexicon RECOGNIZED; an empty
/// field is « not said », never « guessed ».
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IntentContract {
    /// The utterance, verbatim.
    pub(crate) goal: String,
    /// Recognized inputs (« support-tickets » · « git » · « logs »…).
    pub(crate) sources: Vec<String>,
    /// Recognized transforms (« prioritize » · « translate » · « resize »…).
    pub(crate) transformations: Vec<String>,
    /// The named artifact, when one is named (« report » · « manifest » ·
    /// a literal `RELEASE.md`).
    pub(crate) output: Option<String>,
    /// Recognized constraints (« cadence:every friday » ·
    /// « human-approval » · « no-deploy »).
    pub(crate) constraints: Vec<String>,
    /// Capabilities the chosen template MUST carry (a document output →
    /// [`Capability::FsWrite`] · « email » → [`Capability::Net`] ·
    /// « validation humaine » → [`Capability::HumanGate`]).
    pub(crate) required_capabilities: Vec<Capability>,
    /// Capabilities the chosen template must NOT carry (« sans déployer »
    /// → [`Capability::Deploy`]).
    pub(crate) forbidden_capabilities: Vec<Capability>,
    /// Derived done-statements (one per recognized element — « output
    /// `report` produced » · « runs on cadence:every friday »).
    pub(crate) success_criteria: Vec<String>,
}

impl IntentContract {
    /// The contract as JSON (`serde_json` is already a crate dependency —
    /// serialization rides it instead of adding a serde-derive edge).
    // test-pinned: the P0-1 deliverable (« a serializable
    // IntentContract ») — the surface the next wave's displays consume;
    // pinned by the serialization ratchet, un-gated when they arrive.
    #[cfg(test)]
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "goal": self.goal,
            "sources": self.sources,
            "transformations": self.transformations,
            "output": self.output,
            "constraints": self.constraints,
            "required_capabilities": self.required_capabilities.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
            "forbidden_capabilities": self.forbidden_capabilities.iter().map(|c| c.as_str()).collect::<Vec<_>>(),
            "success_criteria": self.success_criteria,
        })
    }
}

/// The routing verdict. Both variants carry the [`IntentContract`] — the
/// intention is on record either way.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RoutingOutcome {
    /// The bar was cleared: instantiate this template — as a DRAFT.
    Routed {
        /// The winning template name.
        template: String,
        /// Its BM25 score (the evidence the win rests on).
        score: f64,
        /// The contract extracted before the choice.
        contract: IntentContract,
    },
    /// Below the bar: name the closest skeletons, write NOTHING.
    NeedsClarification {
        /// The closest template names (empty on zero evidence).
        candidates: Vec<String>,
        /// The contract extracted before the choice.
        contract: IntentContract,
    },
}

/// The capabilities a template ACTUALLY carries, derived from its
/// meaningful body (comments stripped — the same surface the BM25 index
/// sees). Derived, never hand-listed: a template that gains a tool gains
/// the capability with no table to update.
fn template_capabilities(body: &str) -> Vec<Capability> {
    let meaningful = body
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .map(|l| l.split_once(" #").map_or(l, |(before, _)| before))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    let mut caps = Vec::new();
    if meaningful.contains("nika:read") || meaningful.contains("nika:glob") {
        caps.push(Capability::FsRead);
    }
    if meaningful.contains("nika:write") {
        caps.push(Capability::FsWrite);
    }
    if meaningful.contains("nika:fetch") || meaningful.contains("net:") {
        caps.push(Capability::Net);
    }
    if meaningful.contains("exec:") {
        caps.push(Capability::Exec);
    }
    if meaningful.contains("infer:") || meaningful.contains("agent:") {
        caps.push(Capability::Llm);
    }
    if meaningful.contains("nika:prompt") {
        caps.push(Capability::HumanGate);
    }
    if meaningful.contains("for_each:") {
        caps.push(Capability::FanOut);
    }
    if meaningful.contains("docker") {
        caps.push(Capability::Container);
    }
    caps
}

/// The extraction lexicon — a closed vocabulary, English + French.
/// Sources: what the workflow reads.
const SOURCE_LEXICON: &[(&str, &str)] = &[
    ("tickets", "support-tickets"),
    ("ticket", "support-tickets"),
    ("crm", "crm"),
    ("leads", "crm-leads"),
    ("prospects", "prospects"),
    ("factures", "invoices"),
    ("facture", "invoices"),
    ("invoices", "invoices"),
    ("invoice", "invoices"),
    ("git", "git"),
    ("commits", "git-commits"),
    ("logs", "logs"),
    ("alerte", "alerts"),
    ("alert", "alerts"),
    ("png", "png-images"),
    ("images", "images"),
    ("markdown", "markdown"),
    ("transcription", "transcript"),
    ("competitors", "competitors"),
    ("seo", "seo"),
    ("pages", "pages"),
];

/// Transforms: what the workflow does to what it reads.
const TRANSFORM_LEXICON: &[(&str, &str)] = &[
    ("prioritized", "prioritize"),
    ("prioritaires", "prioritize"),
    ("enrichis", "enrich"),
    ("enrich", "enrich"),
    ("marque", "score"),
    ("compare", "compare"),
    ("translate", "translate"),
    ("traduis", "translate"),
    ("resize", "resize"),
    ("extrais", "extract"),
    ("extract", "extract"),
    ("summarize", "summarize"),
    ("analyse", "analyze"),
    ("relance", "remind"),
    ("rassemble", "gather"),
    ("produit", "produce"),
];

/// Calendar words: « every FRIDAY » is a cadence, not a per-item fan-out.
const TIME_WORDS: &[&str] = &[
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    "morning",
    "evening",
    "night",
    "day",
    "daily",
    "weekly",
    "monthly",
    "hour",
    "hourly",
    "weekend",
    "lundi",
    "mardi",
    "mercredi",
    "jeudi",
    "vendredi",
    "samedi",
    "dimanche",
    "matin",
    "soir",
    "jour",
    "semaine",
    "mois",
    "heure",
];

/// The word following an « every »-family trigger, when one exists.
fn after_trigger<'a>(words: &[&'a str], triggers: &[&str]) -> Option<(&'a str, &'a str)> {
    words
        .windows(2)
        .find(|pair| triggers.contains(&pair[0]))
        .map(|pair| (pair[0], pair[1]))
}

/// The named artifact: a literal `FILE.md` first, then the document
/// vocabulary, first match wins.
fn detect_output(lower: &str, words: &[&str]) -> Option<String> {
    if let Some(file) = words
        .iter()
        .find(|w| w.to_ascii_lowercase().ends_with(".md"))
    {
        return Some((*file).to_owned());
    }
    for (needle, name) in [
        ("rapport", "report"),
        ("report", "report"),
        ("brief", "brief"),
        ("manifest", "manifest"),
        ("email", "email"),
        ("notes", "notes"),
    ] {
        if lower.contains(needle) {
            return Some(name.to_owned());
        }
    }
    None
}

/// The phrase-level rules — the ones that read the lowered utterance
/// whole (a document output → fs-write · « email » → net · « validation
/// humaine » → a human gate · « sans déployer » → a forbidden deploy).
/// Returns (required, constraints, forbidden).
fn phrase_capabilities(
    lower: &str,
    output: Option<&str>,
) -> (Vec<Capability>, Vec<String>, Vec<Capability>) {
    let mut required = Vec::new();
    let mut constraints = Vec::new();
    let mut forbidden = Vec::new();
    // A document output — or a save/keep/write verb — commits to fs-write.
    let document = output.is_some_and(|o| {
        o.to_ascii_lowercase().ends_with(".md")
            || matches!(o, "report" | "brief" | "manifest" | "notes")
    });
    if document
        || [" save ", " keep ", " write ", " crée "]
            .iter()
            .any(|v| lower.contains(v))
    {
        required.push(Capability::FsWrite);
    }
    if [
        "email", "http", "url", "website", "scrape", "crawl", "fetch",
    ]
    .iter()
    .any(|n| lower.contains(n))
    {
        required.push(Capability::Net);
    }
    if [
        "validation humaine",
        "human approval",
        "approval",
        "approve",
        "approbation",
    ]
    .iter()
    .any(|n| lower.contains(n))
    {
        required.push(Capability::HumanGate);
        constraints.push("human-approval".to_owned());
    }
    // Negations: « sans déployer » · « without deploy » · « no deploy ».
    for (phrase, cap) in [
        ("sans déployer", Capability::Deploy),
        ("sans deploy", Capability::Deploy),
        ("without deploy", Capability::Deploy),
        ("no deploy", Capability::Deploy),
    ] {
        if lower.contains(phrase) && !forbidden.contains(&cap) {
            forbidden.push(cap);
            constraints.push("no-deploy".to_owned());
        }
    }
    (required, constraints, forbidden)
}

/// The done-statements — one per recognized contract element.
fn derive_success_criteria(
    output: Option<&str>,
    constraints: &[String],
    required: &[Capability],
) -> Vec<String> {
    let mut criteria = Vec::new();
    if let Some(o) = output {
        criteria.push(format!("output `{o}` produced"));
    }
    for c in constraints {
        if c.starts_with("cadence:") {
            criteria.push(format!("runs on {c}"));
        }
    }
    if required.contains(&Capability::HumanGate) {
        criteria.push("a human approves before the act".to_owned());
    }
    criteria
}

/// The deterministic utterance read (see the module docs for the limits).
fn extract(intent: &str) -> IntentContract {
    let lower = intent.to_ascii_lowercase();
    let words: Vec<&str> = intent
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let tokens: Vec<String> = words.iter().map(|w| w.to_ascii_lowercase()).collect();

    let lexicon = |table: &[(&str, &str)]| -> Vec<String> {
        let mut hits = Vec::new();
        for (word, canonical) in table {
            if tokens.iter().any(|t| t == word) && !hits.contains(&(*canonical).to_owned()) {
                hits.push((*canonical).to_owned());
            }
        }
        hits
    };
    let sources = lexicon(SOURCE_LEXICON);
    let transformations = lexicon(TRANSFORM_LEXICON);

    let output = detect_output(&lower, &words);
    let (mut required, mut constraints, forbidden) = phrase_capabilities(&lower, output.as_deref());

    // « every Friday » = cadence · « every PNG » = per-item fan-out.
    let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
    if let Some((trigger, next)) = after_trigger(&token_refs, &["every", "each", "chaque"]) {
        if TIME_WORDS.contains(&next) {
            constraints.push(format!("cadence:{trigger} {next}"));
        } else {
            required.push(Capability::FanOut);
        }
    }
    for phrase in [
        "par page",
        "per page",
        "par fichier",
        "per file",
        "par item",
        "per item",
    ] {
        if lower.contains(phrase) && !required.contains(&Capability::FanOut) {
            required.push(Capability::FanOut);
        }
    }

    let success_criteria = derive_success_criteria(output.as_deref(), &constraints, &required);

    IntentContract {
        goal: intent.to_owned(),
        sources,
        transformations,
        output,
        constraints,
        required_capabilities: required,
        forbidden_capabilities: forbidden,
        success_criteria,
    }
}

/// The catalog facet of one entry — where it came from decides what
/// taking it MEANS: a job runs today, a lesson teaches, a skeleton is
/// homework. Derived from the name (template set membership · the
/// numbered-lesson slug convention), never hand-listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Facet {
    /// A complete, runnable example doing a real job (the 26).
    Job,
    /// A numbered curriculum lesson (`01-…` · the 13).
    Lesson,
    /// An embedded template with `# SLOT:` lines to fill (the 10).
    Skeleton,
}

impl Facet {
    /// The display noun (the clarify list's facet chip).
    pub(crate) fn noun(self) -> &'static str {
        match self {
            Self::Job => "job",
            Self::Lesson => "lesson",
            Self::Skeleton => "skeleton",
        }
    }
}

/// The facet of a catalog name. Skeletons are the template set; a
/// leading digit is the curriculum convention (`01-hello`); everything
/// else in the example corpus is a job.
pub(crate) fn facet_of(name: &str) -> Facet {
    if nika_pack::template(name).is_some() {
        return Facet::Skeleton;
    }
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Facet::Lesson;
    }
    Facet::Job
}

/// The WHOLE catalog the router sees (user gauntlet 2026-07-31 · G-16:
/// the router searched the 10 skeletons while the 26 human-worded jobs
/// — the exact matter that answers a human sentence — stayed invisible;
/// 3 probes, 3 misses, one rage-quit adjacent). Examples first, then
/// templates: names never collide (pinned by test) so one flat
/// namespace routes everything.
fn catalog_names() -> Vec<String> {
    let mut names = nika_pack::example_slugs();
    names.extend(nika_pack::template_names());
    names
}

/// One catalog body by name — template or example, whichever set owns
/// the name (disjoint by the no-collision pin).
fn body_of(name: &str) -> Option<&'static str> {
    nika_pack::template(name).or_else(|| nika_pack::example(name))
}

/// BM25-route a free-form intent against the embedded catalog, gated by
/// the [`IntentContract`]: extract FIRST, then score. A candidate must
/// carry every required capability (and none forbidden); the winner must
/// clear [`TAU`] and outscore the runner-up by [`MARGIN`]. Anything else
/// is a [`RoutingOutcome::NeedsClarification`] — routing below the
/// evidence bar would be a guess (P0-1 · P0-10).
/// The naive singular of a plural token — `invoices` → `invoice`.
/// Query-side only, like every other expansion here: documents are never
/// rewritten, so a wrong guess costs one term that matches nothing.
fn singular_of(token: &str) -> Option<&str> {
    if let Some(stem) = token.strip_suffix("es")
        && stem.len() >= 3
        && stem.ends_with(['s', 'x', 'z', 'h'])
    {
        return Some(stem);
    }
    token
        .strip_suffix('s')
        .filter(|stem| stem.len() >= 3 && !stem.ends_with('s'))
}

/// Index each entry's MEANINGFUL vocabulary — verbs · tools ·
/// structure — but STRIP `#` comments. The `# SLOT: kebab-case workflow
/// id` scaffolding prose otherwise pollutes the index, so
/// boilerplate/stopword queries ("slot" · "kebab" · "fill" · "the")
/// spuriously route instead of listing the set. Real intent routes on
/// the YAML verbs/tools + the ALIASES, not the comment prose — and a
/// job's `description:` line (its human wording) rides the index too:
/// that line is exactly what a human sentence matches.
fn build_template_index(names: &[String]) -> BmIndex {
    let mut index = BmIndex::new(BmParams::default());
    for (i, name) in names.iter().enumerate() {
        let body = body_of(name).unwrap_or_default();
        let sentence = crate::banner::sentence(body).unwrap_or_default();
        let meaningful: String = body
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .map(|l| l.split_once(" #").map_or(l, |(before, _)| before))
            .collect::<Vec<_>>()
            .join("\n");
        let Ok(doc_id) = u32::try_from(i) else {
            continue;
        };
        index.add_document(doc_id, &format!("{name}\n{sentence}\n{meaningful}"));
    }
    index.finalize();
    index
}

/// Route against the WHOLE catalog (jobs + lessons + skeletons) — the
/// `nika new --from <plain words>` door (G-16: human words must see the
/// human-worded corpus).
pub(crate) fn route(intent: &str) -> RoutingOutcome {
    route_impl(intent, &catalog_names(), TAU_CATALOG)
}

/// Route against the SKELETON set only — the wizard's door (its whole
/// gesture instantiates a `# SLOT:` file, so its corpus IS the template
/// set; the index is built on those 10 docs alone, keeping its BM25
/// scores — and the TAU/MARGIN calibration — exactly as shipped).
pub(crate) fn route_skeletons(intent: &str) -> RoutingOutcome {
    route_impl(intent, &nika_pack::template_names(), TAU)
}

/// The winner's best SAME-FACET rival — the score the margin judges
/// against. Scans the whole ranked tail, never just position 1 (found
/// by the adversarial pass 2026-07-31: a same-facet peer hiding at
/// position 2 behind a cross-facet runner-up made the router
/// overconfident — it routed a coin-flip it should have clarified).
fn best_same_facet_runner(qualified: &[(String, f64)]) -> f64 {
    let Some((winner, _)) = qualified.first() else {
        return 0.0;
    };
    qualified[1..]
        .iter()
        .find(|(n, _)| facet_of(n) == facet_of(winner))
        .map_or(0.0, |(_, s)| *s)
}

fn route_impl(intent: &str, names: &[String], tau: f64) -> RoutingOutcome {
    let contract = extract(intent);
    let index = build_template_index(names);

    // Keep only signal-bearing tokens (drop stopwords + Nika boilerplate);
    // an all-boilerplate query routes NOWHERE → the honest unknown · list.
    let tokens: Vec<String> = intent
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|t| !t.is_empty() && !STOPWORDS.contains(&t.as_str()))
        .collect();
    if tokens.is_empty() {
        return RoutingOutcome::NeedsClarification {
            candidates: Vec::new(),
            contract,
        };
    }
    let mut query = tokens.join(" ");
    for token in &tokens {
        for (word, expansions) in ALIASES {
            if token == *word {
                for e in *expansions {
                    query.push(' ');
                    query.push_str(e);
                }
            }
        }
        if let Some(singular) = singular_of(token) {
            query.push(' ');
            query.push_str(singular);
        }
    }

    let ranked = index.top_k(&query, names.len());
    // The contract gate: every required capability present, every
    // forbidden one absent (capabilities derived from the entry's body).
    let qualified: Vec<(String, f64)> = ranked
        .iter()
        .filter_map(|(doc, score)| {
            let name = names.get(*doc as usize)?;
            let caps = template_capabilities(body_of(name)?);
            let ok = contract
                .required_capabilities
                .iter()
                .all(|c| caps.contains(c))
                && contract
                    .forbidden_capabilities
                    .iter()
                    .all(|c| !caps.contains(c));
            ok.then(|| (name.clone(), *score))
        })
        .collect();
    let Some((winner, s1)) = qualified.first() else {
        // The contract emptied the field — clarify with the closest RAW
        // matches so the human still sees what almost fit.
        let candidates = ranked
            .iter()
            .filter(|(_, s)| *s > 0.0)
            .take(3)
            .filter_map(|(d, _)| names.get(*d as usize).cloned())
            .collect();
        return RoutingOutcome::NeedsClarification {
            candidates,
            contract,
        };
    };
    // The margin judges COIN-FLIPS BETWEEN PEERS: two entries of the
    // SAME facet inside the margin are interchangeable answers — the
    // honest move is to name both. Entries of DIFFERENT facets are
    // different KINDS of answer (the generic shape vs the concrete
    // job), not a coin-flip: the words already ranked them, the top
    // routes, and the clarify list stays the place cross-facet
    // neighbours surface (entry-doors redesign R1 · G-16). On the
    // skeleton-only corpus every pair is same-facet, so the wizard's
    // shipped behavior is byte-identical.
    let same_facet_runner = best_same_facet_runner(&qualified);
    if *s1 >= tau && (same_facet_runner <= 0.0 || *s1 >= MARGIN * same_facet_runner) {
        RoutingOutcome::Routed {
            template: winner.clone(),
            score: *s1,
            contract,
        }
    } else {
        let candidates = qualified
            .iter()
            .filter(|(_, s)| *s > 0.0)
            .take(3)
            .map(|(n, _)| n.clone())
            .collect();
        RoutingOutcome::NeedsClarification {
            candidates,
            contract,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The audit fixture (ux-fixtures/semantic-cases.jsonl · audit UX
    /// 2026-07-30), copied VERBATIM into the crate so the ratchet runs
    /// hermetic — 10 utterances with the template classes each must never
    /// resolve to. Provenance: /Users/thibaut/Desktop/test-project/ux-fixtures.
    const SEMANTIC_CASES: &str = include_str!("../tests/fixtures/semantic-cases.jsonl");

    /// The audit's forbidden CLASSES → the embedded template(s) embodying
    /// each (mapped by hand against the 10 skeletons, 2026-07-31):
    /// - `docker` — docker-report is the docker-daemon skeleton;
    /// - `image_generation` / `asset_manifest` — media-asset-pack renders
    ///   images (`nika:image_generate`) and writes their manifest;
    /// - `items_markdown_generic` / `flat_merge` — fanout globs `*.md`
    ///   items and merges them into ONE report (the tree flattens);
    /// - `send_without_approval` — the outward-acting skeletons with NO
    ///   `nika:prompt` human gate (fetch/notify/exec · etl-state and
    ///   human-gated-ship DO gate, so they are out);
    /// - `single_example_page` / `single_brief` — website-brief turns ONE
    ///   page into ONE brief;
    /// - `echo_shipped` — human-gated-ship's gates + act are `echo` stubs;
    /// - `slack_notification` — the skeletons notifying `hooks.slack.com`;
    /// - `deployment` / `shipping` — human-gated-ship IS the ship skeleton;
    /// - `generic_processing` — chain, the generic gather→think→persist;
    /// - `llm_markdown_processing` — the infer-over-markdown skeletons.
    ///
    /// An unmapped class guards a FUTURE template: vacuously green today.
    // The catalog widened to the jobs (G-16) — a class extends to a job
    // ONLY where the class is the job's ESSENCE (og-images and
    // competitor-radar exist to render images — a cost surprise), never
    // where it is one incidental step (six jobs notify Slack as their
    // LAST step; the archetype belt guards the notification skeletons,
    // and the check-time JOURNEY line names every incidental egress
    // before a token is spent). An unmapped entry stays the vacuous
    // guard it always was.
    fn forbidden_templates(class: &str) -> &'static [&'static str] {
        match class {
            "docker" => &["docker-report"],
            "image_generation" | "asset_manifest" => {
                &["media-asset-pack", "competitor-radar", "og-images"]
            }
            "items_markdown_generic" | "flat_merge" => &["fanout"],
            "send_without_approval" => &[
                "api-upload-and-create",
                "website-brief",
                "gate-and-act",
                "docker-report",
            ],
            "single_example_page" | "single_brief" => &["website-brief"],
            "echo_shipped" | "deployment" | "shipping" => &["human-gated-ship"],
            "slack_notification" => &["human-gated-ship", "gate-and-act"],
            "generic_processing" => &["chain"],
            "llm_markdown_processing" => &["chain", "fanout", "website-brief"],
            _ => &[],
        }
    }

    struct Case {
        id: String,
        utterance: String,
        forbidden: Vec<String>,
    }

    fn cases() -> Vec<Case> {
        SEMANTIC_CASES
            .lines()
            .map(|line| {
                let v: serde_json::Value =
                    serde_json::from_str(line).expect("fixture line is json");
                Case {
                    id: v["id"].as_str().expect("id").to_owned(),
                    utterance: v["utterance"].as_str().expect("utterance").to_owned(),
                    forbidden: v["forbidden"]
                        .as_array()
                        .expect("forbidden array")
                        .iter()
                        .map(|c| c.as_str().expect("class").to_owned())
                        .collect(),
                }
            })
            .collect()
    }

    fn contract_of(utterance: &str) -> IntentContract {
        match route(utterance) {
            RoutingOutcome::Routed { contract, .. }
            | RoutingOutcome::NeedsClarification { contract, .. } => contract,
        }
    }

    /// The P0-1/P0-10 ratchet, table-driven over the audit fixture: the
    /// RESOLVED template never belongs to the case's forbidden classes,
    /// and the non-interactive door (`guided::run`) writes ONLY on a
    /// confident route — a clarification is an honest non-zero exit that
    /// names its candidates and leaves the disk untouched; a route lands
    /// a DRAFT that says so and hands over to `nika check`.
    #[test]
    fn semantic_cases_never_resolve_into_a_forbidden_template() {
        for case in cases() {
            let forbidden: Vec<&str> = case
                .forbidden
                .iter()
                .flat_map(|c| forbidden_templates(c))
                .copied()
                .collect();
            let outcome = route(&case.utterance);
            if let RoutingOutcome::Routed { template, .. } = &outcome {
                assert!(
                    !forbidden.contains(&template.as_str()),
                    "{}: `{template}` is forbidden for {:?}",
                    case.id,
                    case.utterance
                );
            }
            let dest = std::env::temp_dir().join(format!(
                "nika-intent-{}-{}.nika.yaml",
                std::process::id(),
                case.id
            ));
            let dest_s = dest.to_string_lossy().into_owned();
            let out = crate::guided::run(&case.utterance, Some(&dest_s), true);
            match &outcome {
                RoutingOutcome::Routed { template, .. } => {
                    assert_eq!(out.code, crate::codes::OK, "{}: {}", case.id, out.text);
                    assert!(
                        dest.exists(),
                        "{}: a confident route lands the file",
                        case.id
                    );
                    let lower = out.text.to_ascii_lowercase();
                    // A routed SKELETON is a draft (SLOTs to fill) and
                    // says so; a routed EXAMPLE lands verbatim and says
                    // « yours now » — calling it a draft would be the
                    // lie (the catalog widened past skeletons · G-16).
                    let says_what_it_is = match facet_of(template) {
                        Facet::Skeleton => lower.contains("draft"),
                        Facet::Job | Facet::Lesson => lower.contains("yours now"),
                    };
                    assert!(
                        says_what_it_is,
                        "{}: the message names what landed ({:?}): {}",
                        case.id,
                        facet_of(template),
                        out.text
                    );
                    assert!(
                        out.text.contains("nika check"),
                        "{}: hands over to check: {}",
                        case.id,
                        out.text
                    );
                    // The WORD « ready » (the readiness over-promise) —
                    // never the substring: « already yours, kept » is
                    // the ingredients note, not a claim.
                    assert!(
                        !lower
                            .split(|c: char| !c.is_alphanumeric())
                            .any(|w| w == "ready"),
                        "{}: never « ready »: {}",
                        case.id,
                        out.text
                    );
                }
                RoutingOutcome::NeedsClarification { candidates, .. } => {
                    assert!(
                        !dest.exists(),
                        "{}: below the bar, NOTHING is written",
                        case.id
                    );
                    assert_eq!(
                        out.code,
                        crate::codes::FILE,
                        "{}: an honest non-zero exit: {}",
                        case.id,
                        out.text
                    );
                    for c in candidates {
                        assert!(
                            out.text.contains(c.as_str()),
                            "{}: the error names `{c}`: {}",
                            case.id,
                            out.text
                        );
                    }
                }
            }
            std::fs::remove_file(&dest).ok();
        }
    }

    /// P0-1 reproduced: « Relance les factures impayées par email après
    /// validation humaine » routed to docker-report before this slice —
    /// a docker skeleton for a human-gated email job. The contract
    /// requires net + a human gate; no skeleton clears the bar.
    #[test]
    fn the_invoice_case_never_routes_to_the_docker_report() {
        let outcome = route("Relance les factures impayées par email après validation humaine");
        assert!(
            matches!(outcome, RoutingOutcome::NeedsClarification { .. }),
            "the reproduced contre-sens must clarify, got {outcome:?}"
        );
    }

    #[test]
    fn the_invoice_contract_requires_net_and_a_human_gate() {
        let c = contract_of("Relance les factures impayées par email après validation humaine");
        assert!(c.sources.contains(&"invoices".to_owned()), "{c:?}");
        assert_eq!(c.output.as_deref(), Some("email"), "{c:?}");
        assert!(
            c.required_capabilities.contains(&Capability::Net),
            "email → net: {c:?}"
        );
        assert!(
            c.required_capabilities.contains(&Capability::HumanGate),
            "validation humaine → human gate: {c:?}"
        );
        assert!(
            c.constraints.iter().any(|s| s.contains("approval")),
            "the approval constraint is on record: {c:?}"
        );
    }

    #[test]
    fn a_negation_lands_in_forbidden_capabilities() {
        let c = contract_of(
            "Quand une alerte arrive rassemble les logs et produit un rapport d'incident sans déployer",
        );
        assert!(
            c.forbidden_capabilities.contains(&Capability::Deploy),
            "« sans déployer » forbids deploy: {c:?}"
        );
        assert!(c.sources.contains(&"logs".to_owned()), "{c:?}");
        assert_eq!(c.output.as_deref(), Some("report"), "{c:?}");
    }

    #[test]
    fn a_cadence_is_not_a_fanout() {
        let c = contract_of("Every Friday turn support tickets into a prioritized Markdown report");
        assert!(
            c.constraints.iter().any(|s| s.starts_with("cadence:")),
            "every Friday is a cadence: {c:?}"
        );
        assert!(
            !c.required_capabilities.contains(&Capability::FanOut),
            "a cadence is not a per-item fan-out: {c:?}"
        );
        assert!(c.sources.contains(&"support-tickets".to_owned()), "{c:?}");
        assert!(
            c.transformations.contains(&"prioritize".to_owned()),
            "{c:?}"
        );
        // …but a real per-item utterance DOES ask for the fan-out.
        let batch = contract_of("Batch resize every PNG in a folder and write an image manifest");
        assert!(
            batch.required_capabilities.contains(&Capability::FanOut),
            "every PNG → fan-out: {batch:?}"
        );
        assert!(
            batch.required_capabilities.contains(&Capability::FsWrite),
            "write a manifest → fs-write: {batch:?}"
        );
        assert!(
            batch.transformations.contains(&"resize".to_owned()),
            "{batch:?}"
        );
    }

    #[test]
    fn the_contract_serializes_to_json() {
        let c = contract_of("Relance les factures impayées par email après validation humaine");
        let json = c.to_json();
        assert_eq!(json["goal"].as_str().expect("goal"), c.goal);
        let required: Vec<&str> = json["required_capabilities"]
            .as_array()
            .expect("array")
            .iter()
            .map(|c| c.as_str().expect("cap"))
            .collect();
        assert!(required.contains(&"net"), "{json}");
        assert!(required.contains(&"human-gate"), "{json}");
        // The JSON round-trips through the string form (serializable).
        let text = serde_json::to_string(&json).expect("serialize");
        let back: serde_json::Value = serde_json::from_str(&text).expect("parse");
        assert_eq!(back, json);
    }

    #[test]
    fn below_the_bar_clarifies_with_the_closest_candidates() {
        // A genuine coin-flip: several same-facet entries say nearly the
        // same words, so none wins and all are named. The NAMES moved
        // when the corpus did (`competitor-radar` entered on the word it
        // is about, `media-asset-pack` left) — what this pins is the
        // behaviour, never a score from a corpus that has since changed.
        let outcome = route("Chaque lundi compare three competitors and write a French brief");
        let candidates = match outcome {
            RoutingOutcome::NeedsClarification { candidates, .. } => Some(candidates),
            RoutingOutcome::Routed { .. } => None,
        }
        .expect("a coin-flip must clarify");
        assert!(
            candidates.contains(&"competitor-radar".to_owned()),
            "{candidates:?}"
        );
        assert!(
            candidates.contains(&"website-brief".to_owned()),
            "{candidates:?}"
        );
        assert!(
            candidates.len() <= 3,
            "2-3 candidates, not the catalog: {candidates:?}"
        );
    }

    #[test]
    fn a_confident_route_carries_the_contract() {
        let outcome = route("summarize every item in parallel");
        let (template, contract) = match outcome {
            RoutingOutcome::Routed {
                template, contract, ..
            } => Some((template, contract)),
            RoutingOutcome::NeedsClarification { .. } => None,
        }
        .expect("the fan-out intent routes");
        assert_eq!(template, "fanout");
        assert!(contract.required_capabilities.contains(&Capability::FanOut));
    }

    #[test]
    fn zero_evidence_clarifies_with_no_candidates() {
        let outcome = route("zzzz qqqq xxxx");
        let candidates = match outcome {
            RoutingOutcome::NeedsClarification { candidates, .. } => Some(candidates),
            RoutingOutcome::Routed { .. } => None,
        }
        .expect("gibberish clarifies");
        assert!(
            candidates.is_empty(),
            "zero evidence names nobody: {candidates:?}"
        );
    }

    /// Every embedded template's derived capability set stays honest —
    /// the derivation is from the body, so this pins the DERIVATION, not
    /// a hand-table (a template gaining a tool updates here deliberately).
    #[test]
    fn template_capabilities_derive_from_the_bodies() {
        let caps_of =
            |name: &str| template_capabilities(nika_pack::template(name).expect("embedded"));
        let fanout = caps_of("fanout");
        assert!(fanout.contains(&Capability::FanOut), "{fanout:?}");
        assert!(
            !fanout.contains(&Capability::FsWrite),
            "fanout merges in memory — no write: {fanout:?}"
        );
        let gated = caps_of("human-gated-ship");
        assert!(gated.contains(&Capability::HumanGate), "{gated:?}");
        assert!(
            !gated.contains(&Capability::FsWrite),
            "the ship skeleton writes no file: {gated:?}"
        );
        let docker = caps_of("docker-report");
        assert!(docker.contains(&Capability::Container), "{docker:?}");
        assert!(docker.contains(&Capability::FsWrite), "{docker:?}");
    }
}

#[cfg(test)]
#[allow(
    clippy::print_stdout,
    clippy::disallowed_macros,
    reason = "a one-shot calibration instrument: its whole output IS the printout (run with --ignored --nocapture)"
)]
mod calibration_probe {
    use super::*;

    #[test]
    #[ignore = "one-shot calibration probe · run with --ignored --nocapture"]
    fn print_catalog_scores() {
        let names = catalog_names();
        let index = build_template_index(&names);
        let cases = [
            "extract action items from meeting notes with owners and deadlines",
            "translate my docs folder to spanish",
            "chase unpaid invoices every friday",
            "please answer my emails",
            "summarize every item in parallel",
            "an agentic loop that researches until the budget runs out",
            "scrape a website and summarize it",
            "fill the lines",
            "the workflow template",
            "make it good",
            "do the thing properly",
        ];
        for intent in cases {
            let tokens: Vec<String> = intent
                .split(|c: char| !c.is_ascii_alphanumeric())
                .map(str::to_ascii_lowercase)
                .filter(|t| !t.is_empty() && !STOPWORDS.contains(&t.as_str()))
                .collect();
            let mut query = tokens.join(" ");
            for token in &tokens {
                for (word, expansions) in ALIASES {
                    if token == *word {
                        for e in *expansions {
                            query.push(' ');
                            query.push_str(e);
                        }
                    }
                }
            }
            let ranked = index.top_k(&query, 3);
            let row: Vec<String> = ranked
                .iter()
                .filter_map(|(d, s)| names.get(*d as usize).map(|n| format!("{n}={s:.3}")))
                .collect();
            println!("CAL · {intent} → {}", row.join(" · "));
        }
    }
}

#[cfg(test)]
mod catalog_routing_laws {
    use super::*;

    #[test]
    fn release_notes_intent_leads_the_named_job() {
        match route("write release notes from git commits") {
            RoutingOutcome::Routed { template, .. } => assert_eq!(template, "release-notes"),
            RoutingOutcome::NeedsClarification { candidates, .. } => assert_eq!(
                candidates.first().map(String::as_str),
                Some("release-notes"),
                "the named job must lead the clarify list: {candidates:?}"
            ),
        }
    }

    /// THE LAW (RAMS-1 · G-16 · entry-doors probes 2026-07-31): the
    /// three probes that each failed on the skeleton-only corpus (« 3
    /// sondes, 3 échecs ») now find the job-shaped matter. The first
    /// two route outright; the invoice probe may honestly clarify (its
    /// runner-up is a same-facet coin-flip) but the job MUST lead the
    /// candidates — visible either way, never the 10-skeleton dump.
    #[test]
    fn the_three_evidence_probes_see_the_jobs() {
        let p1 = route("extract action items from meeting notes with owners and deadlines");
        assert!(
            matches!(p1, RoutingOutcome::Routed { ref template, .. } if template == "meeting-actions"),
            "probe 1 must route to the job: {p1:?}"
        );
        // Probe 2 carries the same honest arm as probe 3: its runner-up
        // (`07-for-each-locales`) is the LESSON that teaches the very
        // shape the job uses, so the two say almost the same words and
        // the margin does not clear. The job must LEAD either way.
        match route("translate my docs folder to spanish") {
            RoutingOutcome::Routed { template, .. } => {
                assert_eq!(template, "localization-factory");
            }
            RoutingOutcome::NeedsClarification { candidates, .. } => assert_eq!(
                candidates.first().map(String::as_str),
                Some("localization-factory"),
                "the job leads the clarify list: {candidates:?}"
            ),
        }
        match route("chase unpaid invoices every friday") {
            RoutingOutcome::Routed { template, .. } => assert_eq!(template, "invoice-chaser"),
            RoutingOutcome::NeedsClarification { candidates, .. } => assert_eq!(
                candidates.first().map(String::as_str),
                Some("invoice-chaser"),
                "the job leads the clarify list: {candidates:?}"
            ),
        }
    }

    /// The one flat namespace the catalog rests on: no example slug may
    /// collide with a template name (facet derivation + body lookup
    /// both assume the sets are disjoint — a collision would shadow).
    #[test]
    fn catalog_names_never_collide() {
        let templates = nika_pack::template_names();
        for slug in nika_pack::example_slugs() {
            assert!(
                !templates.contains(&slug),
                "example `{slug}` shadows a template name"
            );
        }
    }

    /// The facet derivation reads the catalog itself: every template is
    /// a Skeleton, every numbered example a Lesson, everything else a
    /// Job — and the counts match the corpus (a moved file shows up
    /// here before it surprises the router).
    #[test]
    fn facets_partition_the_whole_catalog() {
        let names = catalog_names();
        let (mut jobs, mut lessons, mut skeletons) = (0, 0, 0);
        for n in &names {
            match facet_of(n) {
                Facet::Job => jobs += 1,
                Facet::Lesson => lessons += 1,
                Facet::Skeleton => skeletons += 1,
            }
        }
        assert_eq!(skeletons, nika_pack::template_names().len());
        assert!(lessons >= 12, "the numbered curriculum: {lessons}");
        assert!(jobs >= 20, "the job corpus: {jobs}");
        assert_eq!(jobs + lessons + skeletons, names.len());
    }

    /// The wizard's world is UNTOUCHED: its route sees the skeleton set
    /// only, so a job-shaped utterance still lands the generic skeleton
    /// there (the SLOT gesture) while the guided door sees the catalog.
    #[test]
    fn the_wizard_route_stays_skeleton_only() {
        match route_skeletons("translate my docs folder to spanish") {
            RoutingOutcome::Routed { template, .. } => {
                assert!(
                    nika_pack::template(&template).is_some(),
                    "the wizard corpus is the template set: {template}"
                );
            }
            RoutingOutcome::NeedsClarification { candidates, .. } => {
                for c in &candidates {
                    assert!(
                        nika_pack::template(c).is_some(),
                        "a wizard candidate is always a skeleton: {c}"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod adversarial_pass_2026_07_31 {
    use super::*;

    /// THE LAW (found adversarially): the margin's rival is the best
    /// SAME-FACET entry anywhere in the ranked tail — never merely
    /// position 1. A job coin-flip hiding at position 2 behind a
    /// cross-facet skeleton must still clarify. Synthetic names lean on
    /// the derivation itself: `chain` IS a pack template (Skeleton);
    /// unknown non-digit names read as Jobs.
    #[test]
    fn the_margin_rival_is_the_best_same_facet_anywhere() {
        let q = vec![
            ("alpha-job".to_owned(), 10.0),
            ("chain".to_owned(), 9.0),
            ("beta-job".to_owned(), 8.5),
        ];
        assert!(
            (best_same_facet_runner(&q) - 8.5).abs() < f64::EPSILON,
            "the job peer at position 2 is the rival, not the skeleton at 1"
        );
        // No same-facet rival anywhere → 0.0 (the top routes freely).
        let lone = vec![("alpha-job".to_owned(), 10.0), ("chain".to_owned(), 9.0)];
        assert!(best_same_facet_runner(&lone).abs() < f64::EPSILON);
        assert!(
            best_same_facet_runner(&[]).abs() < f64::EPSILON,
            "empty is calm"
        );
    }

    /// The LIVING bounds of [`TAU_CATALOG`] (Q2 of the socratic pass):
    /// the floor was measured against TODAY's corpus, and nothing used
    /// to force a re-measure when the corpus moves. These two probes
    /// ARE the bounds — a corpus change that lets a vague utterance
    /// route (or starves the honest one below the floor plus the
    /// contract gate) turns this red, and red here means « re-run
    /// `calibration_probe`, re-derive `TAU_CATALOG` », never « bump the
    /// assertion ».
    #[test]
    fn tau_catalog_bounds_hold_on_the_living_corpus() {
        for vague in ["make it good", "do the thing properly"] {
            assert!(
                matches!(route(vague), RoutingOutcome::NeedsClarification { .. }),
                "`{vague}` is vague and must never route — TAU_CATALOG drifted under the corpus"
            );
        }
        // The honest end stays reachable: the invoice probe either
        // routes or leads its clarify list (the probe test pins which).
        match route("chase unpaid invoices every friday") {
            RoutingOutcome::Routed { template, .. } => assert_eq!(template, "invoice-chaser"),
            RoutingOutcome::NeedsClarification { candidates, .. } => {
                assert_eq!(
                    candidates.first().map(String::as_str),
                    Some("invoice-chaser")
                );
            }
        }
    }
}
