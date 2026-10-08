// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::path::Path;

use super::{Basis, Place, PlaceKind, Reach, World};
use crate::change::check_on_disk;

fn tasks(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_owned()).collect()
}

fn world(endpoints: &[(&str, &str, Vec<String>)], credentials: &[&str]) -> World {
    World::declared(
        endpoints
            .iter()
            .map(|(kind, target, tasks)| (*kind, *target, tasks.as_slice())),
        credentials.iter().copied(),
    )
}

fn kinds(world: &World) -> Vec<(PlaceKind, &str)> {
    world
        .places
        .iter()
        .map(|p| (p.kind, p.target.as_str()))
        .collect()
}

#[test]
fn files_alone_reach_nothing_outside_this_machine() {
    let w = world(
        &[
            ("fs.read", "./world/pages", tasks(&["pages"])),
            ("fs.write", "./out/report.json", tasks(&["report"])),
        ],
        &[],
    );
    assert_eq!(w.basis, Basis::Declared);
    assert_eq!(w.reach, Reach::Local);
    assert_eq!(
        kinds(&w),
        [
            (PlaceKind::FileRead, "./world/pages"),
            (PlaceKind::FileWrite, "./out/report.json")
        ]
    );
    assert_eq!(w.summary(), "local only · files, no service reached");
}

#[test]
fn an_empty_journey_says_nothing_leaves_the_process() {
    let w = world(&[], &[]);
    assert_eq!(w.reach, Reach::Local);
    assert_eq!(w.summary(), "local only · nothing outside the process");
}

#[test]
fn an_exact_loopback_host_is_a_local_service_never_the_real_one() {
    for host in ["127.0.0.1", "localhost", "::1", "[::1]"] {
        let w = world(&[("net.http", host, tasks(&["notify"]))], &[]);
        assert_eq!(w.reach, Reach::LocalServices, "{host}");
        assert_eq!(kinds(&w), [(PlaceKind::LocalService, host)], "{host}");
    }
    let w = world(
        &[
            ("fs.read", "./world/pages", tasks(&["pages"])),
            ("net.http", "127.0.0.1", tasks(&["notify"])),
        ],
        &[],
    );
    assert_eq!(
        w.summary(),
        "local services only: 127.0.0.1 · no connected service"
    );
}

#[test]
fn a_public_host_is_a_connected_service_whatever_else_is_local() {
    let w = world(
        &[
            ("fs.read", "./world/pages", tasks(&["pages"])),
            ("net.http", "127.0.0.1", tasks(&["sink"])),
            ("net.http", "hooks.slack.com", tasks(&["notify"])),
            ("mcp.tool", "mcp:shopify/list_products", tasks(&["stock"])),
        ],
        &["slack_hook", "shopify_token", "slack_hook"],
    );
    assert_eq!(w.reach, Reach::Connected);
    assert_eq!(
        w.credentials,
        ["shopify_token", "slack_hook"],
        "sorted, deduplicated names"
    );
    assert_eq!(
        w.summary(),
        "connected: hooks.slack.com · credentials: shopify_token, slack_hook"
    );
}

#[test]
fn a_tool_server_or_a_program_leaves_the_reach_undetermined() {
    let w = world(
        &[("mcp.tool", "mcp:slack/post_message", tasks(&["notify"]))],
        &[],
    );
    assert_eq!(w.reach, Reach::Undetermined);
    assert_eq!(w.summary(), "reach undetermined: mcp:slack/post_message");
    let w = world(
        &[
            ("net.http", "127.0.0.1", tasks(&["sink"])),
            ("exec", "curl", tasks(&["post"])),
        ],
        &[],
    );
    assert_eq!(
        w.reach,
        Reach::Undetermined,
        "a program may contact anything: a loopback host does not settle it"
    );
    assert_eq!(w.summary(), "reach undetermined: curl");
}

#[test]
fn placeholder_and_floor_refused_hosts_reach_no_service() {
    let w = world(
        &[
            ("net.http", "api.shop.example.com", tasks(&["check"])),
            ("net.http", "10.0.0.5", tasks(&["internal"])),
            ("net.http", "169.254.169.254", tasks(&["metadata"])),
        ],
        &[],
    );
    assert_eq!(
        kinds(&w),
        [
            (PlaceKind::Placeholder, "api.shop.example.com"),
            (PlaceKind::Refused, "10.0.0.5"),
            (PlaceKind::Refused, "169.254.169.254"),
        ]
    );
    assert_eq!(w.reach, Reach::Local);
}

