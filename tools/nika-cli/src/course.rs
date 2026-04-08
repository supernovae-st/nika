// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Course subcommand handler — interactive learning CLI
//!
//! 8 subcommands: status, next, check, hint, reset, run, info, watch

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use clap::Subcommand;
use colored::Colorize;

use nika_engine::error::NikaError;
use nika_init::course::{
    checks::{
        check_has_depends_on, check_has_schema, check_has_verb, check_has_with_bindings,
        check_min_tasks, check_no_todos, CheckVerdict, ExerciseReport, LevelReport,
    },
    exercises,
    hints::{get_hints, next_hint_level},
    levels::{self, Level, LEVELS},
    progress::{CourseProgress, ExerciseStatus, LevelStatus},
};

/// Course subcommand actions
#[derive(Subcommand)]
pub enum CourseAction {
    /// Show course progress — constellation map
    Status,
    /// Show the next exercise to work on
    Next,
    /// Check an exercise or level (validates YAML)
    Check {
        /// Level number, slug, or name (e.g., "1", "jailbreak", "Jailbreak")
        level: Option<String>,
    },
    /// Show progressive hints for an exercise
    Hint {
        /// Exercise ID like "01-03" (level-exercise)
        exercise: Option<String>,
    },
    /// Reset a level to start over
    Reset {
        /// Level number, slug, or name
        level: String,
    },
    /// Run a course exercise workflow
    Run {
        /// Exercise ID like "01-03" (level-exercise)
        exercise: String,
    },
    /// Show detailed info about a level or the whole course
    Info {
        /// Level number, slug, or name (omit for overview)
        level: Option<String>,
    },
    /// Watch exercise files and auto-check on save
    Watch,
}

/// Entry point for `nika course <action>`
pub fn handle_course_command(action: CourseAction, _quiet: bool) -> Result<(), NikaError> {
    match action {
        CourseAction::Status => cmd_status(),
        CourseAction::Next => cmd_next(),
        CourseAction::Check { level } => cmd_check(level),
        CourseAction::Hint { exercise } => cmd_hint(exercise),
        CourseAction::Reset { level } => cmd_reset(&level),
        CourseAction::Run { exercise } => cmd_run(&exercise),
        CourseAction::Info { level } => cmd_info(level),
        CourseAction::Watch => cmd_watch(),
    }
}

// ─── Course root discovery ──────────────────────────────────────────────────

/// Progress file path relative to course root
const PROGRESS_FILE: &str = ".nika/course-progress.toml";

/// Find the course root by walking up from cwd looking for
/// `.nika/course-progress.toml` or a `course/` level directory.
fn find_course_root() -> Result<PathBuf, NikaError> {
    let cwd = std::env::current_dir()?;
    let mut dir = cwd.as_path();

    loop {
        // Check for progress file
        if dir.join(PROGRESS_FILE).exists() {
            return Ok(dir.to_path_buf());
        }
        // Check for course level directories (e.g., 01-jailbreak/)
        if dir.join("01-jailbreak").is_dir() {
            return Ok(dir.to_path_buf());
        }
        // Check for .nika/ directory (project root)
        if dir.join(".nika").is_dir() {
            return Ok(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => {
                return Err(NikaError::ValidationError {
                    reason: "Not inside a Nika project. Run `nika init` first.".to_string(),
                });
            }
        }
    }
}

/// Load or initialize course progress
fn load_progress(root: &Path) -> Result<CourseProgress, NikaError> {
    let path = root.join(PROGRESS_FILE);
    if path.exists() {
        CourseProgress::load(&path).map_err(|e| NikaError::ConfigError {
            reason: e.to_string(),
        })
    } else {
        Ok(CourseProgress::new_course())
    }
}

/// Save course progress
fn save_progress(root: &Path, progress: &mut CourseProgress) -> Result<(), NikaError> {
    let path = root.join(PROGRESS_FILE);
    progress.save(&path).map_err(|e| NikaError::ConfigError {
        reason: e.to_string(),
    })
}

// ─── Level resolution ───────────────────────────────────────────────────────

/// Resolve a user-provided level identifier to a Level.
///
/// Accepts: number ("1", "01"), slug ("jailbreak"), or name ("Jailbreak").
pub fn resolve_level(input: &str) -> Result<&'static Level, NikaError> {
    // Try as number
    if let Ok(n) = input.parse::<u8>() {
        if let Some(level) = levels::by_number(n) {
            return Ok(level);
        }
    }

    // Try as zero-padded number (e.g., "01")
    let trimmed = input.trim_start_matches('0');
    if !trimmed.is_empty() {
        if let Ok(n) = trimmed.parse::<u8>() {
            if let Some(level) = levels::by_number(n) {
                return Ok(level);
            }
        }
    }

    // Try as slug (case-insensitive)
    let lower = input.to_lowercase();
    if let Some(level) = levels::by_slug(&lower) {
        return Ok(level);
    }

    // Try as name (case-insensitive)
    if let Some(level) = LEVELS.iter().find(|l| l.name.to_lowercase() == lower) {
        return Ok(level);
    }

    Err(NikaError::ValidationError {
        reason: format!(
            "Unknown level '{}'. Use a number (1-12), slug, or name. Try: nika course info",
            input
        ),
    })
}

