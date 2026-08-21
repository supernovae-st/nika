// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Path provenance for `nika check --fix` / `nika run` clean-gate.
//! Descended from `nika-cli::registry` so the CLI unit stays under 15k.

use nika_display::check_render::RepairTarget;
use nika_registry_client::default_cache_root;

/// Classify the source behind a path before any repair guidance is
/// rendered. Existing paths are canonicalized, so a symlink, `..`, or a
/// symlinked parent cannot disguise the digest-pinned registry cache.
/// Metadata follows ordinary symlinks: a symlink to a regular workspace
/// file remains a writable workspace entry, while devices, FIFOs, and
/// other non-regular sources must first be copied to a regular file.
///
/// A nonexistent ordinary path stays [`RepairTarget::WorkspaceFile`]. The
/// repair loop reads before it writes, so absence refuses without creating
/// anything; keeping that boundary avoids turning a missing HOME or a
/// canonicalization error into a refusal for every normal workspace path.
/// A hardlink outside the cache is likewise a workspace entry: the repair
/// publisher writes a sibling temporary file and renames it over that entry,
/// replacing the link rather than mutating the cache inode.
#[must_use]
pub fn repair_target_for_path(path: &str) -> RepairTarget {
    let root = default_cache_root().ok();
    repair_target_for_path_under(path, root.as_deref())
}

#[must_use]
pub fn repair_target_for_path_under(
    path: &str,
    registry_root: Option<&std::path::Path>,
) -> RepairTarget {
    if path == "-" {
        return RepairTarget::Stdin;
    }
    if registry_root
        .is_some_and(|root| is_registry_cache_path_under(std::path::Path::new(path), root))
    {
        return RepairTarget::RegistryArtifact;
    }
    match std::fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => RepairTarget::NonRegularSource,
        _ => RepairTarget::WorkspaceFile,
    }
}

fn is_registry_cache_path_under(path: &std::path::Path, root: &std::path::Path) -> bool {
    let (Ok(target), Ok(root)) = (std::fs::canonicalize(path), std::fs::canonicalize(root)) else {
        // An absent target cannot be repaired (the loop reads first), and
        // an absent root cannot contain an existing cache artifact.
        return false;
    };
    target.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arena(label: &str) -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "nika-repair-target-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).expect("repair-target arena");
        path
    }

    #[test]
    fn direct_dotdot_and_missing_cache_shapes_are_immutable() {
        let root = arena("cache");
        let cache_root = root.join(".nika/registry");
        let cache = cache_root.join("acme/report");
        std::fs::create_dir_all(&cache).expect("cache dirs");
        let target = cache.join("workflow.nika.yaml");
        std::fs::write(&target, "nika: cached\ntasks: {}\n").expect("cache fixture");
        let dotdot = cache.join("../report/workflow.nika.yaml");

        for path in [&target, &dotdot] {
            assert_eq!(
                repair_target_for_path_under(&path.to_string_lossy(), Some(&cache_root)),
                RepairTarget::RegistryArtifact,
                "{}",
                path.display()
            );
        }

        let missing = root.join("ordinary-missing.nika.yaml");
        assert_eq!(
            repair_target_for_path_under(&missing.to_string_lossy(), None),
            RepairTarget::WorkspaceFile,
            "a missing ordinary file reaches the read-first refusal"
        );

        let lookalike = root.join("project/.nika/registry/workflow.nika.yaml");
        std::fs::create_dir_all(lookalike.parent().expect("lookalike parent"))
            .expect("lookalike dirs");
        std::fs::write(&lookalike, "nika: local\ntasks: {}\n").expect("lookalike fixture");
        assert_eq!(
            repair_target_for_path_under(&lookalike.to_string_lossy(), Some(&cache_root)),
            RepairTarget::WorkspaceFile,
            "a component lookalike outside the canonical root stays ordinary"
        );
    }

    #[cfg(unix)]
    #[test]
    fn cache_aliases_are_immutable_but_regular_workspace_symlinks_stay_files() {
        use std::os::unix::fs::symlink;

        let root = arena("aliases");
        let cache_root = root.join(".nika/registry");
        let cache_dir = cache_root.join("acme/report");
        std::fs::create_dir_all(&cache_dir).expect("cache dirs");
        let cached = cache_dir.join("workflow.nika.yaml");
        std::fs::write(&cached, "nika: cached\ntasks: {}\n").expect("cache fixture");

        let file_alias = root.join("cache-file-alias.nika.yaml");
        symlink(&cached, &file_alias).expect("cache file symlink");
        let parent_alias = root.join("cache-parent");
        symlink(&cache_root, &parent_alias).expect("cache parent symlink");
        for path in [
            file_alias,
            parent_alias.join("acme/report/workflow.nika.yaml"),
        ] {
            assert_eq!(
                repair_target_for_path_under(&path.to_string_lossy(), Some(&cache_root)),
                RepairTarget::RegistryArtifact,
                "{}",
                path.display()
            );
        }

        let workspace = root.join("workspace.nika.yaml");
        std::fs::write(&workspace, "nika: workspace\ntasks: {}\n").expect("workspace fixture");
        let workspace_alias = root.join("workspace-alias.nika.yaml");
        symlink(&workspace, &workspace_alias).expect("workspace symlink");
        assert_eq!(
            repair_target_for_path_under(&workspace_alias.to_string_lossy(), Some(&cache_root)),
            RepairTarget::WorkspaceFile
        );
    }

    #[cfg(unix)]
    #[test]
    fn devices_and_fifos_are_non_regular_sources() {
        assert_eq!(
            repair_target_for_path("/dev/stdin"),
            RepairTarget::NonRegularSource
        );
        assert_eq!(
            repair_target_for_path("/dev/fd/0"),
            RepairTarget::NonRegularSource
        );

        let root = arena("fifo");
        let fifo = root.join("workflow.pipe");
        nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRUSR)
            .expect("mkfifo creates the fixture");
        assert_eq!(
            repair_target_for_path(&fifo.to_string_lossy()),
            RepairTarget::NonRegularSource
        );
    }
}