#[test]
fn an_unknown_endpoint_kind_is_kept_and_never_called_local() {
    let w = world(&[("queue.publish", "orders", tasks(&["emit"]))], &[]);
    assert_eq!(
        w.places,
        [Place {
            kind: PlaceKind::Unclassified,
            target: "orders".to_owned(),
            tasks: tasks(&["emit"]),
        }]
    );
    assert_eq!(w.reach, Reach::Undetermined);
}

#[test]
fn unaudited_bytes_claim_no_reach() {
    let w = World::default();
    assert_eq!(w.basis, Basis::NotAudited);
    assert_eq!(w.reach, Reach::Undetermined);
    assert_eq!(w.summary(), "reach unknown · the bytes were not audited");
}

#[test]
fn the_wire_shape_is_snake_case_and_carries_names_only() {
    let w = world(
        &[("net.http", "hooks.slack.com", tasks(&["notify"]))],
        &["slack_hook"],
    );
    let json = serde_json::to_value(&w).expect("serializes");
    assert_eq!(
        json,
        serde_json::json!({
            "basis": "declared",
            "reach": "connected",
            "places": [{"kind": "connected_service", "target": "hooks.slack.com", "tasks": ["notify"]}],
            "credentials": ["slack_hook"],
        })
    );
}

fn audited(root: &Path, name: &str, source: &str) -> World {
    std::fs::write(root.join(name), source).expect("workflow");
    let audit = check_on_disk(root, Path::new(name));
    assert!(
        audit.findings.iter().all(|f| !f.starts_with("NIKA-PARSE")),
        "{name} parses: {:?}",
        audit.findings
    );
    audit.world
}

const FIXTURE: &str = r#"nika: stock-fixture

permits:
  tools: ["nika:glob", "nika:write"]
  fs:
    read: ["./world/pages/**"]
    write: ["./out/**"]

tasks:
  pages:
    invoke:
      tool: "nika:glob"
      args:
        pattern: "./world/pages/*.json"
  report:
    with:
      pages: ${{ tasks.pages }}
    invoke:
      tool: "nika:write"
      args:
        path: "./out/report.json"
        content: "${{ with.pages }}"
"#;

const LOCAL_SINK: &str = r#"nika: stock-sink

permits:
  tools: ["nika:fetch"]
  net:
    http: ["127.0.0.1"]

tasks:
  notify:
    invoke:
      tool: "nika:fetch"
      args:
        url: "http://127.0.0.1:8787/notifications/stock"
        method: POST
        headers:
          idempotency-key: "stock-1"
        body: { channel: "stock", item_ids: ["v-101"] }
"#;

const CONNECTED: &str = r#"nika: stock-connected

secrets:
  slack_hook:
    source: env
    key: SLACK_WEBHOOK_URL
    egress:
      - to: "nika:notify"
        host_from_self: true

permits:
  tools: ["nika:notify"]
  net:
    http: ["hooks.slack.com"]

tasks:
  notify:
    invoke:
      tool: "nika:notify"
      args:
        channel: webhook
        target: "${{ secrets.slack_hook }}"
        message: "stock low"
"#;

#[test]
fn the_audit_of_exact_bytes_tells_fixture_local_service_and_connected_apart() {
    let root = tempfile::tempdir().expect("root");
    let fixture = audited(root.path(), "fixture.nika", FIXTURE);
    assert_eq!(fixture.basis, Basis::Declared);
    assert_eq!(fixture.reach, Reach::Local, "{fixture:?}");
    assert!(
        fixture
            .of_kind(PlaceKind::FileWrite)
            .any(|p| p.target.contains("out/report.json")),
        "{fixture:?}"
    );

    let sink = audited(root.path(), "sink.nika", LOCAL_SINK);
    assert_eq!(sink.reach, Reach::LocalServices, "{sink:?}");
    assert_eq!(
        sink.of_kind(PlaceKind::LocalService)
            .map(|p| p.target.as_str())
            .collect::<Vec<_>>(),
        ["127.0.0.1"]
    );

    let connected = audited(root.path(), "connected.nika", CONNECTED);
    assert_eq!(connected.reach, Reach::Connected, "{connected:?}");
    assert_eq!(connected.credentials, ["slack_hook"]);
    assert!(
        connected
            .of_kind(PlaceKind::ConnectedService)
            .any(|p| p.target == "hooks.slack.com"),
        "{connected:?}"
    );
}