/// Parse an exercise ID like "01-03" -> (level_num, exercise_num)
fn parse_exercise_id(id: &str) -> Result<(u8, u8), NikaError> {
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() != 2 {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Invalid exercise ID '{}'. Use format: LL-EE (e.g., 01-03)",
                id
            ),
        });
    }
    let level: u8 = parts[0].parse().map_err(|_| NikaError::ValidationError {
        reason: format!("Invalid level number in '{}'. Expected: LL-EE", id),
    })?;
    let exercise: u8 = parts[1].parse().map_err(|_| NikaError::ValidationError {
        reason: format!("Invalid exercise number in '{}'. Expected: LL-EE", id),
    })?;

    // Validate level exists
    let level_def = levels::by_number(level).ok_or_else(|| NikaError::ValidationError {
        reason: format!("Level {} does not exist. Valid: 1-12", level),
    })?;

    // Validate exercise within range
    if exercise < 1 || exercise > level_def.exercise_count {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Exercise {} out of range for level {} ({}). Valid: 1-{}",
                exercise, level, level_def.name, level_def.exercise_count
            ),
        });
    }

    Ok((level, exercise))
}

/// Find the exercise workflow file path.
///
/// Uses the embedded exercise data to resolve the actual filename
/// (e.g., `01-hello-world.nika.yaml`), falling back to a glob of the
/// level directory when the exercise is not found in the embedded data.
fn exercise_path(root: &Path, level: u8, exercise: u8) -> Result<PathBuf, NikaError> {
    let level_def = levels::by_number(level).ok_or_else(|| NikaError::CourseNotFound {
        path: format!("level {}", level),
    })?;
    let level_dir = root.join(format!("{:02}-{}", level, level_def.slug));

    // Look up the real filename from the embedded exercise data
    if let Some(ex) = exercises::all_exercises()
        .into_iter()
        .find(|e| e.level_slug == level_def.slug && e.exercise_num == exercise)
    {
        return Ok(level_dir.join(ex.filename));
    }

    // Fallback: list *.nika.yaml files sorted and pick the Nth one
    if let Ok(entries) = std::fs::read_dir(&level_dir) {
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|ext| ext == "yaml")
                    && p.to_string_lossy().ends_with(".nika.yaml")
            })
            .collect();
        files.sort();
        if let Some(path) = files.into_iter().nth(exercise as usize - 1) {
            return Ok(path);
        }
    }

    // Last resort: use the level_dir with a synthetic name so errors are clear
    Ok(level_dir.join(format!("{:02}-exercise-{}.nika.yaml", exercise, exercise)))
}

// ─── Status icons ───────────────────────────────────────────────────────────

fn exercise_status_icon(status: &ExerciseStatus) -> &'static str {
    match status {
        ExerciseStatus::NotStarted => "  ",
        ExerciseStatus::Attempted => "  ",
        ExerciseStatus::Passed => "  ",
        ExerciseStatus::Perfect => "  ",
    }
}

// ─── Command implementations ────────────────────────────────────────────────

/// nika course status -- enhanced constellation map
fn cmd_status() -> Result<(), NikaError> {
    let root = find_course_root()?;
    let progress = load_progress(&root)?;

    let completed_levels = progress.completed_levels();
    let total_levels = LEVELS.len();
    let completed_ex = progress.completed_exercises();
    let total_ex = levels::total_exercises();

    println!();
    println!(
        "  {}",
        "Nika Course -- Your Liberation Journey".cyan().bold()
    );
    println!();

    let level_statuses: Vec<(&Level, LevelStatus)> = LEVELS
        .iter()
        .map(|l| {
            let key = l.number.to_string();
            let status = progress
                .levels
                .get(&key)
                .map(|lp| lp.status.clone())
                .unwrap_or(LevelStatus::Locked);
            (l, status)
        })
        .collect();

    for row in 0..2 {
        let start = row * 6;
        let end = (start + 6).min(level_statuses.len());
        let slice = &level_statuses[start..end];

        let mut star_line = String::from("  ");
        for (i, (level, status)) in slice.iter().enumerate() {
            star_line.push_str(&constellation_star(status, level.boss));
            if i < slice.len() - 1 {
                star_line.push_str(&"----".dimmed().to_string());
            }
        }
        println!("{star_line}");

        let mut num_line = String::from("  ");
        for (i, (level, status)) in slice.iter().enumerate() {
            let num = format!("{:02}", level.number);
            let colored_num = match status {
                LevelStatus::Completed => num.green().to_string(),
                LevelStatus::InProgress => num.yellow().to_string(),
                LevelStatus::Unlocked => num.cyan().to_string(),
                LevelStatus::Locked => num.dimmed().to_string(),
            };
            num_line.push_str(&colored_num);
            if i < slice.len() - 1 {
                num_line.push_str("    ");
            }
        }
        println!("{num_line}");
        println!();
    }

    for (level, status) in &level_statuses {
        let key = level.number.to_string();
        let lp = progress.levels.get(&key);

        let ex_done = lp
            .map(|l| {
                l.exercises
                    .values()
                    .filter(|s| **s == ExerciseStatus::Passed || **s == ExerciseStatus::Perfect)
                    .count()
            })
            .unwrap_or(0);

        let perfect_count = lp
            .map(|l| {
                l.exercises
                    .values()
                    .filter(|s| **s == ExerciseStatus::Perfect)
                    .count()
            })
            .unwrap_or(0);

        let star = constellation_star(status, level.boss);
        let name = level.name;

        let pct = if level.exercise_count > 0 {
            (ex_done as f64 / level.exercise_count as f64 * 100.0) as u8
        } else {
            0
        };

        let stars_str = if *status == LevelStatus::Completed {
            let total = level.exercise_count as usize;
            if perfect_count == total {
                format!("  {}", "***".yellow())
            } else if perfect_count >= total / 2 {
                format!("  {}{}", "**".yellow(), "*".dimmed())
            } else {
                format!("  {}{}", "*".yellow(), "**".dimmed())
            }
        } else {
            String::new()
        };

        let line = match status {
            LevelStatus::Locked => {
                format!("  {} {:<16}  {}", star, name.dimmed(), "locked".dimmed())
            }
            LevelStatus::Unlocked => {
                format!("  {} {:<16}  {}", star, name.bold(), "ready".cyan())
            }
            LevelStatus::InProgress => {
                format!(
                    "  {} {:<16}  {:>3}%",
                    star,
                    name.yellow().bold(),
                    pct.to_string().yellow()
                )
            }
            LevelStatus::Completed => {
                format!(
                    "  {} {:<16}  {:>3}%{}",
                    star,
                    name.green(),
                    "100".green(),
                    stars_str
                )
            }
        };
        println!("{line}");

        if level.boss {
            println!("                       {}", "BOSS".red().bold());
        }
    }

    println!();
    println!(
        "  Progress: {}/{} levels | {}/{} exercises",
        completed_levels.to_string().green(),
        total_levels,
        completed_ex.to_string().green(),
        total_ex,
    );

    if progress.metadata.total_hints_used > 0 {
        println!(
            "  Hints used: {}",
            progress.metadata.total_hints_used.to_string().yellow()
        );
    }

    println!();
    println!("  {}", "Run `nika course next` to continue.".dimmed());
    println!();

    Ok(())
}

