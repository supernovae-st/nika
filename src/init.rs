//! Project initialization
//!
//! Creates .nika/ directory structure with templates.

use anyhow::Result;
use std::fs;
use std::path::Path;

/// Initialize a new Nika project
pub fn init_project(name: &str, path: &Path) -> Result<InitResult> {
    let project_dir = if name == "." {
        path.to_path_buf()
    } else {
        path.join(name)
    };

    // Create directories
    let nika_dir = project_dir.join(".nika");
    let workflows_dir = nika_dir.join("workflows");
    let tasks_dir = nika_dir.join("tasks");

    if nika_dir.exists() {
        anyhow::bail!(".nika directory already exists");
    }

    fs::create_dir_all(&workflows_dir)?;
    fs::create_dir_all(&tasks_dir)?;

    // Create main workflow
    let main_workflow = project_dir.join("main.nika.yaml");
    fs::write(&main_workflow, MAIN_WORKFLOW_TEMPLATE)?;

    // Create manifest
    let manifest = project_dir.join("nika.yaml");
    let manifest_content = MANIFEST_TEMPLATE.replace("{{name}}", &project_name(name, path));
    fs::write(&manifest, manifest_content)?;

    // Create .gitignore
    let gitignore = nika_dir.join(".gitignore");
    fs::write(&gitignore, GITIGNORE_TEMPLATE)?;

    Ok(InitResult {
        project_dir: project_dir.display().to_string(),
        files_created: vec![
            ".nika/".to_string(),
            ".nika/workflows/".to_string(),
            ".nika/tasks/".to_string(),
            "main.nika.yaml".to_string(),
            "nika.yaml".to_string(),
        ],
    })
}

fn project_name(name: &str, path: &Path) -> String {
    if name == "." {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("my-project")
            .to_string()
    } else {
        name.to_string()
    }
}

/// Result of project initialization
pub struct InitResult {
    pub project_dir: String,
    pub files_created: Vec<String>,
}

const MAIN_WORKFLOW_TEMPLATE: &str = r#"# Main Workflow (v4.7.1)
# Run with: nika run main.nika.yaml

agent:
  model: claude-sonnet-4-5
  systemPrompt: |
    You are a helpful assistant.
    Complete tasks efficiently and accurately.

tasks:
  - id: start
    agent: "Greet the user and ask how you can help."

flows: []
"#;

const MANIFEST_TEMPLATE: &str = r#"# Nika Project Manifest
name: {{name}}
version: 0.1.0
description: A Nika workflow project

# Default workflow
main: main.nika.yaml

# Dependencies (future)
dependencies: {}
"#;

const GITIGNORE_TEMPLATE: &str = r#"# Nika cache
.cache/
*.log

# Secrets (never commit)
.env
secrets/
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_init_project() {
        let temp = tempdir().unwrap();
        let result = init_project("test-project", temp.path()).unwrap();

        assert!(temp.path().join("test-project/.nika").exists());
        assert!(temp.path().join("test-project/main.nika.yaml").exists());
        assert!(temp.path().join("test-project/nika.yaml").exists());
        assert_eq!(result.files_created.len(), 5);
    }

    #[test]
    fn test_init_current_dir() {
        let temp = tempdir().unwrap();
        let result = init_project(".", temp.path()).unwrap();

        assert!(temp.path().join(".nika").exists());
        assert!(temp.path().join("main.nika.yaml").exists());
        assert_eq!(result.files_created.len(), 5);
    }

    #[test]
    fn test_init_already_exists() {
        let temp = tempdir().unwrap();
        fs::create_dir(temp.path().join(".nika")).unwrap();

        let result = init_project(".", temp.path());
        assert!(result.is_err());
    }
}
