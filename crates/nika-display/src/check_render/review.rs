// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rows a review shows of one `nika check` report, beside the report's other renders
//! (descended from `nika-session`'s change preview, C10): the report's first findings and hints,
//! what the workflow reaches when it runs and the spend it can reach. Pure text over the
//! report, no theme and no I/O: the caller keeps the verdict, the path and every authority.

/// The report's first findings (`code · message`, at most eight) and hints (`kind · advice`, at
/// most four), in the report's order.
#[must_use]
pub fn finding_rows(report: &nika_check::CheckReport) -> (Vec<String>, Vec<String>) {
    let findings = report
        .findings
        .iter()
        .take(8)
        .map(|f| format!("{} · {}", f.code.as_deref().unwrap_or("-"), f.message))
        .collect();
    let hints = report
        .hints
        .iter()
        .take(4)
        .map(|h| format!("{} · {}", h.kind, h.advice))
        .collect();
    (findings, hints)
}

/// What the workflow reaches when it runs, from the report's own
/// permits (needed) and requirements: one row per effect class present.
#[must_use]
pub fn effect_rows(report: &nika_check::CheckReport) -> Vec<String> {
    let mut rows = Vec::new();
    let needed = &report.permits.needed;
    if let Some(fs) = &needed.fs {
        if !fs.read.is_empty() {
            rows.push(format!("reads {}", fs.read.join(" · ")));
        }
        if !fs.write.is_empty() {
            rows.push(format!("writes {}", fs.write.join(" · ")));
        }
    }
    if let Some(net) = &needed.net
        && !net.http.is_empty()
    {
        rows.push(format!("network {}", net.http.join(" · ")));
    }
    match &needed.exec {
        Some(nika_cap::ExecPermit::Any) => rows.push("runs any program".to_owned()),
        Some(nika_cap::ExecPermit::Programs(p)) if !p.is_empty() => {
            rows.push(format!("runs {}", p.join(" · ")));
        }
        _ => {}
    }
    if let Some(tools) = &needed.tools {
        if !tools.is_empty() {
            rows.push(format!("tools {}", tools.join(" · ")));
        }
        if tools.iter().any(|t| t == "nika:prompt") {
            rows.push("pauses for a human answer (`nika:prompt`)".to_owned());
        }
    }
    if let Some(env) = &needed.env
        && !env.is_empty()
    {
        rows.push(format!("environment {}", env.join(" · ")));
    }
    for m in &report.requirements.models {
        rows.push(format!("model {} (tasks {})", m.model, m.tasks.join(" · ")));
    }
    for s in &report.requirements.secrets {
        rows.push(format!(
            "secret {} (key {} · a reference, never a value)",
            s.name, s.key
        ));
    }
    rows.extend(spend_rows(&report.cost));
    rows
}

/// The spend a run can reach, from the check's own cost envelope, never
/// from a total read alone. A workflow with no model call spends nothing
/// on inference. A mock model is a proven zero. A model with no catalog
/// price is its own row: unknown, never counted as free. A missing token
/// bound or an unknown fan-out stays unbounded.
fn spend_rows(cost: &nika_check::CostCeiling) -> Vec<String> {
    if cost.tasks.is_empty() && cost.composed.is_empty() {
        return vec![
            "model output estimate · $0 · no direct model task in these checked bytes".to_owned(),
        ];
    }
    let no_price = cost
        .tasks
        .iter()
        .filter(|t| t.unbounded_reason == Some(nika_check::UnboundedReason::NoPrice))
        .count();
    let unbounded = cost
        .tasks
        .iter()
        .filter(|t| {
            t.usd.is_none() && t.unbounded_reason != Some(nika_check::UnboundedReason::NoPrice)
        })
        .count()
        + cost.composed.iter().filter(|c| c.has_unbounded).count();
    let mut rows = Vec::new();
    if no_price > 0 {
        rows.push(format!(
            "model output estimate · unknown · {no_price} model task(s) with no catalog price — never counted as free"
        ));
    }
    if unbounded > 0 {
        rows.push(
            "model output estimate · unbounded: a token or iteration bound is missing; Run admission is separate"
                .to_owned(),
        );
    }
    if no_price == 0 && unbounded == 0 {
        let all_mock = cost.composed.is_empty()
            && cost.tasks.iter().all(|t| {
                t.model
                    .as_deref()
                    .is_some_and(|m| m == "mock" || m.starts_with("mock/"))
            });
        rows.push(if all_mock {
            "model output estimate · $0 · mock model tasks".to_owned()
        } else {
            format!(
                "model output estimate ≤ ${:.4} at catalog prices · input tokens and other charges excluded",
                cost.bounded_total_usd
            )
        });
    }
    rows
}

#[cfg(test)]
mod tests;