/// Constellation star icon for a level based on its status
fn constellation_star(status: &LevelStatus, boss: bool) -> String {
    if boss {
        match status {
            LevelStatus::Completed => "*".yellow().bold().to_string(),
            _ => "*".dimmed().to_string(),
        }
    } else {
        match status {
            LevelStatus::Completed => "*".green().bold().to_string(),
            LevelStatus::InProgress => "+".yellow().bold().to_string(),
            LevelStatus::Unlocked => "+".cyan().to_string(),
            LevelStatus::Locked => "o".dimmed().to_string(),
        }
    }
}

/// nika course next — find next exercise
fn cmd_next() -> Result<(), NikaError> {
    let root = find_course_root()?;
    let progress = load_progress(&root)?;

    // Find first non-completed exercise in an unlocked/in-progress level
    for level in LEVELS {
        let key = level.number.to_string();
        let lp = match progress.levels.get(&key) {
            Some(lp) => lp,
            None => continue,
        };

        match lp.status {
            LevelStatus::Locked | LevelStatus::Completed => continue,
            LevelStatus::Unlocked | LevelStatus::InProgress => {}
        }

        // Find first non-passed exercise
        for ex in 1..=level.exercise_count {
            let ex_key = ex.to_string();
            let status = lp
                .exercises
                .get(&ex_key)
                .unwrap_or(&ExerciseStatus::NotStarted);
            if *status != ExerciseStatus::Passed && *status != ExerciseStatus::Perfect {
                let path = exercise_path(&root, level.number, ex)?;
                println!();
                println!(
                    "  {} Level {:02}: {} -- Exercise {}",
                    ">>".cyan().bold(),
                    level.number,
                    level.name.bold(),
                    ex
                );
                println!("  {}", level.description.dimmed());
                println!();

                if path.exists() {
                    println!("  File: {}", path.display().to_string().cyan());
                    println!();
                    println!("  {}", "Edit the file, then run:".dimmed());
                    println!("    nika course check {:02}", level.number);
                } else {
                    println!(
                        "  {} Exercise file not found: {}",
                        "!".yellow().bold(),
                        path.display()
                    );
                    println!(
                        "  {}",
                        "The course content may not be generated yet.".dimmed()
                    );
                    println!(
                        "  {}",
                        "Tip: run `nika init --course` to generate course files.".dimmed()
                    );
                }
                println!();
                return Ok(());
            }
        }
    }

    // All done!
    println!();
    println!(
        "  {} {}",
        "***".green().bold(),
        "You've completed all available exercises!".green().bold()
    );
    println!(
        "  {}",
        "Run `nika course status` to see your constellation.".dimmed()
    );
    println!();

    Ok(())
}

