// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Private compiler pattern-facet derivation (#1666).
//!
//! This is compiler metadata, not a workflow language key, not a fifth
//! verb, and not an SDK noun. Facets come from the parsed [`RawWorkflow`]
//! plus the Check report already available to Compile. Comments, prompt
//! text and envelope keys are never scanned for effects or task identity.
//! Check remains the membrane: this module does not grant permits.
//! Refused constructs are not emitted.

use std::collections::BTreeSet;

use nika_schema::raw::{RawAction, RawInvokeTarget, RawWorkflow};
use nika_schema::{FileId, ParseMode};

use nika_check::CheckReport;

/// Private facet-schema version carried on every derived contract.
pub(crate) const PATTERN_CONTRACT_VERSION: u32 = 0;

/// Canonical miss card: no registered pattern is compatible.
pub(crate) const NONE_CARD_ID: &str = "NONE";

/// Alias of [`NONE_CARD_ID`]. Reject-all is a compiler citizen, not a
/// schema convention.
pub(crate) const NO_COMPATIBLE_PATTERN_ID: &str = "NO_COMPATIBLE_PATTERN";

const FETCH_TOOL: &str = "nika:fetch";
const PROMPT_TOOL: &str = "nika:prompt";
const READ_TOOL: &str = "nika:read";
const GLOB_TOOL: &str = "nika:glob";
const GREP_TOOL: &str = "nika:grep";
const WRITE_TOOL: &str = "nika:write";
const EDIT_TOOL: &str = "nika:edit";

/// Whether a construct can be expressed as four-verb Nika source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum Legality {
    /// Fully expressible with infer / exec / invoke / agent.
    Expressible,
    /// Expressible, but some Check facts could not be pinned statically.
    ExpressibleWithLoss,
    /// The spec has not yet named this construct.
    DeferredBySpec,
    /// Never emitted (fifth verb, public pattern key, granted authority).
    RefusedPermanent,
}

/// Origin of one contract field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldProvenance {
    /// Taken from the AST, Check report, schema, permits, tools or outputs.
    Derived,
    /// Written by a human (or the compiler schema itself) with intent.
    Authored,
}

/// Closed effect vocabulary derived from real verbs and builtins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) enum EffectCategory {
    /// Network egress — a real `nika:fetch` invoke or Check `needed.net`.
    Net,
    /// Process execution — a real `exec:` verb or Check `needed.exec`.
    Exec,
    /// Filesystem read (`nika:read` / `nika:glob` / `nika:grep`).
    FsRead,
    /// Filesystem write (`nika:write` / `nika:edit`).
    FsWrite,
    /// A model call (`infer:` / `agent:`).
    Llm,
    /// A human gate (`nika:prompt`).
    HumanGate,
}

/// Derived hard-filter facts for [`IndexCard`] retrieval.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Facets {
    /// Task identities from the `tasks:` map only — never envelope keys.
    pub task_ids: BTreeSet<String>,
    /// Effects observed on the AST and corroborated by Check `needed`.
    pub effect_categories: BTreeSet<EffectCategory>,
}

/// Per-field provenance for a [`PatternContract`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContractProvenance {
    pub id: FieldProvenance,
    pub version: FieldProvenance,
    pub legality: FieldProvenance,
    pub task_ids: FieldProvenance,
    pub effect_categories: FieldProvenance,
}

impl ContractProvenance {
    fn derived_body() -> Self {
        Self {
            id: FieldProvenance::Derived,
            version: FieldProvenance::Authored,
            legality: FieldProvenance::Derived,
            task_ids: FieldProvenance::Derived,
            effect_categories: FieldProvenance::Derived,
        }
    }

    fn authored() -> Self {
        Self {
            id: FieldProvenance::Authored,
            version: FieldProvenance::Authored,
            legality: FieldProvenance::Authored,
            task_ids: FieldProvenance::Authored,
            effect_categories: FieldProvenance::Authored,
        }
    }
}

/// Private pattern contract. Names are not frozen; keep crate-visible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PatternContract {
    pub id: String,
    pub version: u32,
    pub legality: Legality,
    pub facets: Facets,
    pub provenance: ContractProvenance,
}

