// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The Live host's display preference, separate from Session authority.
//!
//! The renderer only lends pure arrangement data. This adapter owns one
//! versioned HOME file through `OwnedDir`; a malformed or future file is
//! preserved, and a failure keeps the current layout usable. Nothing here
//! restores a request, consent, workflow revision, model choice or Run.

use std::path::Path;

use nika_fs::OwnedDir;
use serde_json::{Value, json};

use crate::workspace::geometry::{Arrangement, Layout};

const FILE: &str = "tui-layout.json";
const SCHEMA: &str = "nika/tui-layout@1";
// This three-share preference document has a fixed shape, unlike Session content.
const CAP: u64 = 8192;

pub(super) struct Store {
    root: Option<OwnedDir>,
    current: Option<Arrangement>,
    refusal: Option<(String, bool)>,
    reported: bool,
}

impl Store {
    pub(super) fn open(home: Option<&Path>) -> Self {
        let mut store = Self {
            root: None,
            current: None,
            refusal: None,
            reported: false,
        };
        let Some(home) = home else { return store };
        // A host-selected HOME may itself be a link; contained children never are.
        let opened = std::fs::canonicalize(home).and_then(|root| OwnedDir::open(&root));
        let root = match opened {
            Ok(root) => root,
            Err(why) => {
                store.refusal = Some((why.to_string(), true));
                return store;
            }
        };
        match read(&root) {
            Ok(current) => store.current = current,
            Err(why) => store.refusal = Some((why, true)),
        }
        store.root = Some(root);
        store
    }

    pub(super) fn current(&self) -> Option<Arrangement> {
        self.current
    }

    /// One local notice, never a refusal of the Session or an input decision.
    pub(super) fn notice(&mut self) -> Option<String> {
        if self.reported {
            return None;
        }
        let (why, preserved) = self.refusal.as_ref()?;
        self.reported = true;
        let scope = if *preserved {
            "Stored preferences are left unchanged."
        } else {
            "The save was not confirmed; inspect stored preferences before relying on them."
        };
        Some(format!(
            "Display preferences could not be kept ({why}). The current layout stays usable. {scope}"
        ))
    }

    /// Keep a settled human presentation change. No call happens while drawing.
    pub(super) fn keep(&mut self, arrangement: Arrangement) -> Option<String> {
        self.current = Some(arrangement);
        if self.refusal.is_some() {
            return self.notice();
        }
        let Some(root) = &self.root else { return None };
        // Re-read the format before replacing it: another version may have written it.
        let kept = read(root).map_err(|why| (why, true)).and_then(|_| {
            let text = encode(arrangement).map_err(|why| (why, true))?;
            let dir = root
                .create_below(&[".nika"])
                .map_err(|why| (why.to_string(), true))?;
            // An error after rename/sync cannot prove the old bytes remain.
            dir.write_atomic(FILE, &text)
                .map_err(|why| (why.to_string(), false))
        });
        if let Err(why) = kept {
            self.refusal = Some(why);
        }
        self.notice()
    }
}

fn read(root: &OwnedDir) -> Result<Option<Arrangement>, String> {
    root.read_capped_below(
        &[".nika"],
        FILE,
        CAP,
        "display preference document is too large",
    )
    .map_err(|why| why.to_string())?
    .map(|text| decode(&text))
    .transpose()
}

fn encode(arrangement: Arrangement) -> Result<String, String> {
    let layout = if arrangement.layout == Layout::Session {
        "session"
    } else if arrangement.layout == Layout::Workbench {
        "workbench"
    } else {
        return Err("this display preference version cannot keep that layout".to_owned());
    };
    if [
        arrangement.aside_width,
        arrangement.conversation_width,
        arrangement.conversation_height,
    ]
    .into_iter()
    .flatten()
    .any(|share| share > 1000)
    {
        return Err("display preference proportions must be between 0 and 1000".to_owned());
    }
    Ok(json!({
        "schema": SCHEMA,
        "layout": layout,
        "aside_width": arrangement.aside_width,
        "conversation_width": arrangement.conversation_width,
        "conversation_height": arrangement.conversation_height,
    })
    .to_string())
}

