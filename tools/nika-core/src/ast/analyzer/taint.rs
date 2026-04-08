// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Compile-time taint analysis for Nika Shield.
//!
//! Propagates `TrustLevel` through the workflow DAG based on verb types
//! and data flow. Generates warnings for risky patterns.
//!
//! This runs at `nika check --security` time (compile-time, no I/O).

use std::collections::HashMap;

use crate::ast::analyzed::{AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow};
use crate::binding::BindingSource;
use crate::trust::{builtin_output_trust, InvocationSource, TrustLevel};

// NOTE: AnalyzedTask.id is TaskId(u32), an interned index.
// We use task.name (String) as the key in trust_map for human-readable output.

// ── TaintAnalyzer ───────────────────────────────────────────────────────────

/// Compile-time taint analysis engine.
///
/// Computes a trust level for each task's output by walking the DAG in
/// topological order and propagating trust through verb-specific rules.
pub struct TaintAnalyzer;

impl TaintAnalyzer {
    /// Analyze the workflow DAG and return trust levels + warnings.
    ///
    /// `workflow.tasks` is already topologically sorted (Phase 2 guarantee),
    /// so we process dependencies before dependents.
    pub fn analyze(workflow: &AnalyzedWorkflow, source: InvocationSource) -> TaintReport {
        let mut trust_map: HashMap<String, TrustLevel> = HashMap::new();

        // Pass 1: Compute trust for each task
        for task in &workflow.tasks {
            let task_id = task.name.clone();
            let input_trust = Self::compute_input_trust(task, &trust_map, source);
            let output_trust = Self::compute_output_trust(task, input_trust);
            trust_map.insert(task_id, output_trust);
        }

        // Pass 2: Generate warnings
        let warnings = Self::generate_warnings(workflow, &trust_map);

        TaintReport {
            trust_map,
            warnings,
        }
    }

    /// Compute the trust of a task's inputs by merging all `with:` binding sources.
    fn compute_input_trust(
        task: &AnalyzedTask,
        trust_map: &HashMap<String, TrustLevel>,
        source: InvocationSource,
    ) -> TrustLevel {
        if task.with_spec.is_empty() {
            // No bindings — pure literal task, fully trusted
            return TrustLevel::Trusted;
        }

        let mut trust = TrustLevel::Trusted;

        for entry in task.with_spec.values() {
            let binding_trust = match &entry.source.source {
                BindingSource::Task(task_id) => trust_map
                    .get(task_id.as_ref())
                    .copied()
                    .unwrap_or(TrustLevel::Trusted),
                BindingSource::Input(_) => source.input_trust(),
                BindingSource::Context(_) => TrustLevel::Trusted,
                BindingSource::Vault { .. } => TrustLevel::Trusted,
                BindingSource::Env(_) => TrustLevel::Trusted,
                BindingSource::LoopVar(_) => {
                    // Loop vars inherit from the for_each source.
                    // In practice the for_each source is already resolved
                    // in the parent task, so we use Trusted as default.
                    TrustLevel::Trusted
                }
            };

            // Default value `??` does not elevate trust — conservative
            trust = trust.merge(binding_trust);
        }

        trust
    }

    /// Compute the trust of a task's output based on its verb and input trust.
    fn compute_output_trust(task: &AnalyzedTask, input_trust: TrustLevel) -> TrustLevel {
        match &task.action {
            // Fetch responses are always untrusted (external HTTP content)
            AnalyzedTaskAction::Fetch(_) => TrustLevel::Untrusted,

            // Shell stdout is always untrusted (external process output)
            AnalyzedTaskAction::Exec(_) => TrustLevel::Untrusted,

            // Invoke: builtins via fail-closed dispatch table, MCP tools always Untrusted.
            AnalyzedTaskAction::Invoke(inv) => {
                if inv.tool.starts_with("nika:") {
                    // P0-4 hardening: unknown builtins fall closed to Untrusted.
                    builtin_output_trust(&inv.tool, input_trust)
                } else {
                    // MCP tool result = untrusted (external server)
                    TrustLevel::Untrusted
                }
            }

            // Infer: output depends on whether inputs were tainted
            AnalyzedTaskAction::Infer(_) => {
                if input_trust.is_untrusted() {
                    TrustLevel::ModelTainted
                } else {
                    TrustLevel::ModelGenerated
                }
            }

            // Agent: always ModelTainted (processes tool results which are untrusted)
            AnalyzedTaskAction::Agent(_) => TrustLevel::ModelTainted,
        }
    }

