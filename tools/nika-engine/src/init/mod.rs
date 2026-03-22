//! # Nika Init — Project scaffolding
//!
//! Creates a new Nika project with example workflows organized in 6 tiers.

mod context;
mod partials;
mod schemas;
pub mod tier1;
pub mod tier2;
pub mod tier3;
pub mod tier4;
pub mod tier5;
pub mod tier6;

pub use context::*;
pub use partials::*;
pub use schemas::*;

/// Alias for backward compat with nika-cli
pub fn get_all_partials() -> Vec<ContextFile> {
    partials::get_partial_files()
}

/// Workflow definition with metadata
pub struct WorkflowTemplate {
    pub filename: &'static str,
    pub tier_dir: &'static str,
    pub content: &'static str,
}

/// Context file definition
pub struct ContextFile {
    pub filename: &'static str,
    pub dir: &'static str,
    pub content: &'static str,
}

pub fn get_all_workflows() -> Vec<WorkflowTemplate> {
    let mut all = Vec::new();
    all.extend(tier1::get_tier1_workflows());
    all.extend(tier2::get_tier2_workflows());
    all.extend(tier3::get_tier3_workflows());
    all.extend(tier4::get_tier4_workflows());
    all.extend(tier5::get_tier5_workflows());
    all.extend(tier6::get_tier6_workflows());
    all
}

pub fn get_all_context_files() -> Vec<ContextFile> {
    context::get_context_files()
}

pub fn get_all_schemas() -> Vec<ContextFile> {
    schemas::get_schema_files()
}

pub const WORKFLOWS_README: &str = "# Nika Workflows

5 starter workflows.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_workflows_exist() {
        assert_eq!(get_all_workflows().len(), 30);
    }

    #[test]
    fn test_workflow_filenames_unique() {
        let workflows = get_all_workflows();
        let mut f: Vec<&str> = workflows.iter().map(|w| w.filename).collect();
        let n = f.len();
        f.sort();
        f.dedup();
        assert_eq!(f.len(), n);
    }

    #[test]
    fn test_workflows_have_schema() {
        for w in get_all_workflows() {
            assert!(w.content.contains("nika/workflow@0.12"));
        }
    }

    #[test]
    fn test_workflows_have_tasks() {
        for w in get_all_workflows() {
            assert!(w.content.contains("tasks:"));
        }
    }

    #[test]
    fn test_context_files_exist() {
        assert!(get_all_context_files().len() >= 2);
    }

    #[test]
    fn test_schema_files_valid_json() {
        for f in get_all_schemas() {
            let p: Result<serde_json::Value, _> = serde_json::from_str(f.content);
            assert!(p.is_ok(), "{} invalid", f.filename);
        }
    }

    #[test]
    fn test_workflows_valid_yaml() {
        for w in get_all_workflows() {
            let p: Result<serde_json::Value, _> = crate::serde_yaml::from_str(w.content);
            assert!(p.is_ok(), "{} invalid", w.filename);
        }
    }

}
