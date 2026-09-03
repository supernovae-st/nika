use super::*;

#[test]
fn registry_cache_provenance_survives_acquisition_into_the_footer() {
    let dir = std::env::temp_dir().join(format!(
        "nika-registry-check-provenance-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join("cached.nika.yaml");
    std::fs::write(
        &path,
        "nika: cached\npermits: { exec: [date] }\ntasks:\n  clock:\n    exec: { command: [date] }\n",
    )
    .expect("registry-cache fixture");
    let cache_path = path.to_string_lossy().into_owned();
    let target = CheckTarget::registry_artifact(cache_path.clone());
    let flags = CheckFlags {
        json: false,
        infer_permits: false,
        native_strict: false,
        profile: Profile::Advisory,
    };

    let out = dispatch_targets(
        std::slice::from_ref(&target),
        &flags,
        false,
        (None, None),
        Theme::new(false, false, false),
    );
    assert_eq!(out.code, 0, "{}", out.text);
    assert!(
        out.text
            .contains("copy the registry artifact into your workspace")
            && out.text.contains("nika check --fix <copy>"),
        "{}",
        out.text
    );
    assert!(
        !out.text.contains(&format!("--fix {cache_path}")),
        "cache path leaked as a writable target:\n{}",
        out.text
    );

    let refused = dispatch_targets(
        &[target],
        &flags,
        true,
        (None, None),
        Theme::new(false, false, false),
    );
    assert_eq!(refused.code, 3);
    assert!(refused.text.contains("copy it into your workspace"));
    assert!(refused.text.contains("fix the copy"));
}

#[test]
fn direct_cache_path_is_never_a_repair_target_or_footer_command() {
    let root = std::env::temp_dir().join(format!(
        "nika-direct-cache-provenance-{}",
        std::process::id()
    ));
    let cache_root = root.join(".nika/registry");
    let dir = cache_root.join("acme/report");
    std::fs::create_dir_all(&dir).expect("cache-shaped dirs");
    let path = dir.join("cached.nika.yaml");
    std::fs::write(
        &path,
        "nika: cached\npermits: { exec: [date] }\ntasks:\n  clock:\n    exec: { command: [date] }\n",
    )
    .expect("cache fixture");
    let cache_path = path.to_string_lossy().into_owned();
    let target = CheckTarget {
        path: cache_path.clone(),
        repair_target: crate::registry::repair_target_for_path_under(
            &cache_path,
            Some(&cache_root),
        ),
    };
    assert!(target.is_registry_artifact());
    let flags = CheckFlags {
        json: false,
        infer_permits: false,
        native_strict: false,
        profile: Profile::Advisory,
    };

    let checked = dispatch_targets(
        std::slice::from_ref(&target),
        &flags,
        false,
        (None, None),
        Theme::new(false, false, false),
    );
    assert!(
        checked
            .text
            .contains("copy the registry artifact into your workspace")
    );
    assert!(!checked.text.contains(&format!("--fix {cache_path}")));

    let refused = dispatch_targets(
        &[target],
        &flags,
        true,
        (None, None),
        Theme::new(false, false, false),
    );
    assert_eq!(refused.code, 3);
    assert!(refused.text.contains("digest-pinned"));
    assert!(!refused.text.contains(&cache_path));
}

#[cfg(unix)]
#[test]
fn stream_sources_refuse_fix_while_regular_workspace_symlinks_remain_files() {
    use std::os::unix::fs::symlink;

    let flags = CheckFlags {
        json: false,
        infer_permits: false,
        native_strict: false,
        profile: Profile::Advisory,
    };
    for path in ["/dev/stdin", "/dev/fd/0"] {
        let target = CheckTarget::workspace(path);
        assert!(target.is_non_regular_source(), "{path}");
        let out = dispatch_targets(
            &[target],
            &flags,
            true,
            (None, None),
            Theme::new(false, false, false),
        );
        assert_eq!(out.code, 3);
        assert!(out.text.contains("non-regular source"));
        assert!(!out.text.contains(path));
    }

    let root = std::env::temp_dir().join(format!("nika-stream-target-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("stream arena");
    let device_alias = root.join("stdin-alias");
    let _ = std::fs::remove_file(&device_alias);
    symlink("/dev/stdin", &device_alias).expect("device symlink");
    assert!(CheckTarget::workspace(device_alias.to_string_lossy()).is_non_regular_source());

    let regular = root.join("regular.nika.yaml");
    std::fs::write(&regular, "nika: regular\ntasks: {}\n").expect("regular fixture");
    let regular_alias = root.join("regular-alias.nika.yaml");
    let _ = std::fs::remove_file(&regular_alias);
    symlink(&regular, &regular_alias).expect("regular symlink");
    let target = CheckTarget::workspace(regular_alias.to_string_lossy());
    assert_eq!(
        target.repair_target,
        nika_display::check_render::RepairTarget::WorkspaceFile
    );
}

#[cfg(unix)]
#[test]
fn atomic_fix_of_a_scratch_hardlink_never_mutates_the_cache_inode() {
    let root = std::env::temp_dir().join(format!("nika-hardlink-fix-{}", std::process::id()));
    let cache_dir = root.join(".nika/registry/acme/report");
    let scratch_dir = root.join("scratch");
    std::fs::create_dir_all(&cache_dir).expect("cache dirs");
    std::fs::create_dir_all(&scratch_dir).expect("scratch dir");
    let cached = cache_dir.join("cached.nika.yaml");
    let original =
        "nika: w\nmodel: mock/echo\ntasks:\n  think:\n    infer: { promt: hi, max_tokens: 10 }\n";
    std::fs::write(&cached, original).expect("cache fixture");
    let scratch = scratch_dir.join("copy.nika.yaml");
    let _ = std::fs::remove_file(&scratch);
    std::fs::hard_link(&cached, &scratch).expect("scratch hardlink");

    let target = CheckTarget::workspace(scratch.to_string_lossy());
    assert_eq!(
        target.repair_target,
        nika_display::check_render::RepairTarget::WorkspaceFile
    );
    let out = dispatch_targets(
        &[target],
        &CheckFlags {
            json: false,
            infer_permits: false,
            native_strict: false,
            profile: Profile::Advisory,
        },
        true,
        (None, None),
        Theme::new(false, false, false),
    );
    assert_eq!(out.code, 0, "{}", out.text);
    assert_eq!(
        std::fs::read_to_string(&cached).expect("cache remains readable"),
        original,
        "atomic publish must not mutate the cache inode"
    );
    assert!(
        std::fs::read_to_string(&scratch)
            .expect("scratch repaired")
            .contains("prompt: hi")
    );
}
