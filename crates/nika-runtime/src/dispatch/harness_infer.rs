// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Infer on a seated agentic CLI (ACP) — the `--access claude-code`
//! path. Lives beside `dispatch.rs` under the 1,500-LOC file law.

use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta};
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::tool_executor::ToolExecuteDyn;
use nika_types::cost::UnpricedReason;
use nika_verb_agent::{AgentValue, NoopObserver};
use nika_verb_infer::InferInput;
use serde_json::Value;

use super::{Dispatched, spend_for_model};
use crate::Runtime;

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C>
where
    P: ProviderInferDyn + ProviderMeta,
    T: ToolExecuteDyn,
    D: ToolDefinitionProviderDyn,
{
    /// `infer:` on the seated agentic CLI (subscription, no vendor API key).
    pub(super) async fn infer_on_seated_harness(&self, input: InferInput) -> Dispatched {
        let note = match &self.harness_seat_id {
            Some(id) => format!("infer · {id}"),
            None => "infer · harness".to_owned(),
        };
        let ran = self
            .agent
            .run_infer_on_harness(
                input.prompt,
                input.system,
                input.model,
                input.schema,
                &NoopObserver,
            )
            .await;
        match ran {
            Ok(out) => {
                let value = match out.output {
                    AgentValue::Text(text) => Value::String(text),
                    AgentValue::Structured(value) => value,
                    other => {
                        return Dispatched::unwired(
                            &note,
                            format!("infer value form not wired yet: {other:?}"),
                        );
                    }
                };
                let tokens = Some(i64::try_from(out.total_tokens).unwrap_or(i64::MAX));
                let (cost_usd, cost_unpriced) = match out.model_resolved.as_deref() {
                    Some(model) => spend_for_model(model, &out.usage),
                    None => (None, Some(UnpricedReason::SubscriptionQuota)),
                };
                Dispatched::ok_metered(
                    note,
                    value,
                    tokens,
                    None,
                    cost_usd,
                    out.model_resolved.clone(),
                    cost_unpriced,
                )
            }
            Err(err) => {
                let spend = super::price_failed_spend(err.spend());
                Dispatched::verb_err_spent(note, &err, spend)
            }
        }
    }
}
