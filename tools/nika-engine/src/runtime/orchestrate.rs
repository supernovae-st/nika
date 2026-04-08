// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Orchestrator rewrite: wraps a goal-driven workflow into a single agent task.
//!
//! When a workflow has `goal:` (and optionally `orchestrate:`), this module
//! transforms the workflow by appending an orchestrator agent task that
//! coordinates execution and pursues the stated goal.
//!
//! The original tasks remain unchanged — the orchestrator task depends on all
//! of them and uses their outputs to validate goal completion.

use nika_core::ast::analyzed::{
    AnalyzedAgentAction, AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow,
};
use nika_core::ast::completion::{CompletionConfig, CompletionMode};
use nika_core::ast::templatable::Templatable;
use nika_core::source::Span;

/// The reserved task ID for the orchestrator agent.
pub const ORCHESTRATOR_TASK_ID: &str = "__orchestrator__";

/// Build the orchestrator system prompt from goal and task descriptions.
fn build_system_prompt(goal: &str, tasks: &[AnalyzedTask], confidence_target: f64) -> String {
    let mut prompt = String::with_capacity(1024);
    prompt.push_str("You are a workflow orchestrator. Your goal:\n\n");
    prompt.push_str(goal);
    prompt.push_str("\n\n## Available Task Results\n\n");

    for task in tasks {
        prompt.push_str(&format!("- **{}**", task.name));
        if let Some(desc) = &task.description {
            prompt.push_str(&format!(": {}", desc));
        }
        prompt.push('\n');
    }

    prompt.push_str("\n## Instructions\n\n");
    prompt.push_str(&format!(
        "1. Review the completed task results using nika:records\n\
         2. Assess whether the goal has been achieved\n\
         3. If results are insufficient, use nika:run to execute additional sub-workflows\n\
         4. Use nika:cost to track spending against your budget\n\
         5. When the goal is met, call nika:complete with your final assessment\n\n\
         ## Confidence Threshold\n\n\
         Your confidence must reach at least {confidence_target:.2} before calling nika:complete.\n\
         If your confidence is below this threshold, continue working to improve results.\n",
    ));

    prompt.push_str(
        "\n## nika:run YAML Syntax\n\n\
         When calling nika:run with yaml_content, use this EXACT syntax:\n\n\
         ```yaml\n\
         schema: \"nika/workflow@0.12\"\n\
         provider: openai\n\
         model: gpt-4o-mini\n\
         tasks:\n\
         \x20 - id: my_task\n\
         \x20   infer: \"Your prompt here\"\n\
         \x20 - id: second_task\n\
         \x20   depends_on: [my_task]\n\
         \x20   with:\n\
         \x20     data: $my_task\n\
         \x20   infer: \"Analyze: {{with.data}}\"\n\
         ```\n\n\
         RULES:\n\
         - ALWAYS start with `schema: \"nika/workflow@0.12\"`\n\
         - Use `tasks:` (NOT `steps:`)\n\
         - Every task MUST have `id:` field\n\
         - Use `infer:` for LLM calls, `exec:` for shell commands, `fetch:` for HTTP\n\
         - Reference other tasks with `$task_id` in `with:` bindings\n\
         - Use `{{with.alias}}` for template variables\n",
    );

    prompt
}

/// Build the orchestrator prompt that binds task outputs.
fn build_agent_prompt(goal: &str, task_names: &[String]) -> String {
    let mut prompt = String::with_capacity(512);
    prompt.push_str(&format!(
        "Goal: {goal}\n\nThe following tasks have completed: {}.\n\
         Review their results and determine if the goal is achieved.",
        task_names.join(", ")
    ));
    prompt
}