/// Progressive-disclosure card. Negative scope is mandatory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndexCard {
    pub id: String,
    pub semantic_role: String,
    pub description: String,
    pub input_family: String,
    pub output_family: String,
    pub effect_class: BTreeSet<EffectCategory>,
    pub human_gate: bool,
    pub open_world: bool,
    pub positive_scope: Vec<String>,
    pub negative_scope: Vec<String>,
}

/// Failures of derivation or card validation. Not a public language error.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum PatternError {
    /// The source did not parse; no contract is derived from comments.
    #[error("workflow source does not parse")]
    Unparseable,
    /// An [`IndexCard`] omitted WHAT IT IS NOT.
    #[error("index card `{id}` is missing negative_scope")]
    MissingNegativeScope { id: String },
    /// A refused construct was asked to be emitted.
    #[error("refused construct is not emitted")]
    Refused,
}

/// Parse with the same schema door Compile uses, then derive facets.
pub(crate) fn derive_from_source(source: &str) -> Result<PatternContract, PatternError> {
    let wf = nika_schema::parse(source, FileId::new(0), ParseMode::Strict)
        .map_err(|_| PatternError::Unparseable)?;
    let report = nika_check::check(&wf);
    Ok(contract_from_ast(&wf, &report))
}

/// Derive from an already-parsed workflow and its Check report.
pub(crate) fn contract_from_ast(wf: &RawWorkflow, report: &CheckReport) -> PatternContract {
    let task_ids = task_ids_from_map(wf);
    let mut effect_categories = effects_from_ast(wf);
    effect_categories.extend(effects_from_check(report));
    let legality = if report.permits.partial.any() {
        Legality::ExpressibleWithLoss
    } else {
        Legality::Expressible
    };
    let id = wf
        .workflow
        .as_ref()
        .map_or_else(|| "unnamed".to_owned(), |name| name.value.clone());
    PatternContract {
        id,
        version: PATTERN_CONTRACT_VERSION,
        legality,
        facets: Facets {
            task_ids,
            effect_categories,
        },
        provenance: ContractProvenance::derived_body(),
    }
}

/// [`IndexCard`] for an expressible contract. Refused constructs are not emitted.
pub(crate) fn index_card_for(contract: &PatternContract) -> Result<IndexCard, PatternError> {
    let Some(contract) = emit_contract(contract) else {
        return Err(PatternError::Refused);
    };
    let human_gate = contract
        .facets
        .effect_categories
        .contains(&EffectCategory::HumanGate);
    let card = IndexCard {
        id: contract.id.clone(),
        semantic_role: contract.id.clone(),
        description: derived_description(&contract.facets),
        input_family: String::new(),
        output_family: String::new(),
        effect_class: contract.facets.effect_categories.clone(),
        human_gate,
        open_world: false,
        positive_scope: derived_positive_scope(&contract.facets),
        negative_scope: derived_negative_scope(&contract.facets),
    };
    validate_index_card(&card)?;
    Ok(card)
}

/// Refuse a card that does not say WHAT IT IS NOT.
pub(crate) fn validate_index_card(card: &IndexCard) -> Result<(), PatternError> {
    if has_negative_scope(card) {
        Ok(())
    } else {
        Err(PatternError::MissingNegativeScope {
            id: card.id.clone(),
        })
    }
}

/// The reserved miss card, with mandatory negative scope filled.
#[must_use]
pub(crate) fn none_compatible_pattern_card() -> IndexCard {
    IndexCard {
        id: NONE_CARD_ID.to_owned(),
        semantic_role: "no compatible pattern".to_owned(),
        description: "No registered pattern is compatible with this intent.".to_owned(),
        input_family: String::new(),
        output_family: String::new(),
        effect_class: BTreeSet::new(),
        human_gate: false,
        open_world: true,
        positive_scope: vec!["no compatible pattern".to_owned()],
        negative_scope: vec![
            "not a registered pattern".to_owned(),
            "not a fifth verb".to_owned(),
            "not compiler-granted authority".to_owned(),
            "not a public YAML pattern key".to_owned(),
            "not an SDK Pattern noun".to_owned(),
        ],
    }
}

/// Either reserved miss identifier.
#[must_use]
pub(crate) fn is_none_card_id(id: &str) -> bool {
    id == NONE_CARD_ID || id == NO_COMPATIBLE_PATTERN_ID
}

