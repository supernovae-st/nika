//! The workflow's REQUIREMENTS (E-REQ) — declaration facts only:
//! models per task · secrets (never values) · config reads vs defines ·
//! required inputs. Presence stays the caller's check.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use nika_schema::expression::NamespaceRef;
use nika_schema::raw::{RawAction, RawWorkflow};

use super::flow::{action_effect_fields, collect_json_strings, prompt_system_fields, refs_in_str};
use nika_schema::types::{SecretSource, VarDecl, WhenGate};

/// One model the run will call, with the tasks that call it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelRequirement {
    /// The combined `<provider>/<name>` form (task override ?? envelope).
    pub model: String,
    /// The infer/agent task ids that resolve to this model.
    pub tasks: Vec<String>,
}

/// One declared secret — the declaration facts, never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecretRequirement {
    pub name: String,
    /// `env` · `vault` · `file` (the spec's closed enum · serde lowercase).
    pub source: SecretSource,
    /// The lookup key (env-source: the variable name).
    pub key: String,
}

/// What this workflow needs from its caller — additive on the report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Requirements {
    /// Models the run will call (infer/agent · task ?? envelope).
    pub models: Vec<ModelRequirement>,
    /// Declared `secrets:` (facts only — presence is the caller's check).
    pub secrets: Vec<SecretRequirement>,
    /// `${{ config.X }}` names the BODY reads — the requirements.
    pub config_reads: Vec<String>,
    /// Envelope `config:` keys — configuration (covers a matching read).
    pub config_defined: Vec<String>,
    /// `inputs:` that are `required: true` with no `default:`.
    pub inputs_required: Vec<String>,
}

/// Collect the requirements (total — a half-broken workflow still
/// states what it declares).
pub(crate) fn collect(wf: &RawWorkflow) -> Requirements {
    let envelope_model = wf.model.as_ref().map(|m| m.value.clone());
    let mut models: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut config_reads: BTreeSet<String> = BTreeSet::new();

    for task in &wf.tasks {
        // Model resolution: the verb's own override ?? the envelope.
        let task_model = match &task.value.action {
            RawAction::Infer(a) => Some(a.model.as_ref().map(|m| m.value.clone())),
            RawAction::Agent(a) => Some(a.model.as_ref().map(|m| m.value.clone())),
            _ => None,
        };
        if let Some(model) = task_model.and_then(|m| m.or_else(|| envelope_model.clone())) {
            models
                .entry(model)
                .or_default()
                .push(task.value.id.value.clone());
        }

        config_reads_of_task(task, &mut config_reads);
    }

    // The envelope `outputs:` return contract — its ${{ }} refs are body
    // reads too (both declaration forms carry an expression string).
    for (_, decl) in &wf.outputs {
        match decl {
            nika_schema::types::OutputDecl::Untyped(expr) => {
                collect_config_reads(&expr.value, &mut config_reads);
            }
            nika_schema::types::OutputDecl::Typed { value, .. } => {
                collect_config_reads(&value.value, &mut config_reads);
            }
        }
    }

    Requirements {
        models: models
            .into_iter()
            .map(|(model, tasks)| ModelRequirement { model, tasks })
            .collect(),
        secrets: wf
            .secrets
            .iter()
            .map(|(name, decl)| SecretRequirement {
                name: name.value.clone(),
                source: decl.value.source,
                key: decl.value.key.clone(),
            })
            .collect(),
        config_reads: config_reads.into_iter().collect(),
        config_defined: wf.config.iter().map(|(k, _)| k.value.clone()).collect(),
        inputs_required: wf
            .inputs
            .iter()
            .filter(|(_, decl)| {
                matches!(
                    decl,
                    VarDecl::Typed {
                        required: true,
                        default: None,
                        ..
                    }
                )
            })
            .map(|(name, _)| name.value.clone())
            .collect(),
    }
}

/// Every `${{ config.X }}` read across ONE task's whole surface —
/// action fields (prompts included) · `with` · when-CEL · `output` ·
/// `for_each` · `on_error` recover · `on_finally` cleanups.
fn config_reads_of_task(
    task: &nika_schema::Spanned<nika_schema::raw::RawTask>,
    config_reads: &mut BTreeSet<String>,
) {
    // Env reads: every template-bearing string of the task surface,
    // through the REAL extractor (the same path the analyzer uses).
    for text in task_template_fields(&task.value.action) {
        collect_config_reads(text, config_reads);
    }
    for (_, v) in &task.value.with {
        for text in collect_json_strings(&v.value) {
            collect_config_reads(text, config_reads);
        }
    }
    if let Some(WhenGate::Expr(cel)) = task.value.when.as_ref().map(|g| &g.value) {
        collect_config_reads(cel, config_reads);
    }
    for (_, expr) in &task.value.output {
        collect_config_reads(&expr.value, config_reads);
    }
    // `for_each:` — the collection expression (or list literals) can
    // read env like any other template surface.
    if let Some(fe) = &task.value.for_each {
        match &fe.value {
            nika_schema::raw::ForEachValue::Expression(expr) => {
                collect_config_reads(expr, config_reads);
            }
            nika_schema::raw::ForEachValue::List(list) => {
                for text in collect_json_strings(list) {
                    collect_config_reads(text, config_reads);
                }
            }
            #[allow(
                clippy::unreachable,
                reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
            )]
            other => unreachable!("unknown for_each form: {other:?}"),
        }
    }
    // `on_error: recover:` — the recovery value substitutes an output;
    // its templates are body reads like any other.
    if let Some(nika_schema::types::OnErrorAction::Recover(value)) =
        task.value.on_error.as_ref().map(|o| &o.value.action)
    {
        for text in collect_json_strings(&value.value) {
            collect_config_reads(text, config_reads);
        }
    }
    // `on_finally:` cleanups carry full actions (and gates) of their own.
    for cleanup in &task.value.on_finally {
        for text in task_template_fields(&cleanup.value.action) {
            collect_config_reads(text, config_reads);
        }
        if let Some(WhenGate::Expr(cel)) = cleanup.value.when.as_ref().map(|g| &g.value) {
            collect_config_reads(cel, config_reads);
        }
    }
}

