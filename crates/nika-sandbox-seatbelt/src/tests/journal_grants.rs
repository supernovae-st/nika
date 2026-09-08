// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! KR03: filesystem kind, syntax and final/ancestor links at the renderer.
//! These fixtures construct profiles; they never launch sandbox-exec.

use super::*;

fn profile(grant: &str, write: bool) -> Result<String, CommandSandboxError> {
    let mut spec = SandboxSpec::new();
    if write {
        spec.fs_write = vec![grant.to_owned()];
    } else {
        spec.fs_read = vec![grant.to_owned()];
    }
    build_profile(&spec, None)
}

fn rendered(grant: &str, write: bool) -> String {
    profile(grant, write).expect("valid fixture must succeed")
}

fn family(text: &str, stem: &Path, present: bool) {
    for suffix in ["-wal", "-shm", "-journal"] {
        let literal = format!("(literal \"{}{suffix}\")", stem.display());
        assert_eq!(text.contains(&literal), present, "{literal}: {text}");
    }
}

#[test]
fn bare_and_explicit_directories_never_gain_sibling_journals() {
    let s = PathBuf::from(canon(&scratch("v4-directories")));
    let dir = s.join("data");
    must_ok(std::fs::create_dir_all(&dir));
    for write in [false, true] {
        for suffix in ["", "/", "/.", "/**"] {
            let grant = format!("{}{suffix}", dir.display());
            let text = rendered(&grant, write);
            let access = if write {
                "file-write* file-read*"
            } else {
                "file-read*"
            };
            assert!(text.contains(&format!("(allow {access} (subpath \"{}\")", dir.display())));
            family(&text, &dir, false);
            assert!(!text.contains(&format!("(literal \"{}\")", s.display())));
        }
    }
    let _ = std::fs::remove_dir_all(&s);
}

#[test]
fn exact_regular_and_clean_absent_files_keep_journal_family() {
    let s = PathBuf::from(canon(&scratch("v4-files")));
    must_ok(std::fs::write(s.join("state.db"), b""));
    for write in [false, true] {
        for name in ["state.db", "new.db", "fresh/new.db"] {
            let file = s.join(name);
            let text = rendered(&file.display().to_string(), write);
            family(&text, &file, true);
            let parent = file.parent().expect("fixture has a parent");
            assert!(text.contains(&format!("(literal \"{}\")", parent.display())));
        }
    }
    let _ = std::fs::remove_dir_all(&s);
}

#[test]
fn healthy_ancestor_alias_preserves_file_kind_and_canonical_spelling() {
    let s = PathBuf::from(canon(&scratch("v4-ancestor")));
    let real = s.join("real");
    let alias = s.join("alias");
    must_ok(std::fs::create_dir_all(real.join("data")));
    must_ok(std::fs::write(real.join("state.db"), b""));
    must_ok(std::os::unix::fs::symlink(&real, &alias));
    for write in [false, true] {
        for (name, journals) in [("data", false), ("state.db", true), ("fresh/new.db", true)] {
            let text = rendered(&alias.join(name).display().to_string(), write);
            let effective = real.join(name);
            assert!(text.contains(&format!("(subpath \"{}\")", effective.display())));
            assert!(!text.contains(&alias.display().to_string()));
            family(&text, &effective, journals);
        }
    }
    let _ = std::fs::remove_dir_all(&s);
}

#[test]
fn bare_final_symlinks_refuse_without_borrowing_target_type() {
    let s = PathBuf::from(canon(&scratch("v4-final-links")));
    must_ok(std::fs::create_dir_all(s.join("real")));
    must_ok(std::fs::write(s.join("state.db"), b""));
    for (link, target) in [
        ("dir-link", "real"),
        ("file-link", "state.db"),
        ("dangling", "missing"),
        ("loop-a", "loop-b"),
        ("loop-b", "loop-a"),
    ] {
        must_ok(std::os::unix::fs::symlink(s.join(target), s.join(link)));
    }
    for write in [false, true] {
        for name in ["dir-link", "file-link", "dangling", "loop-a"] {
            assert!(matches!(
                profile(&s.join(name).display().to_string(), write),
                Err(CommandSandboxError::Profile { .. })
            ));
        }
        // Explicit directory specs keep the final component lexical. The
        // upstream identity judge refuses such pivots before normal dispatch.
        for suffix in ["/", "/.", "/**"] {
            let text = rendered(&format!("{}{suffix}", s.join("dir-link").display()), write);
            assert!(text.contains(&format!("(subpath \"{}\")", s.join("dir-link").display())));
            assert!(!text.contains(&s.join("real").display().to_string()));
            family(&text, &s.join("dir-link"), false);
        }
    }
    let _ = std::fs::remove_dir_all(&s);
}

#[test]
fn unsafe_ancestors_and_non_absence_metadata_errors_refuse() {
    let s = PathBuf::from(canon(&scratch("v4-errors")));
    must_ok(std::fs::write(s.join("afile"), b""));
    must_ok(std::os::unix::fs::symlink(
        s.join("missing"),
        s.join("dangling"),
    ));
    must_ok(std::os::unix::fs::symlink(s.join("loop"), s.join("loop")));
    // Exceeds NAME_MAX on the target macOS/Linux filesystems. Its parent
    // resolves normally; metadata failure must not become "new file".
    let too_long = "x".repeat(512);
    for write in [false, true] {
        for name in [
            "afile/leaf",
            "dangling/leaf",
            "loop/leaf",
            too_long.as_str(),
        ] {
            assert!(matches!(
                profile(&s.join(name).display().to_string(), write),
                Err(CommandSandboxError::Profile { .. })
            ));
        }
    }
    let _ = std::fs::remove_dir_all(&s);
}