/// nika course check [level] — validate exercises
fn cmd_check(level_arg: Option<String>) -> Result<(), NikaError> {
    let root = find_course_root()?;
    let mut progress = load_progress(&root)?;

    let level = match level_arg {
        Some(ref arg) => resolve_level(arg)?,
        None => {
            // Auto-detect: check the current in-progress level
            let current = LEVELS.iter().find(|l| {
                let key = l.number.to_string();
                progress
                    .levels
                    .get(&key)
                    .map(|lp| lp.status == LevelStatus::InProgress)
                    .unwrap_or(false)
            });
            match current {
                Some(l) => l,
                None => {
                    return Err(NikaError::ValidationError {
                        reason: "No level in progress. Specify a level: nika course check <level>"
                            .to_string(),
                    });
                }
            }
        }
    };

    println!();
    println!(
        "  {} Checking Level {:02}: {}",
        ">>>".cyan().bold(),
        level.number,
        level.name.bold()
    );
    println!();

    let mut report = LevelReport {
        level: level.number,
        exercises: Vec::new(),
    };

    for ex in 1..=level.exercise_count {
        let path = exercise_path(&root, level.number, ex)?;
        let ex_id = format!("{:02}-{:02}", level.number, ex);

        if !path.exists() {
            println!(
                "  {} {} -- {}",
                "SKIP".yellow(),
                ex_id,
                "file not found".dimmed()
            );
            continue;
        }

        let yaml = std::fs::read_to_string(&path).map_err(|e| NikaError::ValidationError {
            reason: format!("Failed to read {}: {}", path.display(), e),
        })?;

        // Run standard checks based on level + exercise
        let mut checks = build_checks_for_level(level.number, ex, &yaml);

        // QW #4: Run real AST validation (Phase 1 + Phase 2)
        match nika_engine::ast::parse_analyzed(&yaml) {
            Ok(_) => checks.push(nika_init::course::checks::CheckResult {
                name: "nika check (AST)",
                verdict: CheckVerdict::Pass,
            }),
            Err(e) => checks.push(nika_init::course::checks::CheckResult {
                name: "nika check (AST)",
                verdict: CheckVerdict::Fail(e.to_string()),
            }),
        }

        let ex_report = ExerciseReport {
            exercise_id: ex_id.clone(),
            checks,
        };

        // Display results
        if ex_report.passed() {
            println!("  {} {}", "PASS".green().bold(), ex_id);
            // Update progress
            progress.mark_exercise_passed(level.number, ex);
        } else {
            println!("  {} {}", "FAIL".red().bold(), ex_id);
            for check in &ex_report.checks {
                match &check.verdict {
                    CheckVerdict::Pass => {
                        println!("    {} {}", "+".green(), check.name);
                    }
                    CheckVerdict::Fail(reason) => {
                        println!("    {} {} -- {}", "x".red(), check.name, reason.dimmed());
                    }
                    CheckVerdict::Bonus(msg) => {
                        println!("    {} {} -- {}", "*".yellow(), check.name, msg.dimmed());
                    }
                }
            }
        }

        // Count bonuses
        let bonus = ex_report.bonus_count();
        if bonus > 0 {
            println!("    {} {} bonus(es)!", "*".yellow(), bonus);
        }

        report.exercises.push(ex_report);
    }

    // Save updated progress
    save_progress(&root, &mut progress)?;

    // Summary
    println!();
    let passed = report.pass_count();
    let total = report.exercises.len();
    if report.all_passed() && total > 0 {
        println!(
            "  {} Level {:02} complete! ({}/{})",
            "***".green().bold(),
            level.number,
            passed,
            total
        );
        if let Some(next) = levels::by_number(level.number + 1) {
            println!(
                "  {} Next: Level {:02} -- {}",
                ">>".cyan(),
                next.number,
                next.name
            );
        }
    } else {
        println!(
            "  {}/{} exercises passed",
            passed.to_string().yellow(),
            total
        );
        println!("  {}", "Tip: use `nika course hint` for help.".dimmed());
    }

    // QW #6: Star scoring
    if total > 0 {
        let star_correctness = report.all_passed();
        let total_bonuses: usize = report.exercises.iter().map(|e| e.bonus_count()).sum();
        let star_elegance = total_bonuses > 0;
        let key = level.number.to_string();
        let hints_used = progress
            .levels
            .get(&key)
            .map(|lp| lp.hints_used)
            .unwrap_or(0);
        let star_no_hints = hints_used == 0;

        let star_count = star_correctness as u8 + star_elegance as u8 + star_no_hints as u8;

        let stars: String = (0..3)
            .map(|i| {
                if i < star_count {
                    '\u{2605}'
                } else {
                    '\u{2606}'
                }
            })
            .collect();

        println!();
        println!(
            "  Level {:02} {}: {} ({}/3 stars)",
            level.number,
            level.name,
            stars.yellow(),
            star_count
        );
        println!(
            "  - {} Correctness: {}/{}{}",
            if star_correctness {
                "\u{2605}"
            } else {
                "\u{2606}"
            },
            passed,
            total,
            if star_correctness { " pass" } else { "" }
        );
        println!(
            "  - {} Elegance: {}",
            if star_elegance {
                "\u{2605}"
            } else {
                "\u{2606}"
            },
            if star_elegance {
                format!("{} bonus(es) unlocked", total_bonuses)
            } else {
                "no bonus checks passed yet".to_string()
            }
        );
        println!(
            "  - {} No hints: {}",
            if star_no_hints {
                "\u{2605}"
            } else {
                "\u{2606}"
            },
            if star_no_hints {
                "solved without hints".to_string()
            } else {
                format!("used {} hint(s)", hints_used)
            }
        );
    }
    println!();

    Ok(())
}