#[test]
fn a_workflow_that_cannot_be_read_claims_no_reach() {
    let root = tempfile::tempdir().expect("root");
    let audit = check_on_disk(root.path(), Path::new("missing.nika"));
    assert!(!audit.clean);
    assert_eq!(audit.world, World::default());
}

/// A workflow whose only effect is remote inference: nothing is read or written, yet its
/// prompt leaves this machine for the provider.
const REMOTE_INFER: &str = "nika: summarize-remote\nmodel: openai/gpt-4o-mini\npermits: {}\ntasks:\n  summarize:\n    infer: { prompt: \"Summarize the quarter in two sentences.\", max_tokens: 64 }\n";

/// The same shape whose model is chosen by the caller when the run starts.
const CHOSEN_AT_RUN: &str = "nika: summarize-chosen\ninputs:\n  model: { type: string, required: true }\npermits: {}\ntasks:\n  summarize:\n    infer: { model: \"${{ inputs.model }}\", prompt: \"Summarize.\", max_tokens: 64 }\n";

/// Inference on a model the catalog places on this machine.
const MOCK_INFER: &str = "nika: summarize-mock\nmodel: mock/echo\npermits: {}\ntasks:\n  think:\n    infer: { prompt: \"hi\", max_tokens: 5 }\n";

#[test]
fn remote_inference_is_a_declared_connected_reach_never_local() {
    let root = tempfile::tempdir().expect("root");
    let remote = audited(root.path(), "remote.nika", REMOTE_INFER);
    // Before the fix the model endpoint was dropped: reach Local, « nothing outside the process ».
    assert_eq!(
        remote.basis,
        Basis::Declared,
        "a plan, never an observation"
    );
    assert_eq!(remote.reach, Reach::Connected, "{remote:?}");
    assert_eq!(
        remote.places,
        [Place {
            kind: PlaceKind::RemoteModel,
            target: "openai/gpt-4o-mini".to_owned(),
            tasks: tasks(&["summarize"]),
        }]
    );
    assert_eq!(
        remote.summary(),
        "connected: openai/gpt-4o-mini (remote model)"
    );
    assert!(!remote.summary().contains("nothing outside"));
}

#[test]
fn a_model_chosen_when_the_run_starts_leaves_the_reach_undetermined() {
    let root = tempfile::tempdir().expect("root");
    let chosen = audited(root.path(), "chosen.nika", CHOSEN_AT_RUN);
    assert_eq!(chosen.reach, Reach::Undetermined, "{chosen:?}");
    assert_eq!(
        kinds(&chosen),
        [(PlaceKind::UndeterminedModel, "${{ inputs.model }}")]
    );
    assert_eq!(
        chosen.summary(),
        "reach undetermined: ${{ inputs.model }} (model undetermined)"
    );
}

#[test]
fn a_model_on_this_machine_is_named_and_nothing_leaves() {
    let root = tempfile::tempdir().expect("root");
    let mock = audited(root.path(), "mock.nika", MOCK_INFER);
    assert_eq!(mock.reach, Reach::Local, "{mock:?}");
    assert_eq!(kinds(&mock), [(PlaceKind::LocalModel, "mock/echo")]);
    assert_eq!(
        mock.summary(),
        "local only · mock/echo (local model) · nothing leaves this machine"
    );
}

#[test]
fn model_kinds_rank_with_hosts_in_the_reach() {
    let w = world(
        &[
            ("model.local", "mock/echo", tasks(&["a"])),
            ("net.http", "127.0.0.1", tasks(&["b"])),
        ],
        &[],
    );
    assert_eq!(w.reach, Reach::LocalServices);
    assert_eq!(
        w.summary(),
        "local services only: mock/echo (local model), 127.0.0.1 · no connected service"
    );
    let w = world(
        &[
            ("model.undetermined", "acme/unknown", tasks(&["a"])),
            ("model.remote", "anthropic/claude-x", tasks(&["b"])),
        ],
        &[],
    );
    assert_eq!(
        w.reach,
        Reach::Connected,
        "a remote model outranks the unknown"
    );
    assert_eq!(
        serde_json::to_value(&w.places[1]).expect("place")["kind"],
        "remote_model"
    );
}