    /// Generate warnings for risky data flow patterns.
    fn generate_warnings(
        workflow: &AnalyzedWorkflow,
        trust_map: &HashMap<String, TrustLevel>,
    ) -> Vec<TaintWarning> {
        let mut warnings = Vec::new();

        for task in &workflow.tasks {
            let task_id = task.name.as_str();
            let input_trust = Self::compute_input_trust_from_map(task, trust_map);

            // TAINT-001: Untrusted -> shell
            if matches!(task.action, AnalyzedTaskAction::Exec(_)) && input_trust.is_untrusted() {
                if let Some(source) = Self::find_untrusted_source(task, trust_map) {
                    warnings.push(TaintWarning::UntrustedToExec {
                        source_task: source,
                        exec_task: task_id.to_string(),
                    });
                }
            }

            // TAINT-002: Untrusted -> agent with dangerous tools
            if let AnalyzedTaskAction::Agent(agent) = &task.action {
                if input_trust.is_untrusted() {
                    let dangerous: Vec<String> = agent
                        .tools
                        .iter()
                        .filter(|t| matches!(t.as_str(), "nika:write" | "nika:edit" | "nika:run"))
                        .cloned()
                        .collect();
                    if !dangerous.is_empty() {
                        warnings.push(TaintWarning::UntrustedToAgentTools {
                            source_task: Self::find_untrusted_source(task, trust_map)
                                .unwrap_or_default(),
                            agent_task: task_id.to_string(),
                            dangerous_tools: dangerous,
                        });
                    }
                }
            }

            // TAINT-003: Untrusted -> infer without structured schema
            if matches!(task.action, AnalyzedTaskAction::Infer(_))
                && input_trust.is_untrusted()
                && task.structured.is_none()
            {
                if let Some(source) = Self::find_untrusted_source(task, trust_map) {
                    warnings.push(TaintWarning::UntrustedToInferNoSchema {
                        source_task: source,
                        infer_task: task_id.to_string(),
                    });
                }
            }

            // TAINT-004: for_each over untrusted data with high concurrency
            if let Some(ref for_each) = task.for_each {
                if input_trust.is_untrusted() {
                    if let Some(crate::ast::templatable::Templatable::Value(conc)) =
                        &for_each.concurrency
                    {
                        if *conc > 5 {
                            warnings.push(TaintWarning::UntrustedForEachAmplification {
                                source_task: Self::find_untrusted_source(task, trust_map)
                                    .unwrap_or_default(),
                                foreach_task: task_id.to_string(),
                                concurrency: *conc as usize,
                            });
                        }
                    }
                }
            }

            // TAINT-005: Untrusted -> fetch URL (SSRF via injection)
            if matches!(task.action, AnalyzedTaskAction::Fetch(_)) && input_trust.is_untrusted() {
                if let Some(source) = Self::find_untrusted_source(task, trust_map) {
                    warnings.push(TaintWarning::UntrustedToFetchUrl {
                        source_task: source,
                        fetch_task: task_id.to_string(),
                    });
                }
            }

            // TAINT-006: when: condition depends on untrusted data
            if task.when.is_some() && input_trust.is_untrusted() {
                if let Some(source) = Self::find_untrusted_source(task, trust_map) {
                    warnings.push(TaintWarning::UntrustedWhenCondition {
                        source_task: source,
                        conditional_task: task_id.to_string(),
                    });
                }
            }
        }

        warnings
    }