/// Build check assertions appropriate for a given level and exercise.
///
/// Levels 1-5 have per-exercise verb checks because exercises within a level
/// teach different verbs. Levels 6-12 use broad level-wide checks.
fn build_checks_for_level(
    level_num: u8,
    exercise_num: u8,
    yaml: &str,
) -> Vec<nika_init::course::checks::CheckResult> {
    let mut checks = vec![
        check_has_schema(yaml),
        check_no_todos(yaml),
        check_min_tasks(yaml, 1),
    ];

    match level_num {
        1 => {
            // Level 1 (Jailbreak): exercises teach different verbs
            match exercise_num {
                1 => checks.push(check_has_verb(yaml, "infer")),
                2 => checks.push(check_has_verb(yaml, "exec")),
                3 => checks.push(check_has_verb(yaml, "fetch")),
                4 => checks.push(check_has_verb(yaml, "infer")),
                5 => {
                    checks.push(check_has_depends_on(yaml));
                    checks.push(check_min_tasks(yaml, 2));
                }
                _ => {}
            }
        }
        2 => {
            // Level 2 (Hot Wire): all exercises use with: bindings
            checks.push(check_has_with_bindings(yaml));
        }
        3 => {
            checks.push(check_has_depends_on(yaml));
            checks.push(check_min_tasks(yaml, 2));
        }
        4 => {
            // Level 4 (Root Access): Ex1 uses infer:, Ex2-3 are pipelines
            match exercise_num {
                1 => checks.push(check_has_verb(yaml, "infer")),
                _ => checks.push(check_min_tasks(yaml, 2)),
            }
        }
        5 => {
            // Level 5 (Shapeshifter): Ex1 uses infer: (structured), Ex2-3 are artifacts/retry
            match exercise_num {
                1 => checks.push(check_has_verb(yaml, "infer")),
                _ => checks.push(check_min_tasks(yaml, 2)),
            }
        }
        6 => {
            checks.push(check_has_verb(yaml, "infer"));
            // structured output checks would go here
        }
        7 => {
            checks.push(check_has_verb(yaml, "invoke"));
        }
        8 => {
            checks.push(check_has_verb(yaml, "agent"));
        }
        9 => {
            checks.push(check_has_verb(yaml, "fetch"));
        }
        10 => {
            checks.push(check_has_verb(yaml, "invoke"));
        }
        11 => {
            checks.push(check_has_verb(yaml, "invoke"));
        }
        12 => {
            // Boss level: everything
            checks.push(check_has_depends_on(yaml));
            checks.push(check_has_with_bindings(yaml));
            checks.push(check_min_tasks(yaml, 3));
        }
        _ => {}
    }

    checks
}

/// nika course hint [exercise] — progressive hints
fn cmd_hint(exercise_arg: Option<String>) -> Result<(), NikaError> {
    let root = find_course_root()?;
    let mut progress = load_progress(&root)?;

    let (level_num, ex_num) = match exercise_arg {
        Some(ref id) => parse_exercise_id(id)?,
        None => {
            // QW #7: Smart detection -- find first non-passed exercise
            match find_current_exercise(&progress) {
                Ok(found) => found,
                Err(_) => {
                    println!();
                    println!(
                        "  {} {}",
                        "***".green().bold(),
                        "All exercises complete! Move to the next level."
                            .green()
                            .bold()
                    );
                    println!(
                        "  {}",
                        "Run `nika course status` to see your constellation.".dimmed()
                    );
                    println!();
                    return Ok(());
                }
            }
        }
    };

    let level = levels::by_number(level_num).ok_or_else(|| NikaError::CourseNotFound {
        path: format!("level {}", level_num),
    })?;
    let hints = get_hints(level_num, ex_num).ok_or_else(|| NikaError::ValidationError {
        reason: format!(
            "No hints available for exercise {:02}-{:02} yet.",
            level_num, ex_num
        ),
    })?;

    // Determine how many hints have been revealed
    let key = level_num.to_string();
    let lp = progress.levels.get(&key);
    let hints_revealed = lp.map(|l| l.hints_used).unwrap_or(0);

    let next_level = next_hint_level(hints_revealed);

    println!();
    println!(
        "  {} Level {:02}: {} -- Exercise {}",
        "?".cyan().bold(),
        level_num,
        level.name.bold(),
        ex_num
    );
    println!();

    // Show all previously revealed hints
    for (hint_level, text) in hints.hints {
        let shown = match hint_level {
            nika_init::course::hints::HintLevel::Conceptual => hints_revealed >= 1,
            nika_init::course::hints::HintLevel::Specific => hints_revealed >= 2,
            nika_init::course::hints::HintLevel::Solution => hints_revealed >= 3,
        };

        if shown {
            println!("  {} [{}]", "*".yellow(), hint_level.label().dimmed());
            for line in text.lines() {
                println!("    {}", line);
            }
            println!();
        }
    }

    // Reveal next hint
    match next_level {
        Some(level) => {
            // Find the hint at this level
            if let Some((_, text)) = hints.hints.iter().find(|(l, _)| *l == level) {
                println!("  {} [{}] (new!)", "*".green().bold(), level.label().cyan());
                for line in text.lines() {
                    println!("    {}", line);
                }
                println!();
            }
            // Record the hint
            progress.record_hint(level_num);
            save_progress(&root, &mut progress)?;

            let remaining = 3u32.saturating_sub(hints_revealed + 1);
            if remaining > 0 {
                println!("  {} {} hint(s) remaining", "i".dimmed(), remaining);
            } else {
                println!("  {}", "All hints revealed.".dimmed());
            }
        }
        None => {
            println!(
                "  {}",
                "All hints already revealed for this exercise.".dimmed()
            );
        }
    }
    println!();

    Ok(())
}

/// nika course reset <level> — reset a level
fn cmd_reset(level_arg: &str) -> Result<(), NikaError> {
    let root = find_course_root()?;
    let mut progress = load_progress(&root)?;
    let level = resolve_level(level_arg)?;

    progress.reset_level(level.number);
    save_progress(&root, &mut progress)?;

    println!();
    println!(
        "  {} Level {:02}: {} reset to start",
        "<<".yellow().bold(),
        level.number,
        level.name.bold()
    );
    println!("  {}", "Exercises and hints cleared. Good luck!".dimmed());
    println!();

    Ok(())
}