fn decode(text: &str) -> Result<Arrangement, String> {
    let invalid = || "unrecognized or invalid display preference format".to_owned();
    let value: Value = serde_json::from_str(text).map_err(|_| invalid())?;
    let object = value.as_object().ok_or_else(invalid)?;
    let keys = [
        "schema",
        "layout",
        "aside_width",
        "conversation_width",
        "conversation_height",
    ];
    if object.keys().any(|key| !keys.contains(&key.as_str()))
        || value["schema"].as_str() != Some(SCHEMA)
    {
        return Err(invalid());
    }
    let layout = match value["layout"].as_str() {
        Some("session") => Layout::Session,
        Some("workbench") => Layout::Workbench,
        _ => return Err(invalid()),
    };
    let share = |key: &str| match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|share| *share <= 1000)
            .and_then(|share| u16::try_from(share).ok())
            .map(Some)
            .ok_or_else(invalid),
    };
    Ok(Arrangement::of(Layout::Session)
        .with_layout(layout)
        .with_aside_width(share("aside_width")?)
        .with_conversation_width(share("conversation_width")?)
        .with_conversation_height(share("conversation_height")?))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Home(PathBuf);
    impl Home {
        fn new() -> Self {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("nika-layout-{}-{id}", std::process::id()));
            std::fs::create_dir(&path).expect("isolated home");
            Self(path)
        }
        fn file(&self) -> PathBuf {
            self.0.join(".nika").join(FILE)
        }
    }
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn chosen() -> Arrangement {
        Arrangement::of(Layout::Session)
            .with_layout(Layout::Workbench)
            .with_aside_width(Some(190))
            .with_conversation_width(Some(520))
            .with_conversation_height(Some(350))
    }

    #[test]
    fn codec_preserves_each_layout_and_exact_shares() {
        for layout in [Layout::Session, Layout::Workbench] {
            let value = chosen().with_layout(layout);
            assert_eq!(
                decode(&encode(value).expect("encode")).expect("decode"),
                value
            );
        }
        assert_eq!(
            decode(&encode(Arrangement::of(Layout::Session)).expect("encode")).expect("automatic"),
            Arrangement::of(Layout::Session)
        );
    }

    #[test]
    fn unknown_and_invalid_documents_are_not_reinterpreted() {
        for text in [
            "null",
            "{}",
            "not json",
            r#"{"schema":"nika/tui-layout@2","layout":"session"}"#,
            r#"{"schema":"nika/tui-layout@1","layout":"other"}"#,
            r#"{"schema":"nika/tui-layout@1","layout":"session","aside_width":1001}"#,
            r#"{"schema":"nika/tui-layout@1","layout":"session","aside_width":-1}"#,
            r#"{"schema":"nika/tui-layout@1","layout":"session","conversation_width":0.5}"#,
            r#"{"schema":"nika/tui-layout@1","layout":"session","conversation_height":"350"}"#,
            r#"{"schema":"nika/tui-layout@1","layout":"session","consent":true}"#,
        ] {
            assert!(decode(text).is_err(), "{text}");
        }
    }

    #[test]
    fn opening_is_read_only_and_settled_preferences_reopen_exactly() {
        let home = Home::new();
        let mut store = Store::open(Some(&home.0));
        assert_eq!(store.current(), None);
        assert!(!home.file().exists(), "opening creates no preference file");
        assert_eq!(store.keep(chosen()), None);
        assert_eq!(Store::open(Some(&home.0)).current(), Some(chosen()));
        assert_eq!(
            std::fs::read_to_string(home.file()).expect("stored"),
            encode(chosen()).expect("encode")
        );
    }

    #[test]
    fn invalid_shares_preserve_the_stored_bytes_and_keep_the_local_layout() {
        // Host-supplied public DTO fields can bypass the bounded builders.
        let mut invalid = [chosen(); 3];
        invalid[0].aside_width = Some(1001);
        invalid[1].conversation_width = Some(1001);
        invalid[2].conversation_height = Some(1001);
        for invalid in invalid {
            let home = Home::new();
            let mut store = Store::open(Some(&home.0));
            assert_eq!(store.keep(chosen()), None);
            let before = std::fs::read(home.file()).expect("stored bytes");
            let notice = store.keep(invalid).expect("invalid proportions notice");
            assert!(notice.contains("between 0 and 1000"), "{notice}");
            assert!(
                notice.contains("Stored preferences are left unchanged"),
                "{notice}"
            );
            assert_eq!(std::fs::read(home.file()).expect("preserved"), before);
            assert_eq!(store.current(), Some(invalid));
            assert_eq!(store.notice(), None);
        }
    }

    #[test]
    fn a_future_document_is_preserved_and_its_notice_is_not_repeated() {
        let home = Home::new();
        std::fs::create_dir(home.0.join(".nika")).expect("directory");
        let raw = r#"{"schema":"nika/tui-layout@2","layout":"future"}"#;
        std::fs::write(home.file(), raw).expect("future");
        let mut store = Store::open(Some(&home.0));
        assert_eq!(store.current(), None);
        let notice = store.notice().expect("one notice");
        assert!(notice.contains("Stored preferences are left unchanged"));
        assert_eq!(store.keep(chosen()), None);
        assert_eq!(
            store.current(),
            Some(chosen()),
            "local layout still changes"
        );
        assert_eq!(
            std::fs::read_to_string(home.file()).expect("preserved"),
            raw
        );
        assert_eq!(store.notice(), None);
    }

    #[test]
    fn a_file_changed_to_an_unknown_version_after_open_is_preserved() {
        let home = Home::new();
        let mut store = Store::open(Some(&home.0));
        assert_eq!(store.keep(Arrangement::of(Layout::Session)), None);
        let raw = r#"{"schema":"nika/tui-layout@2","layout":"future"}"#;
        std::fs::write(home.file(), raw).expect("foreign version");
        assert!(store.keep(chosen()).is_some());
        assert_eq!(
            std::fs::read_to_string(home.file()).expect("preserved"),
            raw
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_preference_cannot_write_its_target() {
        let home = Home::new();
        let other = home.0.join("outside.json");
        std::fs::write(&other, "unchanged").expect("target");
        std::fs::create_dir(home.0.join(".nika")).expect("directory");
        std::os::unix::fs::symlink(&other, home.file()).expect("link");
        let mut store = Store::open(Some(&home.0));
        assert!(store.notice().is_some());
        assert_eq!(store.keep(chosen()), None);
        assert_eq!(
            std::fs::read_to_string(&other).expect("target"),
            "unchanged"
        );
        assert!(
            home.file()
                .symlink_metadata()
                .expect("link")
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn no_home_keeps_only_the_current_presentation() {
        let mut store = Store::open(None);
        assert_eq!(store.keep(chosen()), None);
        assert_eq!(store.current(), Some(chosen()));
    }
}
