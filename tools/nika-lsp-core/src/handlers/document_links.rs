// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Document Links handler — clickable URLs and file paths.
//!
//! Protocol-agnostic: returns `Vec<LinkEntry>` with byte offsets + target.
//! The tower-lsp shim converts to `DocumentLink`.
//!
//! Scans line-by-line for:
//! - `url:` values starting with http(s) → clickable URL links
//! - `path:` / `include:` values → file:// links
//! - File paths under `context:` or `skills:` sections → file:// links

/// Protocol-agnostic document link entry.
#[derive(Debug, Clone)]
pub struct LinkEntry {
    pub start_offset: u32,
    pub end_offset: u32,
    pub target: String,
    pub tooltip: String,
}

/// Section context for tracking which YAML block we are inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    /// Inside a `context:` block
    Context,
    /// Inside a `skills:` block
    Skills,
    /// Not in a special section
    None,
}

/// Find all document links (URLs, file paths, include references).
///
/// Scans each line for:
/// - `url:` values → validate as http/https → create URL link
/// - `path:` / `include:` values → create file:// link
/// - In context/skills sections: `key: ./path` → create file link
pub fn document_links(text: &str) -> Vec<LinkEntry> {
    let mut links = Vec::new();
    let mut current_section = Section::None;
    let mut byte_offset: usize = 0;

    for line in text.lines() {
        let trimmed = line.trim();

        // Skip comment and empty lines
        if trimmed.starts_with('#') || trimmed.is_empty() {
            byte_offset = next_line_offset(text, byte_offset, line.len());
            continue;
        }

        // Track section context: detect top-level `context:` and `skills:` blocks.
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            if trimmed.starts_with("context:") {
                current_section = Section::Context;
            } else if trimmed.starts_with("skills:") {
                current_section = Section::Skills;
            } else {
                current_section = Section::None;
            }
        }

        // Pattern 1: `url:` values → HTTP(S) links
        if let Some(entry) = try_url_link(line, byte_offset) {
            links.push(entry);
        }
        // Pattern 2: `path:` or `include:` values → file links
        else if let Some(entry) = try_path_link(line, byte_offset) {
            links.push(entry);
        }
        // Pattern 3: Aliased file paths under context:/skills: sections
        else if current_section != Section::None && indent > 0 {
            if let Some(entry) = try_section_file_link(line, byte_offset, current_section) {
                links.push(entry);
            }
        }

        byte_offset = next_line_offset(text, byte_offset, line.len());
    }

    links
}

/// Try to extract an HTTP(S) URL from a `url:` line.
fn try_url_link(line: &str, line_byte_offset: usize) -> Option<LinkEntry> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("url:")?;
    let rest = rest.trim();

    let (value, quoted) = strip_quotes(rest);
    if !value.starts_with("http://") && !value.starts_with("https://") {
        return None;
    }

    // Basic URL validation: must contain a host portion
    if value.len() < 10 {
        return None;
    }

    let (start, end) = find_value_byte_range(line, line_byte_offset, rest, value, quoted);

    Some(LinkEntry {
        start_offset: start as u32,
        end_offset: end as u32,
        target: value.to_string(),
        tooltip: "Open URL".to_string(),
    })
}

/// Try to extract a file path from `path:` or `include:` lines.
fn try_path_link(line: &str, line_byte_offset: usize) -> Option<LinkEntry> {
    let trimmed = line.trim();

    // Strip optional YAML list prefix (e.g., `- path:` → `path:`)
    let key_part = trimmed.strip_prefix("- ").unwrap_or(trimmed);

    let (rest, tooltip) = if let Some(r) = key_part.strip_prefix("path:") {
        (r, "Open file")
    } else if let Some(r) = key_part.strip_prefix("include:") {
        (r, "Open included workflow")
    } else {
        return None;
    };

    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }

    let (value, quoted) = strip_quotes(rest);

    // Reject URLs (those belong to try_url_link via `url:`)
    if value.starts_with("http://") || value.starts_with("https://") {
        return None;
    }

    if value.is_empty() {
        return None;
    }

    let file_uri = path_to_file_uri(value);
    let (start, end) = find_value_byte_range(line, line_byte_offset, rest, value, quoted);

    Some(LinkEntry {
        start_offset: start as u32,
        end_offset: end as u32,
        target: file_uri,
        tooltip: tooltip.to_string(),
    })
}