/// nika course run <exercise> — run an exercise workflow
fn cmd_run(exercise_arg: &str) -> Result<(), NikaError> {
    let root = find_course_root()?;
    let (level_num, ex_num) = parse_exercise_id(exercise_arg)?;
    let path = exercise_path(&root, level_num, ex_num)?;

    if !path.exists() {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Exercise file not found: {}. Generate course files with `nika init --course`.",
                path.display()
            ),
        });
    }

    let level = levels::by_number(level_num).ok_or_else(|| NikaError::CourseNotFound {
        path: format!("level {}", level_num),
    })?;
    println!();
    println!(
        "  {} Running {:02}-{:02} ({} -- exercise {})",
        ">>".cyan().bold(),
        level_num,
        ex_num,
        level.name,
        ex_num
    );
    println!("  {}", format!("nika run {}", path.display()).dimmed());
    println!();

    // QW #2: Shell out using current executable for reliable dispatch
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("nika"));
    let status = std::process::Command::new(&exe)
        .arg("run")
        .arg(&path)
        .status()
        .map_err(|e| NikaError::ValidationError {
            reason: format!("Failed to execute nika run: {}", e),
        })?;

    if !status.success() {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Exercise {:02}-{:02} failed with exit code {}",
                level_num,
                ex_num,
                status.code().unwrap_or(-1)
            ),
        });
    }

    Ok(())
}

/// nika course info [level] — show course or level details
fn cmd_info(level_arg: Option<String>) -> Result<(), NikaError> {
    match level_arg {
        Some(ref arg) => {
            let level = resolve_level(arg)?;
            let root = find_course_root()?;
            let progress = load_progress(&root)?;
            let key = level.number.to_string();
            let lp = progress.levels.get(&key);

            println!();
            println!(
                "  {} Level {:02}: {}",
                if level.boss { "***" } else { ">>>" }.cyan().bold(),
                level.number,
                level.name.bold()
            );
            println!("  {}", level.description);
            println!();

            let status = lp.map(|l| &l.status).unwrap_or(&LevelStatus::Locked);
            let status_str = match status {
                LevelStatus::Locked => "Locked".dimmed().to_string(),
                LevelStatus::Unlocked => "Unlocked".cyan().to_string(),
                LevelStatus::InProgress => "In Progress".yellow().to_string(),
                LevelStatus::Completed => "Completed".green().to_string(),
            };
            println!("  Status: {status_str}");

            if level.boss {
                println!(
                    "  {}",
                    "BOSS LEVEL -- requires mastery of all prior levels".red()
                );
            }
            println!();

            // Exercise list
            println!("  Exercises ({}):", level.exercise_count);
            for ex in 1..=level.exercise_count {
                let ex_key = ex.to_string();
                let ex_status = lp
                    .and_then(|l| l.exercises.get(&ex_key))
                    .unwrap_or(&ExerciseStatus::NotStarted);
                let icon = exercise_status_icon(ex_status);
                let path = exercise_path(&root, level.number, ex)?;
                let exists = if path.exists() { "" } else { " (missing)" };
                println!(
                    "    {} {:02}-{:02}{}",
                    icon,
                    level.number,
                    ex,
                    exists.dimmed()
                );
            }

            if let Some(lp) = lp {
                if lp.hints_used > 0 {
                    println!();
                    println!("  Hints used: {}", lp.hints_used);
                }
            }
            println!();
        }
        None => {
            // Overview of all levels
            println!();
            println!("{}", "  NIKA COURSE -- 12 Levels to Liberation".bold());
            println!();
            println!(
                "  {} exercises across {} levels",
                levels::total_exercises().to_string().cyan(),
                LEVELS.len().to_string().cyan()
            );
            println!();

            for level in LEVELS {
                let boss = if level.boss { " [BOSS]" } else { "" };
                println!(
                    "  {:02}. {} ({} exercises){}",
                    level.number,
                    level.name.bold(),
                    level.exercise_count,
                    boss.red()
                );
                println!("      {}", level.description.dimmed());
            }
            println!();
            println!(
                "  {}",
                "Use `nika course info <level>` for details.".dimmed()
            );
            println!();
        }
    }

    Ok(())
}

