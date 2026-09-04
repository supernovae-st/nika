// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Explicit file custody and the non-clobber boundary.

use std::env::VarError;
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub(super) struct KeyFiles {
    pub(super) secret: PathBuf,
    pub(super) public: PathBuf,
}

impl KeyFiles {
    /// Prepare both parents before either key write and recheck known aliases
    /// after preparation. This narrows partial-write failures, but does not
    /// make two opens atomic or protect against later directory replacement.
    pub(super) fn store(&self, secret: &str, public: &str, force: bool) -> Result<(), String> {
        self.require_distinct()?;
        for path in [&self.secret, &self.public] {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                // seam-bypass-ok: run-key custody storage, never workflow I/O
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
            }
        }
        self.require_distinct()?;
        write(&self.secret, secret, force)?;
        write(&self.public, public, force)
    }

    fn require_distinct(&self) -> Result<(), String> {
        if known_aliases(&self.secret, &self.public) {
            return Err("private and public key files must occupy distinct slots".to_owned());
        }
        Ok(())
    }
}

/// The sole native keyring-entry constructor in the trace custody plane.
/// Cargo test executables, captured stderr and an explicit off flag refuse
/// before the backend is created, including the public-only readers.
pub(crate) fn keyring_entry(user: &str) -> Option<keyring::Entry> {
    if !super::keychain_enabled() {
        return None;
    }
    keyring::Entry::new(super::KEYRING_SERVICE, user).ok()
}

pub(super) fn configured() -> Result<Option<KeyFiles>, String> {
    select(
        super::key_file_env("NIKA_RUN_KEY_FILE"),
        super::key_file_env("NIKA_RUN_PUB_FILE"),
    )
}

fn select(
    secret: Result<String, VarError>,
    public: Result<String, VarError>,
) -> Result<Option<KeyFiles>, String> {
    match (secret, public) {
        (Err(VarError::NotPresent), Err(VarError::NotPresent)) => Ok(None),
        (Ok(secret), Ok(public))
            if !secret.is_empty()
                && !public.is_empty()
                && !known_aliases(Path::new(&secret), Path::new(&public)) =>
        {
            Ok(Some(KeyFiles {
                secret: secret.into(),
                public: public.into(),
            }))
        }
        _ => Err(
            "NIKA_RUN_KEY_FILE and NIKA_RUN_PUB_FILE must both name distinct, non-empty paths"
                .to_owned(),
        ),
    }
}

/// Refuse lexical aliases, resolved path aliases (including absent files in
/// existing aliased directories), and existing Unix hard links. This probe is
/// not a filesystem lock: an adversary can still change a parent afterwards.
fn known_aliases(secret: &Path, public: &Path) -> bool {
    if secret == public {
        return true;
    }
    if let (Some(secret), Some(public)) = (resolved_slot(secret), resolved_slot(public))
        && secret == public
    {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        // seam-bypass-ok: run-key custody metadata, never workflow I/O
        if let (Ok(secret), Ok(public)) = (std::fs::metadata(secret), std::fs::metadata(public)) {
            return secret.dev() == public.dev() && secret.ino() == public.ino();
        }
    }
    false
}

fn resolved_slot(path: &Path) -> Option<PathBuf> {
    // seam-bypass-ok: run-key custody path resolution, never workflow I/O
    std::fs::canonicalize(path).ok().or_else(|| {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        Some(std::fs::canonicalize(parent).ok()?.join(path.file_name()?))
    })
}

/// Validity is not absence: corrupt, orphaned, unreadable, and symlink file
/// entries still block non-forced initialization. Keychain unavailability
/// retains the existing file-fallback policy; this is not a cross-store lock.
pub(super) fn occupied(explicit: Option<&KeyFiles>) -> bool {
    if let Some(files) = explicit {
        return entry_present(&files.secret) || entry_present(&files.public);
    }
    if [super::KEYRING_USER, super::KEYRING_USER_PUB]
        .iter()
        .any(|user| keyring_entry(user).is_some_and(|entry| entry.get_password().is_ok()))
    {
        return true;
    }
    super::fallback_key_path()
        .is_some_and(|path| entry_present(&path) || entry_present(&path.with_extension("pub")))
}

