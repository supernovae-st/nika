// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exercise checks — validation assertions on raw YAML text
//!
//! Each check function examines the user's workflow YAML to verify
//! that specific patterns are present. Used by the course engine
//! to grade exercises automatically.

/// Verdict for a single check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckVerdict {
    /// The check passed
    Pass,
    /// The check failed with a reason
    Fail(String),
    /// Bonus achievement unlocked
    Bonus(String),
}

impl CheckVerdict {
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass | Self::Bonus(_))
    }
}

/// Result of a single check
#[derive(Debug, Clone)]
pub struct CheckResult {
    /// Human-readable name of the check
    pub name: &'static str,
    /// The verdict
    pub verdict: CheckVerdict,
}

/// Full report for one exercise
#[derive(Debug, Clone)]
pub struct ExerciseReport {
    /// Exercise identifier (e.g., "01-03")
    pub exercise_id: String,
    /// All check results
    pub checks: Vec<CheckResult>,
}

impl ExerciseReport {
    /// Did all required checks pass?
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.verdict.is_pass())
    }

    /// Count of bonus achievements
    pub fn bonus_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| matches!(c.verdict, CheckVerdict::Bonus(_)))
            .count()
    }
}

/// Full report for one level
#[derive(Debug, Clone)]
pub struct LevelReport {
    /// Level number
    pub level: u8,
    /// Exercise reports
    pub exercises: Vec<ExerciseReport>,
}

impl LevelReport {
    /// Did all exercises in the level pass?
    pub fn all_passed(&self) -> bool {
        self.exercises.iter().all(|e| e.passed())
    }

    /// How many exercises passed?
    pub fn pass_count(&self) -> usize {
        self.exercises.iter().filter(|e| e.passed()).count()
    }
}

// ─── Assertion functions ────────────────────────────────────────────────────

/// Check that the YAML contains no TODO/FIXME/XXX placeholders
pub fn check_no_todos(yaml: &str) -> CheckResult {
    let has_todos = yaml.lines().any(|line| {
        let trimmed = line.trim_start();
        // Skip comment lines — templates use # TODO: as instructions
        if trimmed.starts_with('#') {
            return false;
        }
        trimmed.contains("TODO") || trimmed.contains("FIXME") || trimmed.contains("XXX")
    });
    CheckResult {
        name: "no_todos",
        verdict: if has_todos {
            CheckVerdict::Fail("Workflow still contains TODO/FIXME/XXX placeholders".into())
        } else {
            CheckVerdict::Pass
        },
    }
}

/// Check that the YAML contains a specific verb (e.g., "exec", "fetch", "infer", "invoke", "agent")
pub fn check_has_verb(yaml: &str, verb: &str) -> CheckResult {
    let pattern = format!("{}:", verb);
    CheckResult {
        name: "has_verb",
        verdict: if yaml.contains(&pattern) {
            CheckVerdict::Pass
        } else {
            CheckVerdict::Fail(format!("Expected verb '{}:' not found", verb))
        },
    }
}

/// Check that the YAML uses depends_on for task ordering
pub fn check_has_depends_on(yaml: &str) -> CheckResult {
    CheckResult {
        name: "has_depends_on",
        verdict: if yaml.contains("depends_on:") || yaml.contains("depends_on: [") {
            CheckVerdict::Pass
        } else {
            CheckVerdict::Fail("No depends_on: found — tasks should declare dependencies".into())
        },
    }
}

/// Check that the YAML uses for_each for parallel iteration
pub fn check_has_for_each(yaml: &str) -> CheckResult {
    CheckResult {
        name: "has_for_each",
        verdict: if yaml.contains("for_each:") {
            CheckVerdict::Pass
        } else {
            CheckVerdict::Fail("No for_each: found — expected parallel iteration".into())
        },
    }
}

