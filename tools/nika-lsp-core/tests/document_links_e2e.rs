// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! E2E tests for document_links handler.
use nika_lsp_core::handlers::document_links::document_links;

#[test]
fn url_link_detection() {
    let yaml = r#"schema: "@0.12"
workflow: test
tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"
      method: GET
"#;
    let links = document_links(yaml);
    assert_eq!(links.len(), 1, "Expected 1 URL link, got: {:?}", links);

    let link = &links[0];
    assert_eq!(link.target, "https://api.example.com/data");
    assert_eq!(link.tooltip, "Open URL");

    // Verify the byte range points to the actual URL in the text
    let slice = &yaml[link.start_offset as usize..link.end_offset as usize];
    assert_eq!(slice, "https://api.example.com/data");
}

#[test]
fn url_unquoted() {
    let yaml = "tasks:\n  - id: f\n    fetch:\n      url: https://example.com/api\n";
    let links = document_links(yaml);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "https://example.com/api");
}

#[test]
fn file_path_link_detection() {
    let yaml = "include:\n  - path: ./partials/setup.nika.yaml\n    prefix: setup_\n";
    let links = document_links(yaml);
    assert_eq!(links.len(), 1, "Expected 1 file link, got: {:?}", links);

    let link = &links[0];
    assert!(link.target.contains("partials/setup.nika.yaml"));
    assert_eq!(link.tooltip, "Open file");

    // Verify byte range
    let slice = &yaml[link.start_offset as usize..link.end_offset as usize];
    assert_eq!(slice, "./partials/setup.nika.yaml");
}

#[test]
fn include_link_detection() {
    let yaml = "include: ./shared.nika.yaml\n";
    let links = document_links(yaml);
    assert_eq!(links.len(), 1, "Expected 1 include link, got: {:?}", links);

    let link = &links[0];
    assert!(link.target.contains("shared.nika.yaml"));
    assert_eq!(link.tooltip, "Open included workflow");
}

#[test]
fn context_section_file_detection() {
    let yaml = "context:\n  files:\n    brand: ./context/brand.md\n    data: ./context/data.json\ntasks:\n  - id: step1\n    infer: test\n";
    let links = document_links(yaml);
    assert_eq!(links.len(), 2, "Expected 2 context links, got: {:?}", links);

    for link in &links {
        assert_eq!(link.tooltip, "Open context file");
    }

    assert!(links.iter().any(|l| l.target.contains("brand.md")));
    assert!(links.iter().any(|l| l.target.contains("data.json")));

    // Verify byte ranges are valid
    for link in &links {
        let slice = &yaml[link.start_offset as usize..link.end_offset as usize];
        assert!(
            slice.starts_with("./context/"),
            "Slice should be a path, got: {:?}",
            slice
        );
    }
}

#[test]
fn skills_section_file_detection() {
    let yaml = "skills:\n  summarize: ./skills/summarize.nika.yaml\n  translate: ./skills/translate.nika.yaml\ntasks:\n  - id: step1\n    infer: test\n";
    let links = document_links(yaml);
    assert_eq!(links.len(), 2, "Expected 2 skill links, got: {:?}", links);

    for link in &links {
        assert_eq!(link.tooltip, "Open skill file");
    }
    assert!(links
        .iter()
        .any(|l| l.target.contains("summarize.nika.yaml")));
    assert!(links
        .iter()
        .any(|l| l.target.contains("translate.nika.yaml")));
}

#[test]
fn no_links_on_comments() {
    let yaml = "schema: nika/workflow@0.12\n# url: \"https://commented-out.com/api\"\n# path: ./commented/file.yaml\ntasks:\n  - id: step1\n    fetch:\n      url: https://real.example.com/data\n";
    let links = document_links(yaml);
    assert_eq!(
        links.len(),
        1,
        "Only the non-commented URL should produce a link, got: {:?}",
        links
    );
    assert!(links[0].target.contains("real.example.com"));
}

#[test]
fn empty_document() {
    assert!(document_links("").is_empty());
    assert!(document_links("   \n\n  \n").is_empty());
    assert!(document_links("\n\n\n").is_empty());
}

#[test]
fn section_resets_on_new_top_level_key() {
    let yaml = "context:\n  brand: ./context/brand.md\ntasks:\n  data: ./not-a-link.json\n";
    let links = document_links(yaml);
    assert_eq!(
        links.len(),
        1,
        "Section should reset at tasks:, got: {:?}",
        links
    );
    assert!(links[0].target.contains("brand.md"));
}

#[test]
fn mixed_links_in_single_document() {
    let yaml = "schema: \"@0.12\"\nworkflow: mixed\ncontext:\n  ref: ./context/ref.md\nskills:\n  helper: ./skills/helper.nika.yaml\ntasks:\n  - id: fetch_data\n    fetch:\n      url: \"https://api.example.com\"\n  - id: include_step\n    include: ./partials/sub.nika.yaml\n";
    let links = document_links(yaml);
    assert_eq!(links.len(), 4, "Expected 4 links total, got: {:?}", links);

    // Verify all types are present
    assert!(links.iter().any(|l| l.tooltip == "Open URL"));
    assert!(links.iter().any(|l| l.tooltip == "Open context file"));
    assert!(links.iter().any(|l| l.tooltip == "Open skill file"));
    assert!(links.iter().any(|l| l.tooltip == "Open included workflow"));
}

#[test]
fn quoted_variants_produce_same_target() {
    let double = "    path: \"./file.yaml\"\n";
    let single = "    path: './file.yaml'\n";
    let bare = "    path: ./file.yaml\n";

    let ld = document_links(double);
    let ls = document_links(single);
    let lb = document_links(bare);

    assert_eq!(ld.len(), 1);
    assert_eq!(ls.len(), 1);
    assert_eq!(lb.len(), 1);

    assert_eq!(ld[0].target, ls[0].target);
    assert_eq!(ld[0].target, lb[0].target);
}