/// nika course watch — rustlings-style auto-check on file save
fn cmd_watch() -> Result<(), NikaError> {
    let root = find_course_root()?;
    let progress = load_progress(&root)?;

    let (level_num, _) = find_current_exercise(&progress)?;
    let level = levels::by_number(level_num).ok_or_else(|| NikaError::ValidationError {
        reason: format!("Level {} not found", level_num),
    })?;

    let level_dir = root.join(format!("{:02}-{}", level.number, level.slug));

    if !level_dir.is_dir() {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Level directory not found: {}. Run `nika init --course` first.",
                level_dir.display()
            ),
        });
    }

    println!();
    println!(
        "  {} Watching Level {:02}: {} for changes...",
        ">>>".cyan().bold(),
        level.number,
        level.name.bold()
    );
    println!("  {}", level_dir.display().to_string().dimmed());
    println!("  {}", "Press Ctrl+C to stop.".dimmed());
    println!();

    let mut mtimes: HashMap<PathBuf, SystemTime> = HashMap::new();
    seed_mtimes(&level_dir, &mut mtimes);

    loop {
        if let Some(changed_path) = scan_for_changes(&level_dir, &mut mtimes) {
            print!("\x1b[2J\x1b[H");
            println!();
            println!(
                "  {} File changed: {}",
                ">>>".cyan().bold(),
                changed_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .cyan()
            );
            println!();

            // Re-run level checks (reuses cmd_check infrastructure + updates progress)
            let _ = cmd_check(Some(level.number.to_string()));

            println!("  {}", "Watching... Ctrl+C to stop".dimmed());
        }

        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

fn seed_mtimes(level_dir: &Path, mtimes: &mut HashMap<PathBuf, SystemTime>) {
    if let Ok(entries) = std::fs::read_dir(level_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if is_nika_yaml(&path) {
                if let Ok(meta) = std::fs::metadata(&path) {
                    if let Ok(mtime) = meta.modified() {
                        mtimes.insert(path, mtime);
                    }
                }
            }
        }
    }
}

fn scan_for_changes(
    level_dir: &Path,
    mtimes: &mut HashMap<PathBuf, SystemTime>,
) -> Option<PathBuf> {
    let entries = std::fs::read_dir(level_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_nika_yaml(&path) {
            continue;
        }
        let mtime = match std::fs::metadata(&path).and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let changed = mtimes.get(&path).is_none_or(|prev| *prev != mtime);
        if changed {
            mtimes.insert(path.clone(), mtime);
            return Some(path);
        }
    }
    None
}

