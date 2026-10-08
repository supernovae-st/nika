// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The plan under the workflow's AUTHORED access requirement
//! (`run.access` · `run.reasoning`) — the same resolver and the same pin
//! judge, never a parallel router.
//!
//! - The file's `via` is judged exactly as `--access <via>` would be
//!   (NIKA-1800..1803); its refusal names the file field.
//! - A declared `protocol` constrains every lane and every role: `acp`
//!   admits only an agent application's seat, `api` only a key, local or
//!   mock provider path; an `infer:` task needs a route whose ACP one-shot
//!   is attested, because a direct CLI never satisfies `acp`.
//! - `fallback: none` (or its omission) keeps the pin law: refusal, never
//!   a substitute route, protocol, model or effort.
//! - An explicit `--access` that contradicts the file refuses before task
//!   1 (NIKA-1801) instead of silently replacing the declaration.
//! - A declared effort is judged where the route is static: a provider
//!   lane must list it in the model catalog, and a direct CLI one-shot
//!   cannot carry one. A live ACP session judges its own offer before the
//!   first prompt.

use nika_types::access::{AccessClass, AccessProtocol, AccessRequirement, HarnessRuntime};

use super::{ExecutionAccessPlan, LaneVerdict, ModelNeed, resolve_execution_plan_for};
use crate::probe::ProviderProbe;
use crate::resolve_access::{PinRefusal, VerbNeeds, provider_of};

/// The class words a pin accepts and a route id is not: `via` names one
/// route, and the class choice belongs to `protocol`. `mock` stays a
/// route (the in-crate mock provider's id).
const CLASS_WORDS: [&str; 4] = ["api", "local", "harness", "oauth"];

/// [`resolve_execution_plan_for`] under the workflow's authored
/// requirement. `None` (no `run.access`/`run.reasoning`) is the existing
/// resolution, byte-identical; `pin` is the operator's `--access` alone.
#[must_use]
pub fn resolve_execution_plan_declared(
    needs: &[ModelNeed],
    probes: &[ProviderProbe],
    pin: Option<&str>,
    verbs: VerbNeeds,
    requirement: Option<&AccessRequirement>,
) -> ExecutionAccessPlan {
    let Some(req) = requirement else {
        return resolve_execution_plan_for(needs, probes, pin, verbs);
    };
    let route = match route_for(req, pin, probes) {
        Ok(route) => route,
        Err(refusal) => {
            return ExecutionAccessPlan::new(
                std::collections::BTreeMap::default(),
                pin.map(str::to_owned),
                None,
                Some(refusal),
            )
            .with_requirement(Some(req.clone()));
        }
    };
    let mut plan = resolve_execution_plan_for(needs, probes, route.as_deref(), verbs);
    plan.pin = pin.map(str::to_owned);
    plan.requirement = Some(req.clone());
    if let Some(refusal) = plan.pin_refusal.take() {
        plan.pin_refusal = Some(attribute(req, pin, refusal));
    } else if let Some(message) = role_refusal(&plan, req, route.as_deref(), needs, verbs) {
        plan.pin_refusal = Some(PinRefusal::NoPath { message });
        plan.seat = None;
    }
    for verdict in plan.lanes.values_mut() {
        if let LaneVerdict::Admitted(lane) = verdict {
            lane.plan.requirement = Some(Box::new(req.clone()));
        }
    }
    plan
}

/// The route token a requirement names on its own: its `via`, else the
/// class its declared protocol admits.
pub(super) fn route_of(req: &AccessRequirement) -> Option<&str> {
    req.via.as_deref().or(match req.protocol {
        Some(AccessProtocol::Acp) => Some(AccessClass::Harness.as_str()),
        Some(AccessProtocol::Api) => Some(AccessClass::Api.as_str()),
        // `#[non_exhaustive]` · an unknown protocol selects no route.
        _ => None,
    })
}