/// Compiler never emits a refused construct.
#[must_use]
pub(crate) fn emit_contract(contract: &PatternContract) -> Option<&PatternContract> {
    match contract.legality {
        Legality::RefusedPermanent => None,
        Legality::Expressible | Legality::ExpressibleWithLoss | Legality::DeferredBySpec => {
            Some(contract)
        }
    }
}

/// Authored record of a construct this compiler will never emit.
#[must_use]
pub(crate) fn refused_public_language() -> PatternContract {
    PatternContract {
        id: "refused-public-pattern-key".to_owned(),
        version: PATTERN_CONTRACT_VERSION,
        legality: Legality::RefusedPermanent,
        facets: Facets::default(),
        provenance: ContractProvenance::authored(),
    }
}

/// Authored record of a construct the spec has not named yet (JOIN algebra).
#[must_use]
pub(crate) fn deferred_construct() -> PatternContract {
    PatternContract {
        id: "deferred-by-spec".to_owned(),
        version: PATTERN_CONTRACT_VERSION,
        legality: Legality::DeferredBySpec,
        facets: Facets::default(),
        provenance: ContractProvenance::authored(),
    }
}

/// Observational derivation. CREATE/EDIT outcomes do not change.
pub(crate) fn observe(source: &str, wf: &RawWorkflow, report: &CheckReport) {
    let _ = derive_from_source(source);
    let contract = contract_from_ast(wf, report);
    let _ = index_card_for(&contract);
    let _ = validate_index_card(&none_compatible_pattern_card());
    let _ = emit_contract(&refused_public_language());
    let _ = emit_contract(&deferred_construct());
    let _ = is_none_card_id(NONE_CARD_ID);
    let _ = is_none_card_id(NO_COMPATIBLE_PATTERN_ID);
}

fn task_ids_from_map(wf: &RawWorkflow) -> BTreeSet<String> {
    wf.tasks
        .iter()
        .map(|task| task.value.id.value.clone())
        .collect()
}

fn effects_from_ast(wf: &RawWorkflow) -> BTreeSet<EffectCategory> {
    let mut effects = BTreeSet::new();
    for task in &wf.tasks {
        match &task.value.action {
            RawAction::Exec(_) => {
                effects.insert(EffectCategory::Exec);
            }
            RawAction::Infer(_) => {
                effects.insert(EffectCategory::Llm);
            }
            RawAction::Invoke(invoke) => {
                if let RawInvokeTarget::Tool(tool) = &invoke.target {
                    classify_tool(&tool.value, &mut effects);
                }
            }
            RawAction::Agent(agent) => {
                effects.insert(EffectCategory::Llm);
                for tool in agent
                    .tools
                    .iter()
                    .filter(|tool| !tool.value.starts_with('!'))
                {
                    if !tool.value.contains('*') {
                        classify_tool(&tool.value, &mut effects);
                    }
                }
            }
            // Future verb variants stay unknown: never guessed, never emitted.
            _ => {}
        }
    }
    effects
}

fn effects_from_check(report: &CheckReport) -> BTreeSet<EffectCategory> {
    let mut effects = BTreeSet::new();
    let needed = &report.permits.needed;
    if needed.net.as_ref().is_some_and(|net| !net.http.is_empty()) || report.permits.partial.net {
        effects.insert(EffectCategory::Net);
    }
    if needed
        .tools
        .as_ref()
        .is_some_and(|tools| tools.iter().any(|tool| tool == FETCH_TOOL))
    {
        effects.insert(EffectCategory::Net);
    }
    if needed.allows_exec() {
        effects.insert(EffectCategory::Exec);
    }
    if let Some(fs) = &needed.fs {
        if !fs.read.is_empty() {
            effects.insert(EffectCategory::FsRead);
        }
        if !fs.write.is_empty() {
            effects.insert(EffectCategory::FsWrite);
        }
    }
    if needed
        .tools
        .as_ref()
        .is_some_and(|tools| tools.iter().any(|tool| tool == PROMPT_TOOL))
    {
        effects.insert(EffectCategory::HumanGate);
    }
    effects
}

fn classify_tool(tool: &str, effects: &mut BTreeSet<EffectCategory>) {
    match tool {
        FETCH_TOOL => {
            effects.insert(EffectCategory::Net);
        }
        READ_TOOL | GLOB_TOOL | GREP_TOOL => {
            effects.insert(EffectCategory::FsRead);
        }
        WRITE_TOOL | EDIT_TOOL => {
            effects.insert(EffectCategory::FsWrite);
        }
        PROMPT_TOOL => {
            effects.insert(EffectCategory::HumanGate);
        }
        _ => {}
    }
}