/// Check that the YAML uses with: bindings
pub fn check_has_with_bindings(yaml: &str) -> CheckResult {
    // Look for `with:` followed by binding patterns (key: $task_id)
    let has_with = yaml.contains("with:")
        && (yaml.contains(": $") || yaml.contains(": \"$") || yaml.contains(": '$"));
    CheckResult {
        name: "has_with_bindings",
        verdict: if has_with {
            CheckVerdict::Pass
        } else {
            CheckVerdict::Fail("No with: bindings found — use with: { alias: $task_id }".into())
        },
    }
}

/// Check that the YAML declares the workflow schema
pub fn check_has_schema(yaml: &str) -> CheckResult {
    CheckResult {
        name: "has_schema",
        verdict: if yaml.contains("schema:") && yaml.contains("nika/workflow@0.12") {
            CheckVerdict::Pass
        } else {
            CheckVerdict::Fail("Missing schema declaration: schema: \"nika/workflow@0.12\"".into())
        },
    }
}

/// Check that the YAML has at least N tasks
pub fn check_min_tasks(yaml: &str, min: usize) -> CheckResult {
    // Count occurrences of `- id:` which marks task definitions
    let count = yaml.matches("- id:").count();
    CheckResult {
        name: "min_tasks",
        verdict: if count >= min {
            CheckVerdict::Pass
        } else {
            CheckVerdict::Fail(format!(
                "Expected at least {} tasks but found {}",
                min, count
            ))
        },
    }
}

// ─── Bonus checks ───────────────────────────────────────────────────────────

/// Bonus: completed without using any hints
pub fn bonus_no_hints_used(hints_used: u32) -> CheckResult {
    CheckResult {
        name: "bonus_no_hints",
        verdict: if hints_used == 0 {
            CheckVerdict::Bonus("Completed without hints!".into())
        } else {
            CheckVerdict::Pass // Not a failure, just no bonus
        },
    }
}