fn entry_present(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

/// Non-forced file writes also refuse a concurrent winner at the open, not
/// only at the earlier presence probe. A pair is not an atomic transaction:
/// a second-file failure may leave a new private half, but never overwrites
/// an existing entry without force. Its presence blocks an accidental retry.
pub(super) fn write(path: &Path, text: &str, force: bool) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600).custom_flags(nix::libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("cannot protect {}: {e}", path.display()))?;
    }
    file.write_all(text.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{KeyFiles, occupied, select, write};

    #[test]
    fn an_explicit_pair_is_one_authority_not_a_partial_fallback() {
        assert!(
            select(
                Err(std::env::VarError::NotPresent),
                Err(std::env::VarError::NotPresent)
            )
            .expect("default")
            .is_none()
        );
        assert!(select(Ok("key".to_owned()), Err(std::env::VarError::NotPresent)).is_err());
        assert!(select(Err(std::env::VarError::NotPresent), Ok("pub".to_owned())).is_err());
        assert!(select(Ok(String::new()), Ok("pub".to_owned())).is_err());
        assert!(select(Ok("same".to_owned()), Ok("same".to_owned())).is_err());
        let files = select(Ok("key".to_owned()), Ok("pub".to_owned()))
            .expect("valid")
            .expect("explicit");
        assert_eq!(files.secret, std::path::Path::new("key"));
        assert_eq!(files.public, std::path::Path::new("pub"));
    }

    #[cfg(unix)]
    #[test]
    fn explicit_custody_rejects_file_and_parent_aliases_before_force_can_clobber() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let alias = dir.path().join("alias");
        let real = dir.path().join("real");
        std::fs::create_dir(&real).expect("fixture parent");
        std::os::unix::fs::symlink(&real, &alias).expect("fixture parent alias");
        let key = real.join("key");
        let alias_key = alias.join("key");
        let select_paths = |a: &std::path::Path, b: &std::path::Path| {
            select(
                Ok(a.to_str().expect("fixture path").to_owned()),
                Ok(b.to_str().expect("fixture path").to_owned()),
            )
        };
        assert!(
            select_paths(&key, &alias_key).is_err(),
            "absent aliased slot"
        );
        std::fs::write(&key, "SYNTHETIC-PRIVATE").expect("fixture key");
        assert!(
            select_paths(&key, &alias_key).is_err(),
            "existing aliased slot"
        );
        let hard_link = dir.path().join("hard-link");
        std::fs::hard_link(&key, &hard_link).expect("fixture hard link");
        assert!(
            select_paths(&key, &hard_link).is_err(),
            "same inode is one slot"
        );
        assert!(select_paths(&key, &real.join("pub")).is_ok());
        assert_eq!(
            std::fs::read_to_string(&key).expect("retained key"),
            "SYNTHETIC-PRIVATE"
        );
    }

    #[test]
    fn file_presence_and_exclusive_creation_do_not_depend_on_valid_key_bytes() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let files = KeyFiles {
            secret: dir.path().join("key"),
            public: dir.path().join("pub"),
        };
        assert!(!occupied(Some(&files)));
        write(&files.secret, "SYNTHETIC-FIRST", false).expect("first writer");
        assert!(
            occupied(Some(&files)),
            "an orphan still owns the private slot"
        );
        assert!(write(&files.secret, "SYNTHETIC-SECOND", false).is_err());
        assert_eq!(
            std::fs::read_to_string(&files.secret).expect("first survives"),
            "SYNTHETIC-FIRST"
        );
        write(&files.secret, "SYNTHETIC-ROTATED", true).expect("explicit force");
        assert_eq!(
            std::fs::read_to_string(&files.secret).expect("rotated"),
            "SYNTHETIC-ROTATED"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                std::fs::metadata(&files.secret)
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn storing_a_pair_checks_aliases_even_when_the_configuration_was_selected_earlier() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let files = KeyFiles {
            secret: dir.path().join("key"),
            public: dir.path().join("pub"),
        };
        std::fs::write(&files.secret, "SYNTHETIC-OLD-PRIVATE").expect("fixture private");
        std::fs::hard_link(&files.secret, &files.public).expect("fixture alias");
        assert!(
            super::super::store_key_boxes(
                "SYNTHETIC-NEW-PRIVATE",
                "SYNTHETIC-PUBLIC",
                Some(&files),
                true
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(&files.secret).expect("retained key"),
            "SYNTHETIC-OLD-PRIVATE"
        );
    }

    #[test]
    fn an_unusable_public_parent_refuses_before_creating_the_private_half() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let bad_parent = dir.path().join("not-a-directory");
        std::fs::write(&bad_parent, "SYNTHETIC-PARENT").expect("fixture obstruction");
        let files = KeyFiles {
            secret: dir.path().join("key"),
            public: bad_parent.join("pub"),
        };
        assert!(
            super::super::store_key_boxes(
                "SYNTHETIC-PRIVATE",
                "SYNTHETIC-PUBLIC",
                Some(&files),
                false
            )
            .is_err()
        );
        assert!(
            !files.secret.exists(),
            "preparation failure must precede key creation"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_occupied_and_is_not_followed_even_with_force() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let target = dir.path().join("target");
        let files = KeyFiles {
            secret: dir.path().join("key"),
            public: dir.path().join("pub"),
        };
        std::os::unix::fs::symlink(&target, &files.secret).expect("dangling fixture link");
        assert!(occupied(Some(&files)));
        assert!(write(&files.secret, "SYNTHETIC", true).is_err());
        assert!(!target.exists());
    }
    #[test]
    fn init_without_force_preserves_an_existing_private_key_with_a_bad_public_half() {
        const CHILD: &str = "NIKA_TEST_KEY_INIT_REFUSAL";
        if super::super::key_file_env(CHILD).is_ok() {
            assert!(
                super::super::key_init(false).is_err(),
                "existing custody must refuse init"
            );
            return;
        }
        let dir = tempfile::tempdir().expect("fixture directory");
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("fixture keypair");
        let secret = pair.sk.to_box(None).expect("secret box").to_string();
        let key = dir.path().join("key");
        let public = dir.path().join("pub");
        std::fs::write(&key, &secret).expect("fixture private key");
        std::fs::write(&public, "SYNTHETIC-NOT-A-PUBLIC-KEY").expect("fixture public slot");
        // Process-local configuration: never mutate the parallel test harness's
        // environment, consult a real keychain, or touch default user custody.
        #[expect(
            clippy::disallowed_types,
            reason = "test-only self subprocess isolates custody configuration from parallel tests; no workflow effect is dispatched"
        )]
        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", "seal::key_files::tests::init_without_force_preserves_an_existing_private_key_with_a_bad_public_half"])
            .env(CHILD, "1")
            .env("NIKA_KEYCHAIN", "off")
            .env("NIKA_RUN_KEY_PASSWORD", "")
            .env("NIKA_RUN_KEY_FILE", &key)
            .env("NIKA_RUN_PUB_FILE", &public)
            .output()
            .expect("isolated init probe");
        assert!(
            output.status.success(),
            "the isolated non-clobber regression failed"
        );
        assert!(std::fs::read_to_string(&key).expect("retained key") == secret);
        assert_eq!(
            std::fs::read_to_string(&public).expect("retained public slot"),
            "SYNTHETIC-NOT-A-PUBLIC-KEY"
        );
    }

    #[test]
    fn the_guarded_constructor_is_the_only_keyring_entry_site_including_submodules() {
        let mut pending = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let needle = ["keyring::Entry", "::new("].concat();
        let mut sites = Vec::new();
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).expect("crate source directory") {
                let entry = entry.expect("source entry");
                let kind = entry.file_type().expect("entry type");
                let path = entry.path();
                if kind.is_dir() {
                    pending.push(path);
                } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                    let text = std::fs::read_to_string(&path).expect("source file");
                    for _ in text.match_indices(&needle) {
                        sites.push(path.clone());
                    }
                }
            }
        }
        assert_eq!(
            sites.len(),
            1,
            "native constructors must converge: {sites:?}"
        );
        assert!(sites[0].ends_with("seal/key_files.rs"));
        assert!(
            super::keyring_entry(super::super::KEYRING_USER).is_none(),
            "this Cargo test process cannot create a native keyring entry"
        );
    }
}