fn is_nika_yaml(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "yaml") && path.to_string_lossy().ends_with(".nika.yaml")
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Find the current exercise (first non-passed in the active level)
fn find_current_exercise(progress: &CourseProgress) -> Result<(u8, u8), NikaError> {
    for level in LEVELS {
        let key = level.number.to_string();
        let lp = match progress.levels.get(&key) {
            Some(lp) => lp,
            None => continue,
        };

        match lp.status {
            LevelStatus::Locked | LevelStatus::Completed => continue,
            LevelStatus::Unlocked | LevelStatus::InProgress => {}
        }

        for ex in 1..=level.exercise_count {
            let ex_key = ex.to_string();
            let status = lp
                .exercises
                .get(&ex_key)
                .unwrap_or(&ExerciseStatus::NotStarted);
            if *status != ExerciseStatus::Passed && *status != ExerciseStatus::Perfect {
                return Ok((level.number, ex));
            }
        }
    }

    Err(NikaError::ValidationError {
        reason: "All exercises completed! Nothing to hint about.".to_string(),
    })
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_level_by_number() {
        let level = resolve_level("1").unwrap();
        assert_eq!(level.number, 1);
        assert_eq!(level.slug, "jailbreak");
    }

    #[test]
    fn test_resolve_level_by_padded_number() {
        let level = resolve_level("01").unwrap();
        assert_eq!(level.number, 1);

        let level = resolve_level("12").unwrap();
        assert_eq!(level.number, 12);
    }

    #[test]
    fn test_resolve_level_by_slug() {
        let level = resolve_level("jailbreak").unwrap();
        assert_eq!(level.number, 1);

        let level = resolve_level("hot-wire").unwrap();
        assert_eq!(level.number, 2);

        let level = resolve_level("supernovae").unwrap();
        assert_eq!(level.number, 12);
    }

    #[test]
    fn test_resolve_level_by_name() {
        let level = resolve_level("Jailbreak").unwrap();
        assert_eq!(level.number, 1);

        let level = resolve_level("Hot Wire").unwrap();
        assert_eq!(level.number, 2);

        let level = resolve_level("SuperNovae").unwrap();
        assert_eq!(level.number, 12);
    }

    #[test]
    fn test_resolve_level_case_insensitive() {
        let level = resolve_level("JAILBREAK").unwrap();
        assert_eq!(level.number, 1);

        let level = resolve_level("Hot wire").unwrap();
        assert_eq!(level.number, 2);
    }

    #[test]
    fn test_resolve_level_invalid() {
        assert!(resolve_level("0").is_err());
        assert!(resolve_level("13").is_err());
        assert!(resolve_level("nonexistent").is_err());
        assert!(resolve_level("").is_err());
    }

    #[test]
    fn test_parse_exercise_id_valid() {
        let (l, e) = parse_exercise_id("01-03").unwrap();
        assert_eq!(l, 1);
        assert_eq!(e, 3);

        let (l, e) = parse_exercise_id("12-05").unwrap();
        assert_eq!(l, 12);
        assert_eq!(e, 5);
    }

    #[test]
    fn test_parse_exercise_id_invalid_format() {
        assert!(parse_exercise_id("1").is_err());
        assert!(parse_exercise_id("1-2-3").is_err());
        assert!(parse_exercise_id("").is_err());
        assert!(parse_exercise_id("ab-cd").is_err());
    }

    #[test]
    fn test_parse_exercise_id_out_of_range() {
        // Level 1 has 5 exercises
        assert!(parse_exercise_id("01-06").is_err());
        assert!(parse_exercise_id("01-00").is_err());
        // Level 13 doesn't exist
        assert!(parse_exercise_id("13-01").is_err());
    }

    #[test]
    fn test_exercise_path_format() {
        let root = PathBuf::from("/project");
        let path = exercise_path(&root, 1, 3).unwrap();
        // Level 1, exercise 3 = "03-http-requests.nika.yaml"
        assert_eq!(
            path,
            PathBuf::from("/project/01-jailbreak/03-http-requests.nika.yaml")
        );
    }

    #[test]
    fn test_exercise_path_level_12() {
        let root = PathBuf::from("/project");
        let path = exercise_path(&root, 12, 5).unwrap();
        // Level 12, exercise 5 = "05-full-stack.nika.yaml"
        assert_eq!(
            path,
            PathBuf::from("/project/12-supernovae/05-full-stack.nika.yaml")
        );
    }

    #[test]
    fn test_exercise_path_no_course_prefix() {
        let root = PathBuf::from("/project");
        let path = exercise_path(&root, 1, 1).unwrap();
        // Must NOT contain "course/" prefix
        assert!(
            !path.to_string_lossy().contains("/course/"),
            "Path should not have course/ prefix: {}",
            path.display()
        );
        assert_eq!(
            path,
            PathBuf::from("/project/01-jailbreak/01-hello-world.nika.yaml")
        );
    }

    #[test]
    fn test_build_checks_level_1_ex2_exec() {
        let yaml = "schema: \"nika/workflow@0.12\"\nworkflow: test\ntasks:\n  - id: hello\n    exec:\n      run: echo hi\n";
        let checks = build_checks_for_level(1, 2, yaml);
        assert!(checks.iter().all(|c| c.verdict.is_pass()));
    }

    #[test]
    fn test_build_checks_level_1_ex1_infer() {
        let yaml = "schema: \"nika/workflow@0.12\"\nworkflow: test\ntasks:\n  - id: hello\n    infer: \"say hi\"\n";
        let checks = build_checks_for_level(1, 1, yaml);
        assert!(checks.iter().all(|c| c.verdict.is_pass()));
    }

    #[test]
    fn test_build_checks_level_1_ex2_missing_verb() {
        let yaml = "schema: \"nika/workflow@0.12\"\nworkflow: test\ntasks:\n  - id: hello\n    fetch:\n      url: \"https://example.com\"\n";
        let checks = build_checks_for_level(1, 2, yaml);
        // Should fail because exec: is missing for exercise 2
        let has_fail = checks
            .iter()
            .any(|c| matches!(c.verdict, CheckVerdict::Fail(_)));
        assert!(has_fail);
    }

    #[test]
    fn test_build_checks_level_2_with_bindings() {
        let yaml = "schema: \"nika/workflow@0.12\"\nworkflow: test\ntasks:\n  - id: step1\n    exec: \"date\"\n  - id: step2\n    with:\n      data: $step1\n    exec: \"echo with.data\"\n";
        let checks = build_checks_for_level(2, 1, yaml);
        assert!(
            checks.iter().all(|c| c.verdict.is_pass()),
            "Level 2 should check for with: bindings, not fetch:"
        );
    }

    #[test]
    fn test_build_checks_level_12_boss() {
        let yaml = "schema: \"nika/workflow@0.12\"\nworkflow: boss\ntasks:\n  - id: step1\n    exec:\n      run: echo 1\n  - id: step2\n    depends_on: [step1]\n    with:\n      data: $step1\n    infer:\n      prompt: \"with.data\"\n  - id: step3\n    depends_on: [step2]\n    with:\n      result: $step2\n    exec:\n      run: echo done\n";
        let checks = build_checks_for_level(12, 1, yaml);
        assert!(
            checks.iter().all(|c| c.verdict.is_pass()),
            "Boss level checks should all pass for a well-formed workflow"
        );
    }

    #[test]
    fn test_find_current_exercise_fresh_course() {
        let progress = CourseProgress::new_course();
        let (level, ex) = find_current_exercise(&progress).unwrap();
        assert_eq!(level, 1);
        assert_eq!(ex, 1);
    }

    #[test]
    fn test_find_current_exercise_mid_level() {
        let mut progress = CourseProgress::new_course();
        progress.mark_exercise_passed(1, 1);
        progress.mark_exercise_passed(1, 2);
        let (level, ex) = find_current_exercise(&progress).unwrap();
        assert_eq!(level, 1);
        assert_eq!(ex, 3);
    }

    #[test]
    fn test_find_current_exercise_next_level() {
        let mut progress = CourseProgress::new_course();
        // Complete all of level 1
        for ex in 1..=5 {
            progress.mark_exercise_passed(1, ex);
        }
        let (level, ex) = find_current_exercise(&progress).unwrap();
        assert_eq!(level, 2);
        assert_eq!(ex, 1);
    }

    #[test]
    fn test_constellation_star_non_empty() {
        assert!(!constellation_star(&LevelStatus::Locked, false).is_empty());
        assert!(!constellation_star(&LevelStatus::Unlocked, false).is_empty());
        assert!(!constellation_star(&LevelStatus::InProgress, false).is_empty());
        assert!(!constellation_star(&LevelStatus::Completed, false).is_empty());
        assert!(!constellation_star(&LevelStatus::Completed, true).is_empty());
    }

    #[test]
    fn test_exercise_status_icons_are_non_empty() {
        assert!(!exercise_status_icon(&ExerciseStatus::NotStarted).is_empty());
        assert!(!exercise_status_icon(&ExerciseStatus::Attempted).is_empty());
        assert!(!exercise_status_icon(&ExerciseStatus::Passed).is_empty());
        assert!(!exercise_status_icon(&ExerciseStatus::Perfect).is_empty());
    }
}