/// Bonus: passed on the first attempt (no prior Attempted status)
pub fn bonus_first_try(is_first_try: bool) -> CheckResult {
    CheckResult {
        name: "bonus_first_try",
        verdict: if is_first_try {
            CheckVerdict::Bonus("First try!".into())
        } else {
            CheckVerdict::Pass // Not a failure, just no bonus
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_YAML: &str = r#"
schema: "nika/workflow@0.12"
workflow: test
tasks:
  - id: step1
    exec:
      run: echo "hello"
  - id: step2
    depends_on: [step1]
    with:
      result: $step1
    infer:
      prompt: "{{with.result}}"
"#;

    #[test]
    fn test_check_no_todos_pass() {
        let result = check_no_todos(SAMPLE_YAML);
        assert!(result.verdict.is_pass());
    }

    #[test]
    fn test_check_no_todos_fail() {
        let yaml = "tasks:\n  - id: a\n    exec: \"TODO: fix this\"";
        let result = check_no_todos(yaml);
        assert!(!result.verdict.is_pass());
    }

    #[test]
    fn test_check_has_verb_found() {
        let result = check_has_verb(SAMPLE_YAML, "exec");
        assert!(result.verdict.is_pass());
    }

    #[test]
    fn test_check_has_verb_missing() {
        let result = check_has_verb(SAMPLE_YAML, "fetch");
        assert!(!result.verdict.is_pass());
    }

    #[test]
    fn test_check_has_depends_on() {
        let result = check_has_depends_on(SAMPLE_YAML);
        assert!(result.verdict.is_pass());
    }

    #[test]
    fn test_check_has_depends_on_missing() {
        let yaml = "tasks:\n  - id: a\n    exec:\n      run: echo hi";
        let result = check_has_depends_on(yaml);
        assert!(!result.verdict.is_pass());
    }

    #[test]
    fn test_check_has_for_each() {
        let yaml = "tasks:\n  - id: a\n    for_each: [1, 2]\n    exec:\n      run: echo hi";
        let result = check_has_for_each(yaml);
        assert!(result.verdict.is_pass());
    }

    #[test]
    fn test_check_has_for_each_missing() {
        let result = check_has_for_each(SAMPLE_YAML);
        assert!(!result.verdict.is_pass());
    }

    #[test]
    fn test_check_has_with_bindings() {
        let result = check_has_with_bindings(SAMPLE_YAML);
        assert!(result.verdict.is_pass());
    }

    #[test]
    fn test_check_has_schema() {
        let result = check_has_schema(SAMPLE_YAML);
        assert!(result.verdict.is_pass());
    }

    #[test]
    fn test_check_has_schema_missing() {
        let yaml = "workflow: test\ntasks:\n  - id: a\n    exec:\n      run: echo hi";
        let result = check_has_schema(yaml);
        assert!(!result.verdict.is_pass());
    }

    #[test]
    fn test_check_min_tasks_pass() {
        let result = check_min_tasks(SAMPLE_YAML, 2);
        assert!(result.verdict.is_pass());
    }

    #[test]
    fn test_check_min_tasks_fail() {
        let result = check_min_tasks(SAMPLE_YAML, 5);
        assert!(!result.verdict.is_pass());
    }

    #[test]
    fn test_bonus_no_hints() {
        let result = bonus_no_hints_used(0);
        assert!(matches!(result.verdict, CheckVerdict::Bonus(_)));
    }

    #[test]
    fn test_bonus_no_hints_used_some() {
        let result = bonus_no_hints_used(3);
        assert_eq!(result.verdict, CheckVerdict::Pass);
    }

    #[test]
    fn test_bonus_first_try() {
        let result = bonus_first_try(true);
        assert!(matches!(result.verdict, CheckVerdict::Bonus(_)));
    }

    #[test]
    fn test_bonus_first_try_not() {
        let result = bonus_first_try(false);
        assert_eq!(result.verdict, CheckVerdict::Pass);
    }

    #[test]
    fn test_exercise_report_passed() {
        let report = ExerciseReport {
            exercise_id: "01-01".into(),
            checks: vec![
                check_has_schema(SAMPLE_YAML),
                check_has_verb(SAMPLE_YAML, "exec"),
                check_min_tasks(SAMPLE_YAML, 2),
            ],
        };
        assert!(report.passed());
    }

    #[test]
    fn test_exercise_report_failed() {
        let report = ExerciseReport {
            exercise_id: "01-01".into(),
            checks: vec![
                check_has_schema(SAMPLE_YAML),
                check_has_verb(SAMPLE_YAML, "agent"), // missing
            ],
        };
        assert!(!report.passed());
    }

    #[test]
    fn test_exercise_report_bonus_count() {
        let report = ExerciseReport {
            exercise_id: "01-01".into(),
            checks: vec![
                check_has_schema(SAMPLE_YAML),
                bonus_no_hints_used(0),
                bonus_first_try(true),
            ],
        };
        assert_eq!(report.bonus_count(), 2);
    }

    #[test]
    fn test_level_report() {
        let report = LevelReport {
            level: 1,
            exercises: vec![
                ExerciseReport {
                    exercise_id: "01-01".into(),
                    checks: vec![check_has_schema(SAMPLE_YAML)],
                },
                ExerciseReport {
                    exercise_id: "01-02".into(),
                    checks: vec![check_has_schema(SAMPLE_YAML)],
                },
            ],
        };
        assert!(report.all_passed());
        assert_eq!(report.pass_count(), 2);
    }

    #[test]
    fn test_check_verdict_is_pass() {
        assert!(CheckVerdict::Pass.is_pass());
        assert!(CheckVerdict::Bonus("nice".into()).is_pass());
        assert!(!CheckVerdict::Fail("bad".into()).is_pass());
    }

    #[test]
    fn test_check_no_todos_ignores_comments() {
        let yaml = "schema: \"nika/workflow@0.12\"\nworkflow: test\n# TODO: This is a guide comment\ntasks:\n  - id: hello\n    exec: \"echo hello\"\n";
        let result = check_no_todos(yaml);
        assert!(result.verdict.is_pass(), "TODO in comments should not fail");
    }

    #[test]
    fn test_check_no_todos_catches_inline_todos() {
        let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: hello\n    exec: \"TODO implement this\"\n";
        let result = check_no_todos(yaml);
        assert!(!result.verdict.is_pass(), "TODO in values should fail");
    }
}