/// Every template-bearing string of one action — flow's effect fields
/// (command · stdin · exec env · invoke args) PLUS the prompts (a config
/// read in a prompt is as much a requirement as one in a command).
fn task_template_fields(action: &RawAction) -> Vec<&str> {
    let mut fields = action_effect_fields(action);
    match action {
        RawAction::Infer(a) => {
            fields.extend(prompt_system_fields(&a.prompt.value, a.system.as_ref()));
        }
        RawAction::Agent(a) => {
            fields.extend(prompt_system_fields(&a.prompt.value, a.system.as_ref()));
        }
        RawAction::Exec(_) | RawAction::Invoke(_) => {}
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and checker ship together; fail loud beats silently-wrong output"
        )]
        other => unreachable!("unknown action: {other:?}"),
    }
    fields
}

fn collect_config_reads(text: &str, out: &mut BTreeSet<String>) {
    for r in refs_in_str(text) {
        if let NamespaceRef::Config(name) = r
            && !name.is_empty()
        {
            out.insert(name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_schema::parser::{ParseMode, parse};
    use nika_schema::source::FileId;

    fn wf_of(yaml: &str) -> RawWorkflow {
        parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses")
    }

    #[test]
    fn requirements_state_the_full_caller_contract() {
        let wf = wf_of(
            r#"
nika: v1
workflow:
  id: req-probe
model: anthropic/claude-sonnet-4-6
inputs:
  target_url: { type: string, required: true }
  region: { type: string, required: true, default: "eu" }
secrets:
  gh_token:
    source: env
    key: GITHUB_TOKEN
  vault_pass:
    source: vault
    key: prod/db-pass
config:
  REGION: { type: string, default: "eu-west-1" }
tasks:
  fetch:
    invoke:
      tool: "nika:fetch"
      args: { url: "https://api.example.com/${{ config.GITHUB_ORG }}" }
  digest:
    after: { fetch: success }
    infer:
      prompt: "Summarize for ${{ config.REGION }}"
  local_pass:
    after: { fetch: success }
    for_each: "${{ config.SHARDS }}"
    on_finally:
      - exec: { command: ["echo", "${{ config.CLEANUP_FLAG }}"] }
    infer:
      model: ollama/qwen3
      prompt: "rank"
outputs:
  report: "${{ config.REPORT_PATH }}"
"#,
        );
        let req = collect(&wf);
        assert_eq!(
            req.models,
            vec![
                ModelRequirement {
                    model: "anthropic/claude-sonnet-4-6".into(),
                    tasks: vec!["digest".into()],
                },
                ModelRequirement {
                    model: "ollama/qwen3".into(),
                    tasks: vec!["local_pass".into()],
                },
            ]
        );
        assert_eq!(req.secrets.len(), 2);
        assert_eq!(req.secrets[0].name, "gh_token");
        assert_eq!(req.secrets[0].source, SecretSource::Env);
        assert_eq!(req.secrets[0].key, "GITHUB_TOKEN");
        assert_eq!(req.secrets[1].source, SecretSource::Vault);
        // The split IS the semantics: REGION is read AND defined ·
        // GITHUB_ORG is read-only (the caller requirement).
        assert_eq!(
            req.config_reads,
            vec![
                "CLEANUP_FLAG",
                "GITHUB_ORG",
                "REGION",
                "REPORT_PATH",
                "SHARDS"
            ]
        );
        assert_eq!(req.config_defined, vec!["REGION"]);
        // required + defaulted is NOT a requirement.
        assert_eq!(req.inputs_required, vec!["target_url"]);
    }

    #[test]
    fn an_invoke_only_workflow_needs_no_model_and_says_so() {
        let wf = wf_of(
            "nika: v1\nworkflow:\n  id: none\ntasks:\n  a:\n    invoke: { tool: \"nika:uuid\" }\n",
        );
        let req = collect(&wf);
        assert!(req.models.is_empty());
        assert!(req.config_reads.is_empty());
    }
}