/// The token the resolver pins: the file's route, an operator flag the
/// file allows, or today's flag when the file selects no path. A flag
/// that contradicts the file refuses here, before any resolution.
fn route_for(
    req: &AccessRequirement,
    pin: Option<&str>,
    probes: &[ProviderProbe],
) -> Result<Option<String>, PinRefusal> {
    if let Some(via) = req.via.as_deref() {
        if CLASS_WORDS.contains(&via) {
            return Err(PinRefusal::UnknownToken {
                message: format!(
                    "the workflow declares `run.access.via: {via}`, an access class — `via` \
                     names one route (an agent application such as `codex` or `claude-code`, \
                     or a provider such as `openai`); the class choice is \
                     `run.access.protocol`"
                ),
            });
        }
        // A route the declared protocol can never reach is the file's own
        // contradiction, judged before (and whatever) this machine offers.
        if let (Some(protocol), Some(class)) = (req.protocol, static_class(via))
            && !protocol.admits(class)
        {
            return Err(PinRefusal::NoPath {
                message: format!(
                    "the workflow declares `run.access.via: {via}`, reached as {}, and \
                     `run.access.protocol: {protocol}` does not admit it — {} (nothing ran)",
                    class.as_str(),
                    protocol_teaching(protocol)
                ),
            });
        }
        return match pin {
            Some(flag) if flag != via => Err(PinRefusal::PinUnsatisfied {
                message: format!(
                    "`--access {flag}` contradicts the workflow's `run.access.via: {via}` — an \
                     explicit selection is never silently replaced; drop the flag or change \
                     the file (nothing ran)"
                ),
            }),
            _ => Ok(Some(via.to_owned())),
        };
    }
    let Some(protocol) = req.protocol else {
        return Ok(pin.map(str::to_owned));
    };
    match pin {
        Some(flag) => match token_class(flag, probes) {
            Some(class) if protocol.admits(class) => Ok(Some(flag.to_owned())),
            class => Err(PinRefusal::PinUnsatisfied {
                message: format!(
                    "`--access {flag}` reaches {} and the workflow's `run.access.protocol: \
                     {protocol}` does not admit it — an explicit selection is never silently \
                     replaced; drop the flag or change the file (nothing ran)",
                    class.map_or("an unknown path", AccessClass::as_str)
                ),
            }),
        },
        None => Ok(route_of(req).map(str::to_owned)),
    }
}

/// The class a pin token reaches: an agent application or the harness
/// word is a seat; a class word is itself; a provider id is its probe
/// row's class (or the profile's, for a row this machine did not list).
fn token_class(token: &str, probes: &[ProviderProbe]) -> Option<AccessClass> {
    if HarnessRuntime::lookup(token).is_some() {
        return Some(AccessClass::Harness);
    }
    if let Some(class) = AccessClass::ALL.into_iter().find(|c| c.as_str() == token) {
        return Some(class);
    }
    if HarnessRuntime::retired_alias(token).is_some() {
        return None;
    }
    Some(probes.iter().find(|p| p.id == token).map_or_else(
        || crate::profile::access_class_for(token),
        |p| p.readiness.access,
    ))
}

/// The pin judge's own refusal, re-attributed to the file field that
/// declared the route (the judge speaks of `--access <token>`; the author
/// wrote `run.access.via`). An operator flag keeps its message as is.
fn attribute(req: &AccessRequirement, pin: Option<&str>, refusal: PinRefusal) -> PinRefusal {
    if pin.is_some() {
        return refusal;
    }
    let origin = match (&req.via, req.protocol) {
        (Some(via), _) => format!("the workflow declares `run.access.via: {via}`"),
        (None, Some(protocol)) => {
            format!("the workflow declares `run.access.protocol: {protocol}`")
        }
        (None, None) => return refusal,
    };
    let say = |message: String| format!("{origin} (judged as its `--access` token) · {message}");
    match refusal {
        PinRefusal::UnknownToken { message } => PinRefusal::UnknownToken {
            message: say(message),
        },
        PinRefusal::PinUnsatisfied { message } => PinRefusal::PinUnsatisfied {
            message: say(message),
        },
        PinRefusal::NoPath { message } => PinRefusal::NoPath {
            message: say(message),
        },
        PinRefusal::Unavailable { message } => PinRefusal::Unavailable {
            message: say(message),
        },
    }
}

