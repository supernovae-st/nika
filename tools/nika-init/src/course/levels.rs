//! Course levels — 12 progressive levels with Liberation theme names
//!
//! Each level has a fixed number of exercises and builds on the previous.
//! Boss levels (`boss: true`) gate progression and require all prior mastery.

/// A course level definition
#[derive(Debug, Clone)]
pub struct Level {
    /// Level number (01-12)
    pub number: u8,
    /// URL-friendly identifier
    pub slug: &'static str,
    /// Display name with Liberation theme
    pub name: &'static str,
    /// Short description of what the level teaches
    pub description: &'static str,
    /// Number of exercises in this level
    pub exercise_count: u8,
    /// Boss level gates progression
    pub boss: bool,
}

/// All 12 course levels, ordered by number
pub static LEVELS: &[Level] = &[
    Level {
        number: 1,
        slug: "jailbreak",
        name: "Jailbreak",
        description: "Break free. First workflows — infer:, exec:, fetch:, and API key setup.",
        exercise_count: 5,
        boss: false,
    },
    Level {
        number: 2,
        slug: "hot-wire",
        name: "Hot Wire",
        description: "Hot-wire data flow. Master with: bindings and pipe transforms.",
        exercise_count: 4,
        boss: false,
    },
    Level {
        number: 3,
        slug: "fork-bomb",
        name: "Fork Bomb",
        description: "Multiply your power. DAG patterns, depends_on, and parallel execution.",
        exercise_count: 4,
        boss: false,
    },
    Level {
        number: 4,
        slug: "root-access",
        name: "Root Access",
        description: "Unlock the LLM. infer: prompts, multi-step pipelines, and inputs.",
        exercise_count: 3,
        boss: false,
    },
    Level {
        number: 5,
        slug: "shapeshifter",
        name: "Shapeshifter",
        description: "Structured output, artifacts, and schema validation with retry.",
        exercise_count: 3,
        boss: false,
    },
    Level {
        number: 6,
        slug: "pay-per-dream",
        name: "Pay-Per-Dream",
        description: "Multi-provider routing, native local inference, and system prompts.",
        exercise_count: 3,
        boss: false,
    },
    Level {
        number: 7,
        slug: "swiss-knife",
        name: "Swiss Knife",
        description: "Builtin tools via invoke: — nika:log, nika:emit, nika:assert.",
        exercise_count: 3,
        boss: false,
    },
    Level {
        number: 8,
        slug: "gone-rogue",
        name: "Gone Rogue",
        description: "Autonomous agents with agent:, tools, and stop conditions.",
        exercise_count: 3,
        boss: false,
    },
    Level {
        number: 9,
        slug: "data-heist",
        name: "Data Heist",
        description: "Advanced fetch: extraction — markdown, article, metadata, links.",
        exercise_count: 4,
        boss: false,
    },
    Level {
        number: 10,
        slug: "open-protocol",
        name: "Open Protocol",
        description: "MCP integration — invoke: external tools and NovaNet.",
        exercise_count: 3,
        boss: false,
    },
    Level {
        number: 11,
        slug: "pixel-pirate",
        name: "Pixel Pirate",
        description: "Media pipeline — import, thumbnail, vision, CAS workflows.",
        exercise_count: 4,
        boss: false,
    },
    Level {
        number: 12,
        slug: "supernovae",
        name: "SuperNovae",
        description: "Final boss. Orchestrate everything — full production workflows.",
        exercise_count: 5,
        boss: true,
    },
];

/// Look up a level by its number (1-12)
pub fn by_number(n: u8) -> Option<&'static Level> {
    LEVELS.iter().find(|l| l.number == n)
}

/// Look up a level by its slug (e.g., "jailbreak", "hot-wire")
pub fn by_slug(slug: &str) -> Option<&'static Level> {
    LEVELS.iter().find(|l| l.slug == slug)
}

/// Total number of exercises across all levels
pub fn total_exercises() -> usize {
    LEVELS.iter().map(|l| l.exercise_count as usize).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_count() {
        assert_eq!(LEVELS.len(), 12, "Must have exactly 12 levels");
    }

    #[test]
    fn test_level_numbers_sequential() {
        for (i, level) in LEVELS.iter().enumerate() {
            assert_eq!(
                level.number,
                (i + 1) as u8,
                "Level at index {} should be number {}",
                i,
                i + 1
            );
        }
    }

    #[test]
    fn test_exercise_counts() {
        let expected: Vec<(u8, u8)> = vec![
            (1, 5),
            (2, 4),
            (3, 4),
            (4, 3),
            (5, 3),
            (6, 3),
            (7, 3),
            (8, 3),
            (9, 4),
            (10, 3),
            (11, 4),
            (12, 5),
        ];
        for (num, count) in expected {
            let level = by_number(num).unwrap();
            assert_eq!(
                level.exercise_count, count,
                "Level {} ({}) should have {} exercises",
                num, level.name, count
            );
        }
    }

    #[test]
    fn test_total_exercises() {
        // 5+4+4+3+3+3+3+3+4+3+4+5 = 44
        assert_eq!(total_exercises(), 44);
    }

    #[test]
    fn test_boss_only_level_12() {
        for level in LEVELS {
            if level.number == 12 {
                assert!(level.boss, "Level 12 must be a boss level");
            } else {
                assert!(
                    !level.boss,
                    "Level {} should not be a boss level",
                    level.number
                );
            }
        }
    }

    #[test]
    fn test_slugs_unique() {
        let mut slugs: Vec<&str> = LEVELS.iter().map(|l| l.slug).collect();
        let len = slugs.len();
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), len, "All slugs must be unique");
    }

    #[test]
    fn test_by_number() {
        assert!(by_number(0).is_none());
        assert!(by_number(13).is_none());
        assert_eq!(by_number(1).unwrap().slug, "jailbreak");
        assert_eq!(by_number(12).unwrap().slug, "supernovae");
    }

    #[test]
    fn test_by_slug() {
        assert!(by_slug("nonexistent").is_none());
        assert_eq!(by_slug("jailbreak").unwrap().number, 1);
        assert_eq!(by_slug("supernovae").unwrap().number, 12);
        assert_eq!(by_slug("hot-wire").unwrap().number, 2);
    }

    #[test]
    fn test_names_match_theme() {
        let names: Vec<&str> = LEVELS.iter().map(|l| l.name).collect();
        assert_eq!(
            names,
            vec![
                "Jailbreak",
                "Hot Wire",
                "Fork Bomb",
                "Root Access",
                "Shapeshifter",
                "Pay-Per-Dream",
                "Swiss Knife",
                "Gone Rogue",
                "Data Heist",
                "Open Protocol",
                "Pixel Pirate",
                "SuperNovae",
            ]
        );
    }
}
