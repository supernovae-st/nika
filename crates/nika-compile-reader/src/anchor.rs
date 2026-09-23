// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A near-miss citation: the request span a seat names when its citation is a letter or two
//! away from the request's own words (« espere » for « espera »). The span is the request's;
//! the seat's spelling is never kept.
//! Moved from nika-compile to the reader at the 15k prod-LOC wall (2026-09-22), unchanged.

/// The request span a near-miss citation names: a citation of at least 24 characters that is
/// at most two edits (a letter changed, dropped or added) away from exactly one span of the
/// request, one of whose ends the citation reproduces. The request's own words are used.
pub(crate) fn near_excerpt(
    folded: &str,
    offsets: &[usize],
    intent: &str,
    cited: &str,
) -> Option<String> {
    let cited: Vec<char> = cited.chars().collect();
    if cited.len() < 24 {
        return None;
    }
    let text: Vec<(usize, char)> = folded.char_indices().collect();
    let head: String = cited.iter().take(4).collect();
    let tail: String = cited
        .iter()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    // Every window within two edits, then the least distance: the request's clause with one
    // letter doubled is one edit away, its truncation two.
    let mut near: Vec<(usize, usize, usize)> = Vec::new();
    for width in cited.len().saturating_sub(2)..=cited.len() + 2 {
        for start in 0..text.len().saturating_sub(width - 1) {
            let window: Vec<char> = text[start..start + width].iter().map(|(_, c)| *c).collect();
            let window_text: String = window.iter().collect();
            if !(window_text.starts_with(&head) || window_text.ends_with(&tail)) {
                continue;
            }
            if let Some(distance) = edit_distance_within(&cited, &window, 2) {
                let last = text[start + width - 1];
                near.push((text[start].0, last.0 + last.1.len_utf8(), distance));
            }
        }
    }
    let least = near.iter().map(|c| c.2).min()?;
    let mut spans: Vec<(usize, usize)> = near
        .iter()
        .filter(|c| c.2 == least)
        .map(|c| (c.0, c.1))
        .collect();
    spans.sort_unstable();
    // Two distinct spans at the least distance: the citation is not unique, nothing is guessed.
    if spans
        .windows(2)
        .any(|pair| pair[1].0.abs_diff(pair[0].0) > 2)
    {
        return None;
    }
    let found = spans.into_iter().max_by_key(|(s, e)| e - s);
    let (start, end) = found?;
    let begin = *offsets.get(start)?;
    let final_byte = *offsets.get(end - 1)?;
    let stop = final_byte + intent.get(final_byte..)?.chars().next()?.len_utf8();
    intent.get(begin..stop).map(str::to_owned)
}

/// The Levenshtein distance between two texts when it is at most `k`, else `None`; a band
/// of `k` around the diagonal keeps it linear in the texts.
fn edit_distance_within(a: &[char], b: &[char], k: usize) -> Option<usize> {
    if a.len().abs_diff(b.len()) > k {
        return None;
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, &ca) in a.iter().enumerate() {
        let mut current = vec![usize::MAX / 2; b.len() + 1];
        current[0] = i + 1;
        let low = (i + 1).saturating_sub(k);
        let high = (i + 1 + k).min(b.len());
        for j in low.max(1)..=high {
            let cost = usize::from(ca != b[j - 1]);
            current[j] = (previous[j] + 1)
                .min(current[j - 1] + 1)
                .min(previous[j - 1] + cost);
        }
        if current.iter().all(|&d| d > k) {
            return None;
        }
        previous = current;
    }
    let distance = previous[b.len()];
    (distance <= k).then_some(distance)
}