fn has_negative_scope(card: &IndexCard) -> bool {
    card.negative_scope
        .iter()
        .any(|entry| !entry.trim().is_empty())
}

fn derived_description(facets: &Facets) -> String {
    if facets.effect_categories.is_empty() {
        "Derived four-verb workflow with no observed effects.".to_owned()
    } else {
        "Derived four-verb workflow with observed effects.".to_owned()
    }
}

fn derived_positive_scope(facets: &Facets) -> Vec<String> {
    if facets.effect_categories.is_empty() {
        vec!["pure compute".to_owned()]
    } else {
        facets
            .effect_categories
            .iter()
            .map(|effect| format!("{effect:?}").to_ascii_lowercase())
            .collect()
    }
}

fn derived_negative_scope(facets: &Facets) -> Vec<String> {
    let mut scope = vec![
        "not a fifth verb".to_owned(),
        "not a public YAML pattern key".to_owned(),
        "not compiler-granted authority".to_owned(),
        "not an SDK Pattern noun".to_owned(),
    ];
    if !facets.effect_categories.contains(&EffectCategory::Net) {
        scope.push("not network egress".to_owned());
    }
    if !facets.effect_categories.contains(&EffectCategory::Exec) {
        scope.push("not process execution".to_owned());
    }
    scope
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMENT_FETCH: &str = r#"
nika: comment-fetch
# this comment names nika:fetch and must not become effects.net
permits: {}
tasks:
  think:
    infer:
      prompt: "hello"
"#;

    const PERMITS_FS: &str = r#"
nika: permits-are-not-tasks
permits:
  fs:
    read: ["./notes/**"]
  tools: ["nika:jq"]
tasks:
  echo:
    invoke:
      tool: nika:jq
      args:
        input: "1"
        expression: "."
"#;

    const REAL_FETCH: &str = r#"
nika: real-fetch
permits:
  tools: ["nika:fetch"]
  net:
    http: ["example.com"]
tasks:
  lookup:
    invoke:
      tool: nika:fetch
      args:
        url: "https://example.com/"
"#;

    const PROMPT_MENTIONS_FETCH: &str = r#"
nika: prompt-mentions-fetch
permits: {}
tasks:
  think:
    infer:
      prompt: "please call nika:fetch for me"
"#;

    const DECLARED_FETCH_PERMIT_ONLY: &str = r#"
nika: declared-fetch-permit
permits:
  tools: ["nika:fetch"]
  net:
    http: ["example.com"]
tasks:
  think:
    infer:
      prompt: "hello"
"#;

    #[test]
    fn a_comment_naming_nika_fetch_is_not_effects_net() {
        let contract = derive_from_source(COMMENT_FETCH).expect("commented source still parses");
        assert!(
            !contract
                .facets
                .effect_categories
                .contains(&EffectCategory::Net),
            "a YAML comment that names nika:fetch must not become effects.net: {contract:?}"
        );
        assert_eq!(
            contract.facets.task_ids,
            BTreeSet::from(["think".to_owned()])
        );
        assert_eq!(
            contract.provenance.effect_categories,
            FieldProvenance::Derived
        );
        assert_eq!(contract.provenance.task_ids, FieldProvenance::Derived);
    }

    #[test]
    fn a_permits_fs_key_is_not_a_task_id() {
        let contract = derive_from_source(PERMITS_FS).expect("parses");
        assert!(
            !contract.facets.task_ids.contains("fs"),
            "permits.fs is an envelope key, not a task id: {contract:?}"
        );
        assert!(!contract.facets.task_ids.contains("permits"));
        assert!(!contract.facets.task_ids.contains("tools"));
        assert_eq!(
            contract.facets.task_ids,
            BTreeSet::from(["echo".to_owned()])
        );
    }

    #[test]
    fn a_real_nika_fetch_invoke_sets_net() {
        let contract = derive_from_source(REAL_FETCH).expect("parses");
        assert!(
            contract
                .facets
                .effect_categories
                .contains(&EffectCategory::Net),
            "a real nika:fetch invoke must set net: {contract:?}"
        );
        assert_eq!(
            contract.facets.task_ids,
            BTreeSet::from(["lookup".to_owned()])
        );
        assert_eq!(contract.legality, Legality::Expressible);
    }

    #[test]
    fn none_card_without_negative_scope_is_rejected() {
        let mut card = none_compatible_pattern_card();
        assert!(is_none_card_id(&card.id));
        validate_index_card(&card).expect("the reserved miss card carries negative_scope");
        card.negative_scope.clear();
        let err = validate_index_card(&card).expect_err("empty negative_scope must fail");
        assert!(
            matches!(err, PatternError::MissingNegativeScope { ref id } if is_none_card_id(id)),
            "{err:?}"
        );

        let alias = IndexCard {
            id: NO_COMPATIBLE_PATTERN_ID.to_owned(),
            negative_scope: Vec::new(),
            ..none_compatible_pattern_card()
        };
        let err = validate_index_card(&alias).expect_err("alias miss also requires negative_scope");
        assert!(matches!(
            err,
            PatternError::MissingNegativeScope { id } if id == NO_COMPATIBLE_PATTERN_ID
        ));
    }

    #[test]
    fn prompt_text_naming_fetch_is_not_effects_net() {
        let contract = derive_from_source(PROMPT_MENTIONS_FETCH).expect("parses");
        assert!(
            !contract
                .facets
                .effect_categories
                .contains(&EffectCategory::Net),
            "prompt text is not an invoke: {contract:?}"
        );
        assert!(
            contract
                .facets
                .effect_categories
                .contains(&EffectCategory::Llm)
        );
    }

    #[test]
    fn a_declared_fetch_permit_without_invoke_is_not_net() {
        let contract = derive_from_source(DECLARED_FETCH_PERMIT_ONLY).expect("parses");
        assert!(
            !contract
                .facets
                .effect_categories
                .contains(&EffectCategory::Net),
            "declared authority is not an effect: {contract:?}"
        );
    }

    #[test]
    fn a_real_exec_verb_sets_exec_and_is_not_confused_with_permits() {
        let source = r#"
nika: real-exec
permits:
  exec: ["echo"]
tasks:
  run:
    exec:
      command: ["echo", "ok"]
"#;
        let contract = derive_from_source(source).expect("parses");
        assert!(
            contract
                .facets
                .effect_categories
                .contains(&EffectCategory::Exec)
        );
        assert_eq!(contract.facets.task_ids, BTreeSet::from(["run".to_owned()]));
        assert!(!contract.facets.task_ids.contains("exec"));
    }

    #[test]
    fn refused_constructs_are_not_emitted() {
        let refused = refused_public_language();
        assert_eq!(refused.legality, Legality::RefusedPermanent);
        assert!(emit_contract(&refused).is_none());
        assert!(matches!(
            index_card_for(&refused),
            Err(PatternError::Refused)
        ));
        let deferred = deferred_construct();
        assert_eq!(deferred.legality, Legality::DeferredBySpec);
        assert!(emit_contract(&deferred).is_some());
    }

    #[test]
    fn derived_index_card_carries_negative_scope() {
        let contract = derive_from_source(COMMENT_FETCH).expect("parses");
        let card = index_card_for(&contract).expect("expressible");
        assert!(!card.negative_scope.is_empty());
        assert!(
            card.negative_scope
                .iter()
                .any(|entry| entry.contains("not network egress"))
        );
        assert!(!card.open_world);
    }

    #[test]
    fn classify_and_route_template_task_ids_come_from_the_tasks_map() {
        let Some(source) = nika_pack::template("classify-and-route") else {
            panic!("embedded classify-and-route template");
        };
        let contract = derive_from_source(source).expect("template parses");
        assert!(
            !contract.facets.task_ids.contains("fs"),
            "envelope keys must not appear as task ids"
        );
        assert!(contract.facets.task_ids.contains("extract_facts"));
        assert!(contract.facets.task_ids.contains("decide"));
        assert!(
            !contract
                .facets
                .effect_categories
                .contains(&EffectCategory::Net)
        );
        assert!(
            contract
                .facets
                .effect_categories
                .contains(&EffectCategory::Llm)
        );
        assert_eq!(contract.provenance.id, FieldProvenance::Derived);
        assert_eq!(contract.provenance.version, FieldProvenance::Authored);
    }
}