/// The protocol and effort laws over the resolved plan — `Some` is the
/// refusal witness (NIKA-1800, before task 1).
fn role_refusal(
    plan: &ExecutionAccessPlan,
    req: &AccessRequirement,
    route: Option<&str>,
    needs: &[ModelNeed],
    verbs: VerbNeeds,
) -> Option<String> {
    if let Some(protocol) = req.protocol {
        for (model, lane) in plan.admitted() {
            if !protocol.admits(lane.plan.chosen) {
                return Some(format!(
                    "`{model}` resolves to `{}` ({}) and the workflow's `run.access.protocol: \
                     {protocol}` does not admit it — {} (nothing ran)",
                    lane.plan.access,
                    lane.plan.chosen.as_str(),
                    protocol_teaching(protocol)
                ));
            }
        }
        if let Some(route) = route
            && let Some(class) = static_class(route)
            && !protocol.admits(class)
        {
            return Some(format!(
                "the route `{route}` is reached as {} and the workflow's \
                 `run.access.protocol: {protocol}` does not admit it — {} (nothing ran)",
                class.as_str(),
                protocol_teaching(protocol)
            ));
        }
    }
    let infer = verbs.infer || needs.iter().any(|n| n.infer);
    if infer && let Some(seat) = plan.seat.as_deref() {
        if req.protocol == Some(AccessProtocol::Acp) {
            if let Err(witness) = acp_one_shot(seat) {
                return Some(format!(
                    "`infer:` tasks cannot ride `{seat}` under `run.access.protocol: acp`: \
                     {witness} — a direct CLI never satisfies `acp`; run these tasks as \
                     `agent:` on this route, or declare an API route (nothing ran)"
                ));
            }
        } else if let Some(effort) = req.effort.as_deref() {
            return Some(format!(
                "`run.reasoning.effort: {effort}` cannot ride the direct `{seat}` one-shot an \
                 `infer:` task uses here (it carries no effort selection) — declare \
                 `run.access.protocol: acp` and run the task as `agent:`, or drop the effort \
                 (nothing ran)"
            ));
        }
    }
    let effort = req.effort.as_deref()?;
    plan.admitted()
        .filter(|(_, lane)| !matches!(lane.plan.chosen, AccessClass::Harness))
        .find_map(|(model, _)| catalog_effort_refusal(model, effort))
}

fn protocol_teaching(protocol: AccessProtocol) -> &'static str {
    match protocol {
        AccessProtocol::Acp => {
            "`acp` needs an agent application's ACP seat (`via: codex` · `via: claude-code` …)"
        }
        _ => "`api` needs a provider key or a configured local server (`via: openai` …)",
    }
}

/// The class a route token is reached as WITHOUT any probe: an agent
/// application is a seat; a known provider id is its profile class.
fn static_class(route: &str) -> Option<AccessClass> {
    if HarnessRuntime::lookup(route).is_some() || route == AccessClass::Harness.as_str() {
        return Some(AccessClass::Harness);
    }
    AccessClass::ALL
        .into_iter()
        .find(|c| c.as_str() == route)
        .or_else(|| {
            nika_catalog::find_provider(route)
                .map(|_| crate::profile::access_class_for(provider_of(route)))
        })
}

/// A provider lane carries an explicit effort only when the exact model's
/// catalog lists that level (the wire then also requires its exact direct
/// endpoint before a byte leaves). Effort words are the route's own: no
/// alias is ever applied.
fn catalog_effort_refusal(model: &str, effort: &str) -> Option<String> {
    let (provider, name) = model.split_once('/').unwrap_or((model, ""));
    let listed = nika_catalog::model_capabilities(provider, name).reasoning_efforts;
    if listed.iter().any(|level| level.word() == effort) {
        return None;
    }
    let levels = if listed.is_empty() {
        "no effort level".to_owned()
    } else {
        listed
            .iter()
            .map(|level| level.word())
            .collect::<Vec<_>>()
            .join(" · ")
    };
    Some(format!(
        "`run.reasoning.effort: {effort}` is not offered by `{model}`: its model catalog lists \
         {levels} — name one of these exact values, or drop the effort (nothing ran)"
    ))
}

#[cfg(feature = "access-harness")]
fn acp_one_shot(seat: &str) -> Result<(), String> {
    nika_harness::meet_acp_one_shot(seat).map_err(|e| e.to_string())
}

#[cfg(not(feature = "access-harness"))]
fn acp_one_shot(seat: &str) -> Result<(), String> {
    Err(format!(
        "this nika was built without agentic CLI adapters, so `{seat}` has no ACP one-shot"
    ))
}

#[cfg(test)]
mod tests;
