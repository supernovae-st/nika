// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a mock lane does not stub (#1605). A mock model replaces the MODEL route only: the
//! workflow's own fetches, MCP tools and paid media calls still run, and `nika check` used to
//! say « never dials · nothing to judge » and « nothing dials » over them. The layers name
//! those tasks; the mock lane's ACCESS lines and the moot layers line point at the PERMITS
//! row that judges them.

use nika_check::CheckReport;
use nika_providers::ExecutionAccessPlan;

/// The tasks whose own effects can reach the network, sorted (#1605): a `net.http`
/// destination the transport dials (never a documentation host), an MCP tool, a paid media
/// call. ACCESS READY judges model routes only; the PERMITS row judges these.
pub(super) fn tasks(report: &CheckReport) -> Vec<String> {
    let journey = (report.data_journey.destinations.iter())
        .filter(|endpoint| match endpoint.kind {
            "net.http" => !nika_types::net::is_documentation_host(&endpoint.target),
            "mcp.tool" => true,
            _ => false,
        })
        .flat_map(|endpoint| endpoint.tasks.iter().cloned());
    let media = (report.cost.media.iter()).map(|call| call.task.clone());
    let tasks: std::collections::BTreeSet<String> = journey.chain(media).collect();
    tasks.into_iter().collect()
}

/// The ACCESS line a mock lane owes the tasks it does not stub: the mock replaces the model,
/// never a tool, so a rehearsal on it still reaches the network through them.
pub(super) fn mock_scope_line(
    plan: &ExecutionAccessPlan,
    network_tasks: &[String],
) -> Option<String> {
    let mocked = plan
        .admitted()
        .any(|(_, lane)| lane.plan.chosen == nika_types::access::AccessClass::Mock);
    if !mocked || network_tasks.is_empty() {
        return None;
    }
    let named: Vec<String> = (network_tasks.iter())
        .map(|task| format!("`{task}`"))
        .collect();
    Some(format!(
        "a mock model stubs no tool: {} can still reach the network, judged on the PERMITS row",
        named.join(" · ")
    ))
}
