// SPDX-License-Identifier: Apache-2.0
//
// Fixture for examples/06-code-review.nika.yaml.
// It has two defects on purpose — the agent is supposed to find them.

/// Returns the mean of the samples.
pub fn average(samples: &[f64]) -> f64 {
    let total: f64 = samples.iter().sum();
    total / samples.len() as f64 // defect 1 · empty slice → division by zero → NaN
}

/// Returns the first whitespace-separated word of a line.
pub fn first_word(line: &str) -> &str {
    line.split(' ').next().unwrap() // defect 2 · .unwrap() on a library path
}
