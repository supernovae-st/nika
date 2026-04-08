// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exercise hints — progressive hint system
//!
//! Each exercise has 3 hint levels: Conceptual -> Specific -> Solution.
//! Players reveal hints one at a time; using fewer hints earns bonus points.

/// Hint granularity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HintLevel {
    /// High-level concept nudge
    Conceptual,
    /// Specific technique or pattern
    Specific,
    /// Near-complete solution
    Solution,
}

impl HintLevel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Conceptual => "Conceptual",
            Self::Specific => "Specific",
            Self::Solution => "Solution",
        }
    }
}

/// All hints for one exercise (static-friendly — no heap allocation)
#[derive(Debug, Clone)]
pub struct ExerciseHints {
    /// Level number
    pub level: u8,
    /// Exercise number within the level
    pub exercise: u8,
    /// Hints ordered: Conceptual, Specific, Solution
    pub hints: &'static [(HintLevel, &'static str)],
}

/// Get hints for a specific exercise
pub fn get_hints(level: u8, exercise: u8) -> Option<&'static ExerciseHints> {
    HINT_DATA
        .iter()
        .find(|h| h.level == level && h.exercise == exercise)
}

/// Determine the next hint level to reveal based on how many hints were already shown
pub fn next_hint_level(hints_revealed: u32) -> Option<HintLevel> {
    match hints_revealed {
        0 => Some(HintLevel::Conceptual),
        1 => Some(HintLevel::Specific),
        2 => Some(HintLevel::Solution),
        _ => None, // All hints exhausted
    }
}

// ─── Hint data ──────────────────────────────────────────────────────────────
//
// Level 01: Jailbreak (5 exercises)
// More levels will be added in Phase 3/4.

static HINT_DATA: &[ExerciseHints] = &[
    // ── Level 01, Exercise 1: Your first workflow ───────────────────────────
    ExerciseHints {
        level: 1,
        exercise: 1,
        hints: &[
            (
                HintLevel::Conceptual,
                "A workflow needs three things: schema, workflow name, and tasks.",
            ),
            (
                HintLevel::Specific,
                "Start with `schema: \"nika/workflow@0.12\"`, then add `workflow:` and `tasks:` with one `exec:` task.",
            ),
            (
                HintLevel::Solution,
                "schema: \"nika/workflow@0.12\"\nworkflow: hello\ntasks:\n  - id: hello\n    exec:\n      run: echo \"Hello Nika!\"",
            ),
        ],
    },
    // ── Level 01, Exercise 2: Multiple tasks ────────────────────────────────
    ExerciseHints {
        level: 1,
        exercise: 2,
        hints: &[
            (
                HintLevel::Conceptual,
                "Tasks in the tasks: list run in parallel by default. Use depends_on: to order them.",
            ),
            (
                HintLevel::Specific,
                "Add a second task with `depends_on: [first_task_id]` to create a sequence.",
            ),
            (
                HintLevel::Solution,
                "tasks:\n  - id: greet\n    exec:\n      run: echo \"Step 1\"\n  - id: farewell\n    depends_on: [greet]\n    exec:\n      run: echo \"Step 2\"",
            ),
        ],
    },
    // ── Level 01, Exercise 3: Environment variables ─────────────────────────
    ExerciseHints {
        level: 1,
        exercise: 3,
        hints: &[
            (
                HintLevel::Conceptual,
                "The exec: verb can pass environment variables to the shell command.",
            ),
            (
                HintLevel::Specific,
                "Use `env:` under `exec:` to define key-value pairs available in the command.",
            ),
            (
                HintLevel::Solution,
                "- id: with_env\n  exec:\n    run: echo \"Hello $NAME\"\n    env:\n      NAME: \"Nika\"",
            ),
        ],
    },
    // ── Level 01, Exercise 4: Timeouts and error handling ───────────────────
    ExerciseHints {
        level: 1,
        exercise: 4,
        hints: &[
            (
                HintLevel::Conceptual,
                "Commands can hang. Set a timeout to protect your workflow.",
            ),
            (
                HintLevel::Specific,
                "Add `timeout: 5` under `exec:` to limit execution to 5 seconds.",
            ),
            (
                HintLevel::Solution,
                "- id: safe_cmd\n  exec:\n    run: sleep 100\n    timeout: 5\n  on_error: continue",
            ),
        ],
    },
    // ── Level 01, Exercise 5: Complete Jailbreak workflow ────────────────────
    ExerciseHints {
        level: 1,
        exercise: 5,
        hints: &[
            (
                HintLevel::Conceptual,
                "Combine everything: multiple tasks, depends_on, env vars, and timeouts in one workflow.",
            ),
            (
                HintLevel::Specific,
                "Create 3+ tasks forming a DAG. At least one should use env: and one should have timeout:.",
            ),
            (
                HintLevel::Solution,
                "Build a workflow with: schema, workflow name, 3 tasks with depends_on chains, env vars, and a timeout. See 01-exec.nika.yaml for the pattern.",
            ),
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level1_has_5_exercises() {
        let count = HINT_DATA.iter().filter(|h| h.level == 1).count();
        assert_eq!(count, 5, "Level 1 should have hints for 5 exercises");
    }

    #[test]
    fn test_each_exercise_has_3_hints() {
        for hints in HINT_DATA {
            assert_eq!(
                hints.hints.len(),
                3,
                "Exercise {}-{} should have exactly 3 hints",
                hints.level,
                hints.exercise
            );
        }
    }

    #[test]
    fn test_hint_order() {
        for hints in HINT_DATA {
            assert_eq!(hints.hints[0].0, HintLevel::Conceptual);
            assert_eq!(hints.hints[1].0, HintLevel::Specific);
            assert_eq!(hints.hints[2].0, HintLevel::Solution);
        }
    }

    #[test]
    fn test_get_hints_found() {
        let hints = get_hints(1, 1);
        assert!(hints.is_some());
        let h = hints.unwrap();
        assert_eq!(h.level, 1);
        assert_eq!(h.exercise, 1);
    }

    #[test]
    fn test_get_hints_not_found() {
        assert!(get_hints(99, 1).is_none());
        assert!(get_hints(1, 99).is_none());
    }

    #[test]
    fn test_next_hint_level() {
        assert_eq!(next_hint_level(0), Some(HintLevel::Conceptual));
        assert_eq!(next_hint_level(1), Some(HintLevel::Specific));
        assert_eq!(next_hint_level(2), Some(HintLevel::Solution));
        assert_eq!(next_hint_level(3), None);
    }

    #[test]
    fn test_hint_level_ordering() {
        assert!(HintLevel::Conceptual < HintLevel::Specific);
        assert!(HintLevel::Specific < HintLevel::Solution);
    }

    #[test]
    fn test_hint_level_labels() {
        assert_eq!(HintLevel::Conceptual.label(), "Conceptual");
        assert_eq!(HintLevel::Specific.label(), "Specific");
        assert_eq!(HintLevel::Solution.label(), "Solution");
    }

    #[test]
    fn test_hints_not_empty() {
        for hints in HINT_DATA {
            for (_, text) in hints.hints {
                assert!(!text.is_empty(), "Hint text must not be empty");
            }
        }
    }
}