/// Try to extract a file path from a section entry like `brand: ./context/brand.md`.
fn try_section_file_link(
    line: &str,
    line_byte_offset: usize,
    section: Section,
) -> Option<LinkEntry> {
    let trimmed = line.trim();

    // Must be a key: value pair
    let colon_pos = trimmed.find(':')?;
    let value_part = trimmed[colon_pos + 1..].trim();

    if value_part.is_empty() {
        return None;
    }

    let (value, quoted) = strip_quotes(value_part);

    // Must look like a file path (starts with ./ or ../ or /)
    if !value.starts_with("./") && !value.starts_with("../") && !value.starts_with('/') {
        return None;
    }

    let tooltip = match section {
        Section::Context => "Open context file",
        Section::Skills => "Open skill file",
        Section::None => return None,
    };

    let file_uri = path_to_file_uri(value);
    let (start, end) = find_value_byte_range(line, line_byte_offset, value_part, value, quoted);

    Some(LinkEntry {
        start_offset: start as u32,
        end_offset: end as u32,
        target: file_uri,
        tooltip: tooltip.to_string(),
    })
}

/// Compute the byte offset of the next line, accounting for `\r\n` vs `\n`.
///
/// `str::lines()` strips line endings, so we check the original text to determine
/// whether the line was terminated by `\r\n` (2 bytes) or `\n` (1 byte).
fn next_line_offset(text: &str, current_offset: usize, line_len: usize) -> usize {
    let line_end = current_offset + line_len;
    let bytes = text.as_bytes();
    if bytes.get(line_end) == Some(&b'\r') && bytes.get(line_end + 1) == Some(&b'\n') {
        line_end + 2
    } else if bytes.get(line_end) == Some(&b'\n') {
        line_end + 1
    } else {
        line_end // last line without trailing newline
    }
}

/// Strip surrounding quotes (single or double) from a value.
/// Returns the inner value and whether quotes were present.
fn strip_quotes(s: &str) -> (&str, bool) {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        return (&s[1..s.len() - 1], true);
    }
    (s, false)
}

/// Find the byte range of a value within the full document.
///
/// `line` is the full line text, `line_byte_offset` is the byte offset of the line
/// in the document. Returns (start, end) as absolute byte offsets.
fn find_value_byte_range(
    line: &str,
    line_byte_offset: usize,
    rest_trimmed: &str,
    value: &str,
    quoted: bool,
) -> (usize, usize) {
    let rest_offset_in_line = line.find(rest_trimmed).unwrap_or_default();

    if quoted {
        let start = line_byte_offset + rest_offset_in_line + 1; // skip opening quote
        let end = start + value.len();
        (start, end)
    } else {
        let start = line_byte_offset + rest_offset_in_line;
        let end = start + value.len();
        (start, end)
    }
}

