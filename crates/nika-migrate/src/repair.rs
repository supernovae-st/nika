// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The byte-level repair mechanics every codemod applier shares —
//! whole-word unique-site splicing and the atomic publish. Descended out
//! of `nika-cli`'s fix verb at the C2 wall (the 15k prod-LOC budget —
//! the splice engine IS migration machinery, the migrations crate is
//! its home · the `w1`/`w2`/`esplit` family precedent).
//!
//! SAFETY over reach — a repair applies ONLY when the old token occurs
//! EXACTLY ONCE in the source as a whole word (ambiguity or a second
//! occurrence skips it honestly; a blind splice could rewrite the wrong
//! site), and the file publishes atomically (a crash mid-write never
//! truncates a workflow).

/// Byte offsets where `needle` occurs in `hay` bounded by non-word
/// characters (or the ends) — `inpit` matches `inpit:` but never
/// `originpit`. Word chars: `[A-Za-z0-9_]` (the identifier alphabet every
/// spliceable token — field · arg key · `nika:` tool id — draws from;
/// `:` in a tool id is a non-word char, so the FULL `nika:raed` needle
/// still boundary-checks correctly at both ends).
#[must_use]
pub fn word_sites(hay: &str, needle: &str) -> Vec<usize> {
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut sites = Vec::new();
    let mut from = 0;
    while let Some(pos) = hay[from..].find(needle) {
        let at = from + pos;
        let before_ok = at == 0 || !is_word(hay.as_bytes()[at - 1]);
        let end = at + needle.len();
        let after_ok = end >= hay.len() || !is_word(hay.as_bytes()[end]);
        if before_ok && after_ok {
            sites.push(at);
        }
        from = at + needle.len().max(1);
    }
    sites
}

/// Whether `token` has the SHAPE of something a splice may write into a
/// workflow: a bare identifier · a `nika:` tool id · a dotted reference —
/// never prose. A rename target that carries whitespace, a backtick or a
/// `#` is a sentence that leaked out of a teaching, and splicing it makes
/// a YAML key out of a diagnostic (measured 2026-08-18: `check --fix`
/// wrote "the fields here: nika · model · …:" as a mapping key and the
/// file stopped parsing; the shipped 0.108.0 did the same with the
/// modeline teaching). The typed door upstream is the real fix — this is
/// the byte-surgery site's own refusal, so no future caller can repeat it.
#[must_use]
pub fn is_splice_token(token: &str) -> bool {
    !token.is_empty()
        && !token
            .chars()
            .any(|c| c.is_whitespace() || c == '`' || c == '#')
}

/// Splice `old` → `new` in `source` ONLY when `old` occurs EXACTLY ONCE
/// as a whole word ([`word_sites`] — the unique-site gate) AND `new` has
/// the shape of a token ([`is_splice_token`] — never prose). Returns
/// whether it applied; an ambiguous or absent token, or a prose target,
/// leaves the source untouched.
pub fn splice_unique(source: &mut String, old: &str, new: &str) -> bool {
    if !is_splice_token(new) {
        return false;
    }
    if let [at] = word_sites(source, old)[..] {
        source.replace_range(at..at + old.len(), new);
        true
    } else {
        false
    }
}

