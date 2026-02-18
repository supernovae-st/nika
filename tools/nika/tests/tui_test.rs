//! TUI integration tests
//!
//! These tests verify the TUI module compiles and basic state works.
//! Note: Full TUI testing requires manual or headless terminal testing.

#[cfg(feature = "tui")]
mod tui_tests {
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_tui_module_compiles() {
        // Just importing proves the module compiles
        use nika::tui::run_tui;
        // Function exists
        let _ = run_tui;
    }

    #[test]
    fn test_app_creation_with_valid_file() {
        use nika::tui::App;

        let yaml = r#"
schema: "nika/workflow@0.1"
provider: mock
tasks:
  - id: test
    infer:
      prompt: "Hello"
"#;
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(yaml.as_bytes()).unwrap();

        let app = App::new(temp.path());
        assert!(app.is_ok(), "App should be created for valid file");
    }

    #[test]
    fn test_app_creation_with_empty_file() {
        use nika::tui::App;

        // App::new only checks file existence, not content validity
        let temp = NamedTempFile::new().unwrap();

        let app = App::new(temp.path());
        assert!(app.is_ok(), "App should be created for existing file (content validation happens later)");
    }

    #[test]
    fn test_app_creation_with_missing_file() {
        use nika::tui::App;
        use std::path::Path;

        let app = App::new(Path::new("/nonexistent/workflow.yaml"));
        assert!(app.is_err(), "App should fail for missing file");

        // Use match instead of unwrap_err (App doesn't implement Debug)
        match app {
            Err(err) => {
                let msg = err.to_string();
                assert!(
                    msg.contains("Workflow not found") || msg.contains("not found"),
                    "Error message should mention file not found: {}",
                    msg
                );
            }
            Ok(_) => panic!("Expected error for missing file"),
        }
    }
}

#[cfg(not(feature = "tui"))]
mod tui_disabled_tests {
    #[tokio::test]
    async fn test_run_tui_returns_error_when_disabled() {
        use std::path::Path;

        let result = nika::tui::run_tui(Path::new("test.yaml")).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("TUI feature not enabled"),
            "Error message should mention feature: {}",
            msg
        );
    }
}