/// Convert a raw file path to a file:// URI string.
fn path_to_file_uri(path: &str) -> String {
    format!("file://{}", path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_in_fetch_block() {
        let text = "schema: nika/workflow@0.12\n\
                    workflow: test\n\
                    tasks:\n\
                      - id: fetch_data\n\
                        fetch:\n\
                          url: \"https://api.example.com/data\"\n\
                          method: GET\n";
        let links = document_links(text);
        assert_eq!(links.len(), 1, "Expected 1 link, got: {:?}", links);

        let link = &links[0];
        assert_eq!(link.target, "https://api.example.com/data");
        assert_eq!(link.tooltip, "Open URL");
    }

    #[test]
    fn file_path_in_path_key() {
        let text = "include:\n  - path: ./partials/setup.nika.yaml\n    prefix: setup_\n";
        let links = document_links(text);
        assert_eq!(links.len(), 1);
        assert!(links[0].target.contains("partials/setup.nika.yaml"));
        assert_eq!(links[0].tooltip, "Open file");
    }

    #[test]
    fn include_key_produces_link() {
        let text = "include: ./shared.nika.yaml\n";
        let links = document_links(text);
        assert_eq!(links.len(), 1);
        assert!(links[0].target.contains("shared.nika.yaml"));
        assert_eq!(links[0].tooltip, "Open included workflow");
    }

    #[test]
    fn context_section_file_paths() {
        let text = "context:\n  files:\n    brand: ./context/brand.md\n    data: ./context/data.json\ntasks:\n  - id: step1\n    infer: test\n";
        let links = document_links(text);
        assert_eq!(links.len(), 2, "Expected 2 context links, got: {:?}", links);
        assert!(links.iter().all(|l| l.tooltip == "Open context file"));
        assert!(links.iter().any(|l| l.target.contains("brand.md")));
        assert!(links.iter().any(|l| l.target.contains("data.json")));
    }

    #[test]
    fn skills_section_file_paths() {
        let text = "skills:\n  summarize: ./skills/summarize.nika.yaml\n  translate: ./skills/translate.nika.yaml\ntasks:\n  - id: step1\n    infer: test\n";
        let links = document_links(text);
        assert_eq!(links.len(), 2, "Expected 2 skill links, got: {:?}", links);
        assert!(links.iter().all(|l| l.tooltip == "Open skill file"));
        assert!(links
            .iter()
            .any(|l| l.target.contains("summarize.nika.yaml")));
    }

    #[test]
    fn comment_lines_skipped() {
        let text = "schema: nika/workflow@0.12\n\
                    # url: \"https://commented-out.com/api\"\n\
                    # path: ./commented/file.yaml\n\
                    tasks:\n\
                      - id: step1\n\
                        fetch:\n\
                          url: https://real.example.com/data\n";
        let links = document_links(text);
        assert_eq!(
            links.len(),
            1,
            "Only real URL should link, got: {:?}",
            links
        );
        assert!(links[0].target.contains("real.example.com"));
    }

    #[test]
    fn no_links_in_empty_document() {
        assert!(document_links("").is_empty());
        assert!(document_links("   \n\n  \n").is_empty());
    }

    #[test]
    fn quoted_vs_unquoted_paths() {
        let quoted = "    path: \"./partials/setup.nika.yaml\"\n";
        let unquoted = "    path: ./partials/setup.nika.yaml\n";
        let single = "    path: './partials/setup.nika.yaml'\n";

        let links_q = document_links(quoted);
        let links_u = document_links(unquoted);
        let links_s = document_links(single);

        assert_eq!(links_q.len(), 1);
        assert_eq!(links_u.len(), 1);
        assert_eq!(links_s.len(), 1);

        // All resolve to the same target
        assert_eq!(links_q[0].target, links_u[0].target);
        assert_eq!(links_q[0].target, links_s[0].target);

        // Quoted range should exclude the quote characters (offset differs by 1)
        assert_eq!(
            links_q[0].start_offset,
            links_u[0].start_offset + 1,
            "Quoted range start should be 1 past unquoted (skips opening quote)"
        );
    }

    #[test]
    fn section_resets_on_new_top_level_key() {
        let text = "context:\n  brand: ./context/brand.md\ntasks:\n  data: ./context/data.json\n";
        let links = document_links(text);
        // Only the first entry is under context:, the second is under tasks:
        assert_eq!(
            links.len(),
            1,
            "Section should reset at tasks:, got: {:?}",
            links
        );
        assert!(links[0].target.contains("brand.md"));
    }

    #[test]
    fn byte_offsets_are_correct() {
        let text = "url: https://example.com\n";
        let links = document_links(text);
        assert_eq!(links.len(), 1);
        let link = &links[0];
        // "url: " = 5 bytes, "https://example.com" = 19 bytes
        assert_eq!(link.start_offset, 5);
        assert_eq!(link.end_offset, 24);
        assert_eq!(
            &text[link.start_offset as usize..link.end_offset as usize],
            "https://example.com"
        );
    }
}
