// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Suggestion engine using Levenshtein distance.
//!
//! Provides "did you mean?" suggestions for typos and near-misses.

use strsim::jaro_winkler;

/// Find the most similar string from a list of candidates.
///
/// Uses Jaro-Winkler similarity which works well for typos:
/// - Gives extra weight to matching prefixes
/// - Better for short strings like identifiers
///
/// # Arguments
///
/// * `input` - The input string (possibly with typo)
/// * `candidates` - List of valid strings to match against
/// * `threshold` - Minimum similarity score (0.0 - 1.0) to return a suggestion
///
/// # Returns
///
/// The best matching candidate if similarity >= threshold, None otherwise.
///
/// # Example
///
/// ```ignore
/// let suggestion = find_similar("taks1", &["task1", "task2", "other"], 0.6);
/// assert_eq!(suggestion, Some("task1".to_string()));
/// ```
pub fn find_similar(input: &str, candidates: &[&str], threshold: f64) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }

    let mut best_match: Option<(&str, f64)> = None;

    for candidate in candidates {
        let score = jaro_winkler(input, candidate);
        if score >= threshold {
            if let Some((_, best_score)) = best_match {
                if score > best_score {
                    best_match = Some((candidate, score));
                }
            } else {
                best_match = Some((candidate, score));
            }
        }
    }

    best_match.map(|(s, _)| s.to_string())
}

/// Find all similar strings above a threshold, sorted by similarity.
///
/// # Arguments
///
/// * `input` - The input string
/// * `candidates` - List of valid strings
/// * `threshold` - Minimum similarity score
/// * `max_results` - Maximum number of results to return
///
/// # Returns
///
/// Vec of similar strings, sorted by descending similarity.
pub fn find_all_similar(
    input: &str,
    candidates: &[&str],
    threshold: f64,
    max_results: usize,
) -> Vec<String> {
    let mut matches: Vec<(&str, f64)> = candidates
        .iter()
        .map(|c| (*c, jaro_winkler(input, c)))
        .filter(|(_, score)| *score >= threshold)
        .collect();

    // Sort by score descending
    matches.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Take top N results
    matches
        .into_iter()
        .take(max_results)
        .map(|(s, _)| s.to_string())
        .collect()
}

/// Get a formatted "did you mean?" message for a typo.
///
/// # Arguments
///
/// * `input` - The incorrect input
/// * `candidates` - Valid options
/// * `threshold` - Similarity threshold
///
/// # Returns
///
/// A formatted message like "did you mean 'task1'?" or None.
pub fn did_you_mean(input: &str, candidates: &[&str], threshold: f64) -> Option<String> {
    find_similar(input, candidates, threshold)
        .map(|suggestion| format!("did you mean '{}'?", suggestion))
}

/// Check if two strings are similar enough to be considered a typo.
pub fn is_typo(a: &str, b: &str, threshold: f64) -> bool {
    jaro_winkler(a, b) >= threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_similar_exact_match() {
        let suggestion = find_similar("task1", &["task1", "task2", "other"], 0.6);
        assert_eq!(suggestion, Some("task1".to_string()));
    }

    #[test]
    fn test_find_similar_typo() {
        // Common typos
        let suggestion = find_similar("taks1", &["task1", "task2", "other"], 0.6);
        assert_eq!(suggestion, Some("task1".to_string()));

        let suggestion = find_similar("tesk1", &["task1", "task2", "other"], 0.6);
        assert_eq!(suggestion, Some("task1".to_string()));
    }

    #[test]
    fn test_find_similar_no_match() {
        let suggestion = find_similar("xyz", &["task1", "task2", "other"], 0.8);
        assert_eq!(suggestion, None);
    }

    #[test]
    fn test_find_similar_empty_candidates() {
        let suggestion = find_similar("task1", &[], 0.6);
        assert_eq!(suggestion, None);
    }

    #[test]
    fn test_find_similar_case_sensitive() {
        // Jaro-Winkler is case-sensitive, so exact case matches score higher
        let suggestion = find_similar("task1", &["task1", "TASK1"], 0.6);
        // Exact case match should be preferred
        assert_eq!(suggestion, Some("task1".to_string()));

        // Case difference significantly reduces score in Jaro-Winkler
        // "TASK1" vs "task1" - only '1' matches exactly, score ~0.33
        // This verifies Jaro-Winkler is case-sensitive
        let suggestion = find_similar("TASK1", &["task1", "other"], 0.3);
        // With low threshold, should still find "task1" over "other"
        assert_eq!(suggestion, Some("task1".to_string()));
    }

    #[test]
    fn test_find_similar_prefix_bonus() {
        // Jaro-Winkler gives bonus for matching prefix
        let suggestion = find_similar("task_one", &["task_two", "other_task"], 0.6);
        assert_eq!(suggestion, Some("task_two".to_string()));
    }

    #[test]
    fn test_find_all_similar() {
        let results = find_all_similar("task", &["task1", "task2", "task3", "other"], 0.7, 3);
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.starts_with("task")));
    }

    #[test]
    fn test_find_all_similar_limited() {
        let results = find_all_similar("task", &["task1", "task2", "task3", "task4"], 0.5, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_did_you_mean() {
        let msg = did_you_mean("taks1", &["task1", "task2"], 0.6);
        assert_eq!(msg, Some("did you mean 'task1'?".to_string()));

        let msg = did_you_mean("xyz", &["task1", "task2"], 0.8);
        assert_eq!(msg, None);
    }

    #[test]
    fn test_is_typo() {
        assert!(is_typo("taks1", "task1", 0.6));
        assert!(!is_typo("xyz", "task1", 0.8));
    }

    #[test]
    fn test_schema_version_typo() {
        // Test with actual schema versions
        let versions = [
            "nika/workflow@0.1",
            "nika/workflow@0.2",
            "nika/workflow@0.10",
        ];

        // Typo: 0.01 instead of 0.1
        let suggestion = find_similar("nika/workflow@0.01", &versions, 0.8);
        assert!(suggestion.is_some());

        // Typo: workflow misspelled
        let suggestion = find_similar("nika/workfow@0.10", &versions, 0.8);
        assert_eq!(suggestion, Some("nika/workflow@0.10".to_string()));
    }
}
