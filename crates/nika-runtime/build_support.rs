// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::path::{Path, PathBuf};

/// Parse one exact lowercase Git SHA from a comment-bearing identity file.
pub(crate) fn parse_spec_sha<'a>(label: &str, raw: &'a str) -> Result<&'a str, String> {
    let values: Vec<_> = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let [sha] = values.as_slice() else {
        return Err(format!("{label} must contain exactly one identity"));
    };
    if sha.len() != 40
        || !sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "{label} identity must be a 40-character lowercase Git SHA"
        ));
    }
    Ok(sha)
}

/// Bind the conformance pin and embedded pack to one source commit.
pub(crate) fn matching_spec_sha<'a>(pin: &'a str, pack: &str) -> Result<&'a str, String> {
    let pin = parse_spec_sha("SPEC_PIN", pin)?;
    let pack = parse_spec_sha("pack/SPEC_SHA", pack)?;
    if pin != pack {
        return Err(format!(
            "SPEC_PIN {pin} differs from embedded pack identity {pack}; run scripts/sync-pack.sh <spec-checkout-at-SPEC_PIN>"
        ));
    }
    Ok(pin)
}

/// Resolve every Git file whose mutation can change the build identity.
///
/// `git rev-parse --absolute-git-dir` is not enough in a linked worktree:
/// its `HEAD` names a branch ref stored in the common Git directory, not below
/// the worktree-specific directory. Resolve every name through `--git-path` so
/// Cargo sees branch advances without needing a clean build.
/// A missing watch target is always dirty in Cargo. For a packed branch,
/// watch the nearest existing ref directory until a loose ref appears;
/// an absent packed-refs file is irrelevant while HEAD is loose or detached.
pub(crate) fn git_watch_paths(
    mut resolve: impl FnMut(&str) -> Option<PathBuf>,
    mut read: impl FnMut(&Path) -> Option<String>,
    mut exists: impl FnMut(&Path) -> bool,
) -> Vec<PathBuf> {
    let Some(head) = resolve("HEAD") else {
        return Vec::new();
    };
    let mut paths = vec![head.clone()];
    if let Some(reference) = read(&head)
        .and_then(|body| body.trim().strip_prefix("ref: ").map(str::to_owned))
        .and_then(|reference| resolve(&reference))
        .and_then(|reference| {
            reference
                .ancestors()
                .find(|path| exists(path))
                .map(Path::to_path_buf)
        })
    {
        paths.push(reference);
    }
    if let Some(packed_refs) = resolve("packed-refs").filter(|path| exists(path)) {
        paths.push(packed_refs);
    }
    paths.sort();
    paths.dedup();
    paths
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{git_watch_paths, matching_spec_sha, parse_spec_sha};

    const SHA: &str = "9fb39f0978562c1cf06ad7cb0acc680c6b455833";

    #[test]
    fn one_lowercase_sha_is_the_only_identity_shape() {
        assert_eq!(parse_spec_sha("pin", &format!("# pin\n{SHA}\n")), Ok(SHA));
        for bad in [
            "",
            "# comments only\n",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "9FB39F0978562C1CF06AD7CB0ACC680C6B455833",
            "gggggggggggggggggggggggggggggggggggggggg",
        ] {
            assert!(parse_spec_sha("pin", bad).is_err(), "accepted {bad:?}");
        }
        assert!(parse_spec_sha("pin", &format!("{SHA}\n{SHA}\n")).is_err());
    }

    #[test]
    fn pin_and_pack_must_name_the_same_commit() {
        assert_eq!(matching_spec_sha(SHA, SHA), Ok(SHA));
        assert!(matching_spec_sha(SHA, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_err());
    }

    #[test]
    fn linked_worktree_watches_the_common_branch_ref() {
        let head = PathBuf::from("/repo/.git/worktrees/carrier/HEAD");
        let branch = PathBuf::from("/repo/.git/refs/heads/feature");
        let packed = PathBuf::from("/repo/.git/packed-refs");
        let paths = git_watch_paths(
            |name| match name {
                "HEAD" => Some(head.clone()),
                "refs/heads/feature" => Some(branch.clone()),
                "packed-refs" => Some(packed.clone()),
                _ => None,
            },
            |path: &Path| (path == head).then(|| "ref: refs/heads/feature\n".to_owned()),
            |path| [head.as_path(), branch.as_path(), packed.as_path()].contains(&path),
        );

        assert!(paths.contains(&head));
        assert!(paths.contains(&branch));
        assert!(paths.contains(&packed));
        assert!(!paths.contains(&PathBuf::from(
            "/repo/.git/worktrees/carrier/refs/heads/feature"
        )));
    }

    #[test]
    fn detached_head_does_not_watch_an_absent_packed_refs_file() {
        let head = PathBuf::from("/repo/.git/HEAD");
        let paths = git_watch_paths(
            |name| Some(PathBuf::from("/repo/.git").join(name)),
            |path| (path == head).then(|| SHA.to_owned()),
            |path| path == head,
        );
        assert_eq!(paths, vec![head]);
    }

    #[test]
    fn a_packed_branch_watches_loose_ref_creation_in_an_existing_directory() {
        let head = PathBuf::from("/repo/.git/HEAD");
        let parent = PathBuf::from("/repo/.git/refs/heads");
        let packed = PathBuf::from("/repo/.git/packed-refs");
        let paths = git_watch_paths(
            |name| Some(PathBuf::from("/repo/.git").join(name)),
            |path| (path == head).then(|| "ref: refs/heads/topic/nested\n".to_owned()),
            |path| [head.as_path(), parent.as_path(), packed.as_path()].contains(&path),
        );
        assert_eq!(paths, vec![head, packed, parent]);
    }

    #[test]
    fn a_loose_branch_without_packed_refs_watches_only_existing_files() {
        let head = PathBuf::from("/repo/.git/HEAD");
        let branch = PathBuf::from("/repo/.git/refs/heads/main");
        let paths = git_watch_paths(
            |name| Some(PathBuf::from("/repo/.git").join(name)),
            |path| (path == head).then(|| "ref: refs/heads/main\n".to_owned()),
            |path| [head.as_path(), branch.as_path()].contains(&path),
        );
        assert_eq!(paths, vec![head, branch]);
    }
}