    /// Compute input trust from the map (used in warning generation).
    fn compute_input_trust_from_map(
        task: &AnalyzedTask,
        trust_map: &HashMap<String, TrustLevel>,
    ) -> TrustLevel {
        let mut trust = TrustLevel::Trusted;
        for entry in task.with_spec.values() {
            if let BindingSource::Task(task_id) = &entry.source.source {
                if let Some(&dep_trust) = trust_map.get(task_id.as_ref()) {
                    trust = trust.merge(dep_trust);
                }
            }
        }
        trust
    }

    /// Find the first untrusted source task for warning messages.
    fn find_untrusted_source(
        task: &AnalyzedTask,
        trust_map: &HashMap<String, TrustLevel>,
    ) -> Option<String> {
        for entry in task.with_spec.values() {
            if let BindingSource::Task(task_id) = &entry.source.source {
                if let Some(&trust) = trust_map.get(task_id.as_ref()) {
                    if trust.is_untrusted() {
                        return Some(task_id.to_string());
                    }
                }
            }
        }
        None
    }
}

// ── TaintReport ─────────────────────────────────────────────────────────────

/// Result of compile-time taint analysis.
#[derive(Debug)]
pub struct TaintReport {
    /// TrustLevel for each task's output: task_id -> trust.
    pub trust_map: HashMap<String, TrustLevel>,
    /// Risky patterns detected.
    pub warnings: Vec<TaintWarning>,
}

impl TaintReport {
    /// Count tasks at each trust level.
    pub fn trust_summary(&self) -> HashMap<TrustLevel, usize> {
        let mut summary = HashMap::new();
        for trust in self.trust_map.values() {
            *summary.entry(*trust).or_insert(0) += 1;
        }
        summary
    }

    /// True if no warnings were generated.
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }
}

// ── TaintWarning ────────────────────────────────────────────────────────────

/// Risky data flow patterns detected by taint analysis.
#[derive(Debug, Clone)]
pub enum TaintWarning {
    /// TAINT-001: Untrusted data flows to shell command.
    UntrustedToExec {
        source_task: String,
        exec_task: String,
    },
    /// TAINT-002: Agent with dangerous tools processes untrusted data.
    UntrustedToAgentTools {
        source_task: String,
        agent_task: String,
        dangerous_tools: Vec<String>,
    },
    /// TAINT-003: Untrusted data flows to infer without structured schema.
    UntrustedToInferNoSchema {
        source_task: String,
        infer_task: String,
    },
    /// TAINT-004: for_each over untrusted data with high concurrency.
    UntrustedForEachAmplification {
        source_task: String,
        foreach_task: String,
        concurrency: usize,
    },
    /// TAINT-005: Fetch URL built from untrusted data.
    UntrustedToFetchUrl {
        source_task: String,
        fetch_task: String,
    },
    /// TAINT-006: when: condition depends on untrusted data.
    UntrustedWhenCondition {
        source_task: String,
        conditional_task: String,
    },
}

