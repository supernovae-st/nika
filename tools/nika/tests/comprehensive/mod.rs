//! Comprehensive test module for challenging edge cases.
//!
//! Tests:
//! - All 5 verbs in complex combinations
//! - Large DAGs with 100+ tasks
//! - Edge cases (deep chains, wide fan-in/out)
//! - Invalid workflow detection
//! - Error handling
//! - MCP integration scenarios
//! - Agent sniper tests (all AgentParams)

pub mod agent_sniper_tests;
pub mod all_verbs_tests;
pub mod edge_case_tests;
pub mod error_handling_tests;
pub mod invalid_workflow_tests;
pub mod large_dag_tests;
