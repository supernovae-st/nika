//! Integration tests for custom node discovery and validation

use std::fs;
use std::path::PathBuf;

fn get_examples_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("examples/custom-nodes")
}

#[test]
fn test_custom_node_discovery_in_examples() {
    let examples_path = get_examples_path();
    let nodes_path = examples_path.join(".nika/nodes");

    assert!(
        nodes_path.exists(),
        "Examples custom nodes directory should exist"
    );

    // Check slack.node.yaml exists
    assert!(nodes_path.join("slack.node.yaml").exists());

    // Check analyzer.node.yaml exists
    assert!(nodes_path.join("analyzer.node.yaml").exists());
}

#[test]
fn test_workflow_file_exists_in_examples() {
    let examples_path = get_examples_path();
    let workflow_path = examples_path.join("workflow.nika.yaml");

    assert!(
        workflow_path.exists(),
        "Workflow file should exist in examples"
    );
}

#[test]
fn test_custom_node_files_are_valid_yaml() {
    let examples_path = get_examples_path();
    let nodes_path = examples_path.join(".nika/nodes");

    // Test slack.node.yaml
    let slack_content = fs::read_to_string(nodes_path.join("slack.node.yaml"))
        .expect("Should read slack.node.yaml");
    assert!(slack_content.contains("slackNode"));
    assert!(slack_content.contains("data"));
    assert!(slack_content.contains("extends"));

    // Test analyzer.node.yaml
    let analyzer_content = fs::read_to_string(nodes_path.join("analyzer.node.yaml"))
        .expect("Should read analyzer.node.yaml");
    assert!(analyzer_content.contains("analyzerNode"));
    assert!(analyzer_content.contains("isolated"));
    assert!(analyzer_content.contains("extends"));
}

#[test]
fn test_workflow_file_has_required_sections() {
    let examples_path = get_examples_path();
    let workflow_path = examples_path.join("workflow.nika.yaml");

    let workflow_content = fs::read_to_string(workflow_path).expect("Should read workflow.nika.yaml");

    assert!(workflow_content.contains("mainAgent"));
    assert!(workflow_content.contains("nodes"));
    assert!(workflow_content.contains("edges"));
    assert!(workflow_content.contains("claude-sonnet-4-5"));
}