impl TaintWarning {
    /// Warning code (TAINT-001 through TAINT-006).
    pub fn code(&self) -> &'static str {
        match self {
            Self::UntrustedToExec { .. } => "TAINT-001",
            Self::UntrustedToAgentTools { .. } => "TAINT-002",
            Self::UntrustedToInferNoSchema { .. } => "TAINT-003",
            Self::UntrustedForEachAmplification { .. } => "TAINT-004",
            Self::UntrustedToFetchUrl { .. } => "TAINT-005",
            Self::UntrustedWhenCondition { .. } => "TAINT-006",
        }
    }

    /// Human-readable description.
    pub fn message(&self) -> String {
        match self {
            Self::UntrustedToExec {
                source_task,
                exec_task,
            } => format!("Untrusted data from '{source_task}' flows to exec task '{exec_task}'"),
            Self::UntrustedToAgentTools {
                source_task,
                agent_task,
                dangerous_tools,
            } => format!(
                "Untrusted data from '{source_task}' flows to agent '{agent_task}' \
                 with dangerous tools: {}",
                dangerous_tools.join(", ")
            ),
            Self::UntrustedToInferNoSchema {
                source_task,
                infer_task,
            } => format!(
                "Untrusted data from '{source_task}' flows to infer '{infer_task}' \
                 without structured: schema"
            ),
            Self::UntrustedForEachAmplification {
                source_task,
                foreach_task,
                concurrency,
            } => format!(
                "for_each in '{foreach_task}' over untrusted data from '{source_task}' \
                 with concurrency={concurrency} (risk: amplification attack)"
            ),
            Self::UntrustedToFetchUrl {
                source_task,
                fetch_task,
            } => format!(
                "Untrusted data from '{source_task}' may flow to fetch URL in '{fetch_task}' \
                 (risk: SSRF via injection)"
            ),
            Self::UntrustedWhenCondition {
                source_task,
                conditional_task,
            } => format!(
                "when: condition in '{conditional_task}' depends on untrusted data \
                 from '{source_task}' (risk: conditional bypass)"
            ),
        }
    }

    /// Sink task name for the warning — the task that consumes the
    /// untrusted data. Used by lint rules to attach the warning to a
    /// specific task ID in the user's workflow.
    pub fn task_name(&self) -> Option<&str> {
        Some(match self {
            Self::UntrustedToExec { exec_task, .. } => exec_task.as_str(),
            Self::UntrustedToAgentTools { agent_task, .. } => agent_task.as_str(),
            Self::UntrustedToInferNoSchema { infer_task, .. } => infer_task.as_str(),
            Self::UntrustedForEachAmplification { foreach_task, .. } => foreach_task.as_str(),
            Self::UntrustedToFetchUrl { fetch_task, .. } => fetch_task.as_str(),
            Self::UntrustedWhenCondition {
                conditional_task, ..
            } => conditional_task.as_str(),
        })
    }

    /// Recommendation text.
    pub fn recommendation(&self) -> &'static str {
        match self {
            Self::UntrustedToExec { .. } => {
                "Add structured: schema to an intermediate infer task, or add trust: elevated"
            }
            Self::UntrustedToAgentTools { .. } => {
                "Remove dangerous tools from agent, or add trust: elevated to the task"
            }
            Self::UntrustedToInferNoSchema { .. } => {
                "Add structured: schema to constrain output format"
            }
            Self::UntrustedForEachAmplification { .. } => {
                "Reduce concurrency to 5 or less, or add fail_fast: true"
            }
            Self::UntrustedToFetchUrl { .. } => {
                "Validate/allowlist URLs before fetching, or use policy.allowed_hosts"
            }
            Self::UntrustedWhenCondition { .. } => {
                "Avoid branching on untrusted data, or validate the condition separately"
            }
        }
    }
}