/// Wrap a goal-driven workflow by appending an orchestrator agent task.
///
/// The orchestrator task:
/// - Depends on ALL existing tasks (runs after them)
/// - Has access to builtin tools: nika:records, nika:cost, nika:run, nika:complete
/// - Uses `completion: { mode: explicit }` — must call nika:complete to finish
/// - Respects OrchestrateConfig for max_rounds, cost limits, etc.
///
/// Returns the modified workflow with the orchestrator task appended.
pub fn wrap_as_orchestrator(mut workflow: AnalyzedWorkflow) -> AnalyzedWorkflow {
    let goal = match &workflow.goal {
        Some(g) => g.clone(),
        None => return workflow, // No goal = no orchestration
    };

    let config = workflow.orchestrate.clone().unwrap_or_default();

    // Collect task IDs for depends_on
    let task_ids: Vec<_> = workflow.tasks.iter().map(|t| t.id).collect();
    let task_names: Vec<_> = workflow.tasks.iter().map(|t| t.name.clone()).collect();

    // Build the orchestrator agent
    let system = build_system_prompt(&goal, &workflow.tasks, config.confidence_target);
    let prompt = build_agent_prompt(&goal, &task_names);

    let limits = config.max_cost_usd.map(|max| nika_core::ast::LimitsConfig {
        max_cost_usd: max,
        ..Default::default()
    });

    let agent_action = AnalyzedAgentAction {
        prompt,
        system: Some(system),
        tools: vec![
            "nika:records".to_string(),
            "nika:cost".to_string(),
            "nika:run".to_string(),
            "nika:complete".to_string(),
            "nika:log".to_string(),
        ],
        max_turns: Some(Templatable::Value(config.max_rounds * 3)), // ~3 turns per round
        completion: Some(CompletionConfig {
            mode: CompletionMode::Explicit,
            ..Default::default()
        }),
        limits,
        ..Default::default()
    };

    // Register the orchestrator task ID
    let task_id = workflow.task_table.insert(ORCHESTRATOR_TASK_ID);

    let orchestrator_task = AnalyzedTask {
        id: task_id,
        name: ORCHESTRATOR_TASK_ID.to_string(),
        description: Some(format!("Orchestrator: {goal}")),
        action: AnalyzedTaskAction::Agent(Box::new(agent_action)),
        provider: workflow.provider.clone(),
        model: workflow.model.clone(),
        preset: config.agent,
        with_spec: Default::default(),
        depends_on: task_ids,
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
        span: Span::dummy(),
    };

    workflow.tasks.push(orchestrator_task);
    workflow
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_core::ast::analyzed::{
        AnalyzedExecAction, AnalyzedInferAction, SchemaVersion, TaskId, TaskTable,
    };
    use nika_core::ast::OrchestrateConfig;

    fn make_task(
        id: TaskId,
        name: &str,
        desc: Option<&str>,
        action: AnalyzedTaskAction,
        depends_on: Vec<TaskId>,
    ) -> AnalyzedTask {
        AnalyzedTask {
            id,
            name: name.to_string(),
            description: desc.map(String::from),
            action,
            provider: None,
            model: None,
            preset: None,
            with_spec: Default::default(),
            depends_on,
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
            span: Span::dummy(),
        }
    }

    fn make_test_workflow(goal: Option<&str>) -> AnalyzedWorkflow {
        let mut task_table = TaskTable::new();
        let t1 = task_table.insert("research");
        let t2 = task_table.insert("summarize");

        let tasks = vec![
            make_task(
                t1,
                "research",
                Some("Research the topic"),
                AnalyzedTaskAction::Infer(AnalyzedInferAction {
                    prompt: "Research AI".to_string(),
                    ..Default::default()
                }),
                vec![],
            ),
            make_task(
                t2,
                "summarize",
                Some("Summarize findings"),
                AnalyzedTaskAction::Exec(AnalyzedExecAction {
                    command: "echo done".to_string(),
                    ..Default::default()
                }),
                vec![t1],
            ),
        ];

        AnalyzedWorkflow {
            schema_version: SchemaVersion::V12,
            name: Some("test".to_string()),
            goal: goal.map(String::from),
            provider: Some(nika_core::ProviderName::Mock),
            task_table,
            tasks,
            ..Default::default()
        }
    }

    #[test]
    fn wrap_adds_orchestrator_task() {
        let workflow = make_test_workflow(Some("Write a report"));
        assert_eq!(workflow.tasks.len(), 2);

        let wrapped = wrap_as_orchestrator(workflow);
        assert_eq!(wrapped.tasks.len(), 3);

        let orch = &wrapped.tasks[2];
        assert_eq!(orch.name, ORCHESTRATOR_TASK_ID);
        assert!(matches!(orch.action, AnalyzedTaskAction::Agent(_)));
    }

    #[test]
    fn wrap_skips_without_goal() {
        let workflow = make_test_workflow(None);
        assert_eq!(workflow.tasks.len(), 2);

        let wrapped = wrap_as_orchestrator(workflow);
        assert_eq!(wrapped.tasks.len(), 2);
    }

    #[test]
    fn orchestrator_depends_on_all_tasks() {
        let workflow = make_test_workflow(Some("Complete project"));
        let wrapped = wrap_as_orchestrator(workflow);

        let orch = &wrapped.tasks[2];
        assert_eq!(orch.depends_on.len(), 2);
    }

    #[test]
    fn orchestrator_has_builtin_tools() {
        let workflow = make_test_workflow(Some("Build pipeline"));
        let wrapped = wrap_as_orchestrator(workflow);

        let orch = &wrapped.tasks[2];
        if let AnalyzedTaskAction::Agent(agent) = &orch.action {
            assert!(agent.tools.contains(&"nika:records".to_string()));
            assert!(agent.tools.contains(&"nika:run".to_string()));
            assert!(agent.tools.contains(&"nika:complete".to_string()));
            assert!(agent.tools.contains(&"nika:cost".to_string()));
        } else {
            panic!("Expected Agent action");
        }
    }

    #[test]
    fn system_prompt_includes_goal_and_tasks() {
        let workflow = make_test_workflow(Some("Write a report on AI"));
        let system = build_system_prompt("Write a report on AI", &workflow.tasks, 0.85);

        assert!(system.contains("Write a report on AI"));
        assert!(system.contains("research"));
        assert!(system.contains("summarize"));
        assert!(system.contains("Research the topic"));
    }

    #[test]
    fn orchestrate_config_affects_agent() {
        let mut workflow = make_test_workflow(Some("Test config"));
        workflow.orchestrate = Some(OrchestrateConfig {
            max_rounds: 5,
            max_cost_usd: Some(2.0),
            agent: Some("speed".to_string()),
            ..Default::default()
        });

        let wrapped = wrap_as_orchestrator(workflow);
        let orch = &wrapped.tasks[2];

        assert_eq!(orch.preset, Some("speed".to_string()));
        if let AnalyzedTaskAction::Agent(agent) = &orch.action {
            assert_eq!(agent.max_turns, Some(Templatable::Value(15))); // 5 * 3
            assert!(agent.limits.is_some());
        } else {
            panic!("Expected Agent action");
        }
    }
}