/// Publish the healed source ATOMICALLY: write a temp sibling, then one
/// `rename` (POSIX-atomic within a filesystem — the same contract the
/// `nika-fs` effect crate documents for `nika:write`). A crash or ENOSPC
/// mid-write leaves the ORIGINAL file untouched and at most a
/// `.nika-fix-tmp.*` sibling to sweep — never a truncated workflow. The
/// temp lands in the target's own directory (rename across filesystems
/// is not atomic); any failure removes it best-effort at this one site.
///
/// # Errors
///
/// The underlying `std::fs::write` / `std::fs::rename` failure (a
/// missing parent directory, a permissions refusal); the temp sibling
/// is swept before the error returns.
pub fn write_atomic(path: &str, contents: &str) -> std::io::Result<()> {
    let target = std::path::Path::new(path);
    let dir = target.parent().filter(|p| !p.as_os_str().is_empty());
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other("write path has no file name"))?;
    let tmp_name = format!(".nika-fix-tmp.{}", name.to_string_lossy());
    let tmp = dir.map_or_else(
        || std::path::PathBuf::from(&tmp_name),
        |d| d.join(&tmp_name),
    );
    let publish = std::fs::write(&tmp, contents).and_then(|()| std::fs::rename(&tmp, target));
    if publish.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    publish
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_sites_respects_boundaries_and_counts() {
        // whole-word only — substrings inside identifiers never match
        assert_eq!(word_sites("inpit: 1", "inpit"), vec![0]);
        assert_eq!(word_sites("originpit: 1", "inpit"), Vec::<usize>::new());
        assert_eq!(word_sites("a inpit b inpit", "inpit").len(), 2);
        // the full `nika:` tool id boundary-checks at both ends
        assert_eq!(word_sites("tool: \"nika:raed\"", "nika:raed"), vec![7]);
        // …and the `raed` HALF alone still bounds on the `:` (by design:
        // the splice always receives the full typed token, never a half)
        assert_eq!(word_sites("tool: \"nika:raed\"", "raed").len(), 1);
    }

    #[test]
    fn splice_unique_applies_unique_and_skips_ambiguous() {
        let mut s = "invoke: { tool: \"nika:jq\", args: { inpit: 1 } }".to_owned();
        assert!(splice_unique(&mut s, "inpit", "input"));
        assert!(s.contains("input: 1") && !s.contains("inpit"));
        // ambiguous: two sites → untouched
        let mut s2 = "a: { promt: 1 }\nb: { promt: 2 }".to_owned();
        assert!(!splice_unique(&mut s2, "promt", "prompt"));
        assert!(s2.contains("promt: 1"), "ambiguous stays untouched");
    }

    #[test]
    fn splice_unique_refuses_a_prose_target() {
        // The corruption class: a teaching sentence offered as a rename.
        // The old token is unique — the splice would apply — and the
        // ONLY thing standing between the sentence and the file is the
        // token-shape gate. Removing it makes this test fail (the source
        // would then carry the sentence as a key).
        let body = "nika: v1\nworkflow:\n  id: hello\ntasks:\n  say:\n    exec: {}\n";
        let mut s = body.to_owned();
        let prose = "the fields here: nika · model · inputs · const · secrets · permits · run · tasks · outputs";
        assert!(!splice_unique(&mut s, "workflow", prose));
        assert_eq!(s, body, "a prose target never touches the source");
        let mut s2 = body.to_owned();
        assert!(!splice_unique(
            &mut s2,
            "workflow",
            "restore the `# ` prefix"
        ));
        assert_eq!(s2, body);
        // The shapes a repair legitimately writes all pass.
        assert!(is_splice_token("prompt"));
        assert!(is_splice_token("nika:write"));
        assert!(is_splice_token("tasks.build"));
        assert!(!is_splice_token(""));
        assert!(!is_splice_token("two words"));
    }

    #[test]
    fn write_atomic_error_path_cleans_its_temp() {
        // The failure half of the atomic contract: renaming onto a path
        // whose parent DIRECTORY vanished fails — the original (absent)
        // target stays absent and the temp is swept, never leaked.
        let dir = std::env::temp_dir().join(format!("nika-repair-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("tmpdir");
        let gone = dir.join("gone");
        std::fs::create_dir_all(&gone).expect("subdir");
        let target = gone.join("wf.nika.yaml");
        let target_str = target.to_str().expect("utf8").to_owned();
        std::fs::remove_dir_all(&gone).expect("vanish parent");
        assert!(
            write_atomic(&target_str, "nika: v1\n").is_err(),
            "no parent = publish fails"
        );
        // the temp would have lived in `gone/` — the whole dir is absent,
        // and the sweep must not have recreated anything under `dir`.
        let residue: Vec<_> = std::fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(Result::ok)
            .collect();
        assert!(residue.is_empty(), "no residue: {residue:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