impl std::fmt::Display for TaintWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code(), self.message())
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::analyzed::{
        AnalyzedAgentAction, AnalyzedExecAction, AnalyzedFetchAction, AnalyzedInferAction,
        AnalyzedInvokeAction, AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow, TaskId,
    };
    use crate::binding::{BindingPath, BindingSource, BindingType, WithEntry, WithSpec};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    // ── Helpers ─────────────────────────────────────────────────────────

    // Auto-incrementing task ID counter for tests
    static NEXT_TASK_ID: AtomicU32 = AtomicU32::new(0);

    fn next_task_id() -> TaskId {
        TaskId::new(NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed))
    }

    fn make_workflow(tasks: Vec<AnalyzedTask>) -> AnalyzedWorkflow {
        AnalyzedWorkflow {
            tasks,
            ..Default::default()
        }
    }

    fn simple_with_spec(alias: &str, source_task: &str) -> WithSpec {
        let mut spec = WithSpec::default();
        spec.insert(
            alias.to_string(),
            WithEntry {
                source: BindingPath {
                    source: BindingSource::Task(Arc::from(source_task)),
                    segments: vec![],
                },
                binding_type: BindingType::default(),
                default: None,
                lazy: false,
                transform: None,
            },
        );
        spec
    }

    fn input_with_spec(alias: &str, input_name: &str) -> WithSpec {
        let mut spec = WithSpec::default();
        spec.insert(
            alias.to_string(),
            WithEntry {
                source: BindingPath {
                    source: BindingSource::Input(Arc::from(input_name)),
                    segments: vec![],
                },
                binding_type: BindingType::default(),
                default: None,
                lazy: false,
                transform: None,
            },
        );
        spec
    }

    fn make_fetch_task(name: &str) -> AnalyzedTask {
        AnalyzedTask {
            id: next_task_id(),
            name: name.to_string(),
            action: AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
            description: None,
            provider: None,
            model: None,
            preset: None,
            with_spec: WithSpec::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            routing: None,
            when: None,
            trust_elevated: false,
            span: crate::source::Span::dummy(),
        }
    }

    fn make_infer_task(name: &str, with: WithSpec) -> AnalyzedTask {
        let mut t = make_fetch_task(name);
        t.action = AnalyzedTaskAction::Infer(AnalyzedInferAction::default());
        t.with_spec = with;
        t
    }

    fn make_exec_task(name: &str, with: WithSpec) -> AnalyzedTask {
        let mut t = make_fetch_task(name);
        t.action = AnalyzedTaskAction::Exec(AnalyzedExecAction::default());
        t.with_spec = with;
        t
    }

    fn make_invoke_task(name: &str, tool: &str, with: WithSpec) -> AnalyzedTask {
        let mut t = make_fetch_task(name);
        t.action = AnalyzedTaskAction::Invoke(AnalyzedInvokeAction {
            tool: tool.to_string(),
            ..Default::default()
        });
        t.with_spec = with;
        t
    }

    fn make_agent_task(name: &str, tools: Vec<&str>, with: WithSpec) -> AnalyzedTask {
        let mut t = make_fetch_task(name);
        t.action = AnalyzedTaskAction::Agent(Box::new(AnalyzedAgentAction {
            tools: tools.into_iter().map(String::from).collect(),
            ..Default::default()
        }));
        t.with_spec = with;
        t
    }

    // ── Trust Propagation Tests ─────────────────────────────────────────

    #[test]
    fn test_fetch_output_is_untrusted() {
        let wf = make_workflow(vec![make_fetch_task("scrape")]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert_eq!(report.trust_map["scrape"], TrustLevel::Untrusted);
    }

    #[test]
    fn test_infer_with_untrusted_is_model_tainted() {
        let wf = make_workflow(vec![
            make_fetch_task("scrape"),
            make_infer_task("summarize", simple_with_spec("data", "scrape")),
        ]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert_eq!(report.trust_map["scrape"], TrustLevel::Untrusted);
        assert_eq!(report.trust_map["summarize"], TrustLevel::ModelTainted);
    }

    #[test]
    fn test_infer_with_trusted_is_model_generated() {
        let wf = make_workflow(vec![make_infer_task(
            "generate",
            WithSpec::default(), // no bindings = all trusted
        )]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert_eq!(report.trust_map["generate"], TrustLevel::ModelGenerated);
    }

    #[test]
    fn test_taint_propagation_through_dag() {
        // fetch -> infer1 (tainted) -> infer2 (still tainted)
        let wf = make_workflow(vec![
            make_fetch_task("scrape"),
            make_infer_task("step1", simple_with_spec("data", "scrape")),
            make_infer_task("step2", simple_with_spec("data", "step1")),
        ]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert_eq!(report.trust_map["scrape"], TrustLevel::Untrusted);
        assert_eq!(report.trust_map["step1"], TrustLevel::ModelTainted);
        assert_eq!(report.trust_map["step2"], TrustLevel::ModelTainted);
    }

    #[test]
    fn test_builtin_invoke_propagates_trust() {
        // fetch -> nika:jq -> should still be untrusted (C3 correction)
        let wf = make_workflow(vec![
            make_fetch_task("scrape"),
            make_invoke_task("transform", "nika:jq", simple_with_spec("data", "scrape")),
        ]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert_eq!(
            report.trust_map["transform"],
            TrustLevel::Untrusted,
            "nika:jq must propagate untrusted input (C3)"
        );
    }

    #[test]
    fn test_pure_builtin_always_trusted() {
        let wf = make_workflow(vec![
            make_fetch_task("scrape"),
            make_invoke_task("wait", "nika:sleep", simple_with_spec("data", "scrape")),
        ]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert_eq!(
            report.trust_map["wait"],
            TrustLevel::Trusted,
            "nika:sleep output is always Trusted"
        );
    }

    #[test]
    fn test_mcp_invoke_is_untrusted() {
        let wf = make_workflow(vec![make_invoke_task(
            "search",
            "novanet::search",
            WithSpec::default(),
        )]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert_eq!(report.trust_map["search"], TrustLevel::Untrusted);
    }

    #[test]
    fn test_agent_always_model_tainted() {
        let wf = make_workflow(vec![make_agent_task(
            "research",
            vec![],
            WithSpec::default(),
        )]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert_eq!(report.trust_map["research"], TrustLevel::ModelTainted);
    }

    #[test]
    fn test_serve_inputs_untrusted() {
        let wf = make_workflow(vec![make_infer_task(
            "generate",
            input_with_spec("topic", "topic"),
        )]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Serve);
        assert_eq!(
            report.trust_map["generate"],
            TrustLevel::ModelTainted,
            "Serve inputs should be untrusted (C2)"
        );
    }

    #[test]
    fn test_cli_inputs_trusted() {
        let wf = make_workflow(vec![make_infer_task(
            "generate",
            input_with_spec("topic", "topic"),
        )]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert_eq!(
            report.trust_map["generate"],
            TrustLevel::ModelGenerated,
            "CLI inputs should be trusted"
        );
    }

    // ── Warning Tests ───────────────────────────────────────────────────

    #[test]
    fn test_warn_fetch_to_exec() {
        let wf = make_workflow(vec![
            make_fetch_task("scrape"),
            make_exec_task("run", simple_with_spec("data", "scrape")),
        ]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| matches!(w, TaintWarning::UntrustedToExec { .. })),
            "Should warn about fetch->exec"
        );
    }

    #[test]
    fn test_warn_agent_dangerous_tools() {
        let wf = make_workflow(vec![
            make_fetch_task("scrape"),
            make_agent_task(
                "agent",
                vec!["nika:write", "nika:run"],
                simple_with_spec("data", "scrape"),
            ),
        ]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| matches!(w, TaintWarning::UntrustedToAgentTools { .. })),
            "Should warn about agent with dangerous tools"
        );
    }

    #[test]
    fn test_warn_untrusted_infer_no_schema() {
        let wf = make_workflow(vec![
            make_fetch_task("scrape"),
            make_infer_task("analyze", simple_with_spec("data", "scrape")),
        ]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| matches!(w, TaintWarning::UntrustedToInferNoSchema { .. })),
            "Should warn about infer without structured"
        );
    }

    #[test]
    fn test_no_warning_for_clean_workflow() {
        let wf = make_workflow(vec![
            make_infer_task("step1", WithSpec::default()),
            make_infer_task("step2", simple_with_spec("data", "step1")),
        ]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert!(
            report.is_clean(),
            "Clean workflow should have no warnings, got: {:?}",
            report.warnings
        );
    }

    #[test]
    fn test_default_value_does_not_elevate_trust() {
        let mut spec = simple_with_spec("data", "scrape");
        spec.get_mut("data").unwrap().default = Some(serde_json::json!("fallback"));

        let wf = make_workflow(vec![
            make_fetch_task("scrape"),
            make_infer_task("summarize", spec),
        ]);
        let report = TaintAnalyzer::analyze(&wf, InvocationSource::Cli);
        assert_eq!(
            report.trust_map["summarize"],
            TrustLevel::ModelTainted,
            "Default value should not elevate trust"
        );
    }
}
