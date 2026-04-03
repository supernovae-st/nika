//! Preset resolution — extract agent preset fields into task actions.
//!
//! Centralizes the preset application logic previously inline in runner.rs.
//! Handles precedence: task-level > preset > workflow-default.
//!
//! Only Infer and Agent verbs receive preset fields (system, temperature, max_turns).
//! Exec, Fetch, and Invoke are no-ops — they have no LLM parameters.

use crate::ast::TaskAction;
use crate::runtime::resolver::ResolvedAgent;

/// Resolve effective provider/model by merging task-level overrides with preset defaults.
///
/// Precedence: task-level > preset > (caller handles workflow-default).
pub fn resolve_provider_model(
    task_provider: &Option<nika_core::ProviderName>,
    task_model: &Option<String>,
    preset: &ResolvedAgent,
) -> (Option<nika_core::ProviderName>, Option<String>) {
    let provider = task_provider.clone().or_else(|| {
        preset
            .provider
            .as_ref()
            .map(|p| nika_core::ProviderName::parse(p))
    });
    let model = task_model.clone().or_else(|| preset.model.clone());
    (provider, model)
}

/// Apply preset fields (system, temperature, max_turns) to a lowered action.
///
/// Only affects Infer and Agent verbs — Exec/Fetch/Invoke are returned unchanged.
/// Task-level values (already set on the action) take precedence over preset values.
pub fn apply_preset_fields(action: &mut TaskAction, preset: &ResolvedAgent) {
    match action {
        TaskAction::Infer { infer } => {
            if infer.system.is_none() {
                infer.system = Some(preset.system.clone());
            }
            if infer.temperature.is_none() {
                infer.temperature = preset.temperature.map(f64::from);
            }
        }
        TaskAction::Agent { agent } => {
            if agent.system.is_none() {
                agent.system = Some(preset.system.clone());
            }
            if agent.temperature.is_none() {
                agent.temperature = preset.temperature;
            }
            if agent.max_turns.is_none() {
                agent.max_turns = preset.max_turns;
            }
        }
        // Exec, Fetch, Invoke have no LLM parameters — no-op
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{AgentParams, InferParams, TaskAction};
    use crate::runtime::resolver::{AgentSource, ResolvedAgent};

    fn make_preset() -> ResolvedAgent {
        ResolvedAgent {
            system: "You are a deep thinker.".to_string(),
            provider: Some("anthropic".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            max_turns: Some(10),
            temperature: Some(0.3),
            source: AgentSource::Builtin,
        }
    }

    fn make_infer_action() -> TaskAction {
        TaskAction::Infer {
            infer: InferParams {
                prompt: "test".to_string(),
                provider: None,
                model: None,
                temperature: None,
                max_tokens: None,
                system: None,
                response_format: None,
                extended_thinking: None,
                thinking_budget: None,
                content: None,
                guardrails: vec![],
                base_url: None,
                provider_chain: None,
            },
        }
    }

    fn make_agent_action() -> TaskAction {
        TaskAction::Agent {
            agent: AgentParams {
                prompt: "test agent".to_string(),
                ..AgentParams::default()
            },
        }
    }

    // ─── resolve_provider_model ───────────────────────────────────

    #[test]
    fn test_preset_provides_provider_when_task_has_none() {
        let preset = make_preset();
        let (provider, model) = resolve_provider_model(&None, &None, &preset);
        assert_eq!(provider.as_ref().map(|p| p.as_str()), Some("anthropic"));
        assert_eq!(model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn test_task_provider_overrides_preset() {
        let preset = make_preset();
        let (provider, model) = resolve_provider_model(
            &Some(nika_core::ProviderName::OpenAI),
            &Some("gpt-4o".to_string()),
            &preset,
        );
        assert_eq!(provider.as_ref().map(|p| p.as_str()), Some("openai"));
        assert_eq!(model.as_deref(), Some("gpt-4o"));
    }

    #[test]
    fn test_partial_override_provider_only() {
        let preset = make_preset();
        let (provider, model) =
            resolve_provider_model(&Some(nika_core::ProviderName::OpenAI), &None, &preset);
        assert_eq!(provider.as_ref().map(|p| p.as_str()), Some("openai"));
        assert_eq!(
            model.as_deref(),
            Some("claude-sonnet-4-6"),
            "model falls back to preset"
        );
    }

    // ─── apply_preset_fields: Infer ──────────────────────────────

    #[test]
    fn test_preset_applies_system_to_infer() {
        let preset = make_preset();
        let mut action = make_infer_action();
        apply_preset_fields(&mut action, &preset);
        if let TaskAction::Infer { infer } = &action {
            assert_eq!(infer.system.as_deref(), Some("You are a deep thinker."));
        } else {
            panic!("expected Infer");
        }
    }

    #[test]
    fn test_preset_applies_temperature_to_infer() {
        let preset = make_preset();
        let mut action = make_infer_action();
        apply_preset_fields(&mut action, &preset);
        if let TaskAction::Infer { infer } = &action {
            assert!(
                (infer.temperature.unwrap() - 0.3).abs() < 1e-6,
                "temperature should be ~0.3 from preset (f32→f64)"
            );
        } else {
            panic!("expected Infer");
        }
    }

    #[test]
    fn test_task_override_wins_over_preset_infer() {
        let preset = make_preset();
        let mut action = TaskAction::Infer {
            infer: InferParams {
                prompt: "test".to_string(),
                provider: None,
                model: None,
                temperature: Some(0.9),
                max_tokens: None,
                system: Some("My own system".to_string()),
                response_format: None,
                extended_thinking: None,
                thinking_budget: None,
                content: None,
                guardrails: vec![],
                base_url: None,
                provider_chain: None,
            },
        };
        apply_preset_fields(&mut action, &preset);
        if let TaskAction::Infer { infer } = &action {
            assert_eq!(
                infer.system.as_deref(),
                Some("My own system"),
                "task-level system wins"
            );
            assert!(
                (infer.temperature.unwrap() - 0.9).abs() < f64::EPSILON,
                "task-level temperature wins"
            );
        } else {
            panic!("expected Infer");
        }
    }

    // ─── apply_preset_fields: Agent ──────────────────────────────

    #[test]
    fn test_preset_applies_to_agent_verb() {
        let preset = make_preset();
        let mut action = make_agent_action();
        apply_preset_fields(&mut action, &preset);
        if let TaskAction::Agent { agent } = &action {
            assert_eq!(agent.system.as_deref(), Some("You are a deep thinker."));
            assert!(
                (agent.temperature.unwrap() - 0.3).abs() < f32::EPSILON,
                "temperature should be 0.3 from preset"
            );
            assert_eq!(agent.max_turns, Some(10));
        } else {
            panic!("expected Agent");
        }
    }

    // ─── apply_preset_fields: non-LLM verbs ─────────────────────

    #[test]
    fn test_exec_ignores_preset() {
        use crate::ast::ExecParams;
        let preset = make_preset();
        let mut action = TaskAction::Exec {
            exec: ExecParams {
                command: "echo hi".to_string(),
                shell: None,
                cwd: None,
                timeout: None,
                env: None,
                max_stdout: None,
            },
        };
        let original = format!("{action:?}");
        apply_preset_fields(&mut action, &preset);
        assert_eq!(format!("{action:?}"), original, "exec unchanged by preset");
    }

    #[test]
    fn test_fetch_ignores_preset() {
        use crate::ast::FetchParams;
        let preset = make_preset();
        let mut action = TaskAction::Fetch {
            fetch: FetchParams {
                url: "https://example.com".to_string(),
                method: "GET".to_string(),
                headers: Default::default(),
                body: None,
                json: None,
                extract: None,
                selector: None,
                response: None,
                follow_redirects: None,
                timeout: None,
                retry: None,
                session: None,
                cache: None,
            },
        };
        let original = format!("{action:?}");
        apply_preset_fields(&mut action, &preset);
        assert_eq!(format!("{action:?}"), original, "fetch unchanged by preset");
    }

    #[test]
    fn test_invoke_ignores_preset() {
        use crate::ast::InvokeParams;
        let preset = make_preset();
        let mut action = TaskAction::Invoke {
            invoke: InvokeParams {
                tool: Some("nika:sleep".to_string()),
                params: None,
                timeout: None,
                mcp: None,
                resource: None,
            },
        };
        let original = format!("{action:?}");
        apply_preset_fields(&mut action, &preset);
        assert_eq!(
            format!("{action:?}"),
            original,
            "invoke unchanged by preset"
        );
    }
}
