// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE canonical models dir (issue #146) — `~/.nika/models/<owner>/<repo>/`.
//!
//! The RESOLVER (`nika model serve --model <id>`) and the DOWNLOADER
//! (`nika model pull`) share this root BY CONSTRUCTION: both call
//! [`models_root`], so the brouillon-era pull/load two-dir mismatch
//! (`model pull` wrote one dir, inference read another) cannot re-happen.
//! Same `HOME`/`USERPROFILE` resolution as the registry cache
//! (`~/.nika/registry/`) and `nika wire`'s editor configs.
//!
//! Layout: one directory per Hub repo, holding its GGUF(s) and the
//! `tokenizer.json` the serve loader wants beside them — exactly what a
//! repo download produces. Refusals are plain `String`s that teach
//! their fix — the CLI adapter maps them to its exit contract.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// A parsed model reference: `owner/repo[:QUANT]` — the ONE grammar
/// `pull`, `serve --model`, and `rm` all speak.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelRef {
    pub owner: String,
    pub name: String,
    /// The quant tag after `:` (`Q4_K_M` · `Q8_0` · `F16` …), verbatim.
    pub quant: Option<String>,
}

impl ModelRef {
    /// The Hub id (`owner/repo`) — also the installed model's list id.
    pub(crate) fn repo_id(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }

    /// This model's directory under the canonical root.
    pub(crate) fn dir(&self, root: &Path) -> PathBuf {
        root.join(&self.owner).join(&self.name)
    }
}

/// One GGUF on disk under the canonical root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstalledGguf {
    /// `owner/repo` (the two directory levels under the root).
    pub repo_id: String,
    /// The GGUF file name (`qwen3-4b-instruct-q4_k_m.gguf`).
    pub file_name: String,
    /// File size in bytes.
    pub size: u64,
    /// Absolute path to the GGUF.
    pub path: PathBuf,
}

impl InstalledGguf {
    /// The file stem — the id `serve` reports in responses.
    pub(crate) fn stem(&self) -> &str {
        self.file_name
            .strip_suffix(".gguf")
            .unwrap_or(&self.file_name)
    }

    /// The quant tag parsed from the file name (`Q4_K_M`), when one reads.
    pub(crate) fn quant(&self) -> Option<String> {
        quant_of(&self.file_name)
    }

    /// The exact id to hand `serve --model` / `rm`: `owner/repo:QUANT`
    /// when a quant reads from the file name, else the file stem.
    pub(crate) fn serve_id(&self) -> String {
        match self.quant() {
            Some(q) => format!("{}:{q}", self.repo_id),
            None => self.stem().to_owned(),
        }
    }
}

/// Parse `owner/repo[:QUANT]`. Every refusal teaches the form.
pub(crate) fn parse_model_ref(arg: &str) -> Result<ModelRef, String> {
    let (repo, quant) = match arg.split_once(':') {
        Some((repo, tag)) => {
            if tag.is_empty() || !tag.chars().all(quant_char) {
                return Err(refuse_ref(arg, "the quant tag reads wrong"));
            }
            (repo, Some(tag.to_owned()))
        }
        None => (arg, None),
    };
    let Some((owner, name)) = repo.split_once('/') else {
        return Err(refuse_ref(arg, "missing the owner/repo slash"));
    };
    if !valid_segment(owner) || !valid_segment(name) {
        return Err(refuse_ref(arg, "owner and repo are Hub path segments"));
    }
    Ok(ModelRef {
        owner: owner.to_owned(),
        name: name.to_owned(),
        quant,
    })
}

/// One Hub path segment: non-empty · starts alphanumeric · then
/// alphanumerics plus `.` `_` `-` (a second `/` fails the char check).
fn valid_segment(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// One quant-tag character (`Q4_K_M` · `IQ4_XS` · `F16`).
fn quant_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// The canonical models root: `~/.nika/models` (HOME/USERPROFILE — the
/// registry-cache + `nika wire` resolution).
///
/// # Errors
///
/// A teaching refusal when neither HOME nor USERPROFILE resolves.
// Env read is config-path state, not a secret — the same scoped
// exemption as `wire.rs::home_path` and the registry's cache root.
#[allow(clippy::disallowed_methods)]
pub fn models_root() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(".nika").join("models"))
        .ok_or_else(|| {
            "model: cannot find HOME/USERPROFILE for the models dir\n  fix: set HOME — \
             the one canonical dir is ~/.nika/models\n"
                .to_owned()
        })
}

/// Every GGUF under the root (`<root>/<owner>/<repo>/*.gguf`), sorted by
/// repo id then file name. Unreadable directories are skipped, never fatal.
pub(crate) fn installed(root: &Path) -> Vec<InstalledGguf> {
    let mut out = Vec::new();
    for (owner, owner_path) in subdirs(root) {
        for (repo, repo_path) in subdirs(&owner_path) {
            for (file_name, path, size) in files(&repo_path) {
                if is_gguf(&file_name) {
                    out.push(InstalledGguf {
                        repo_id: format!("{owner}/{repo}"),
                        file_name,
                        size,
                        path,
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| {
        (a.repo_id.as_str(), a.file_name.as_str()).cmp(&(b.repo_id.as_str(), b.file_name.as_str()))
    });
    out
}

/// Named subdirectories of `dir` (missing/unreadable → empty).
fn subdirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            out.push((name.to_owned(), path.clone()));
        }
    }
    out
}

/// Named plain files of `dir` with sizes (missing/unreadable → empty).
fn files(dir: &Path) -> Vec<(String, PathBuf, u64)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if let Ok(meta) = entry.metadata()
            && meta.is_file()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            out.push((name.to_owned(), path.clone(), meta.len()));
        }
    }
    out
}

/// A sha256 in the Hub's spelling: exactly 64 lowercase-or-uppercase
/// hex characters, nothing else (a CDN Xet/md5 `etag` is 32 and
/// declares nothing — the digest gate reads this before trusting any
/// header).
pub(crate) fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `<file>.sha256` — the digest sidecar beside a verified GGUF (the
/// store metadata `nika model list` reads).
pub(crate) fn digest_sidecar(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".sha256");
    PathBuf::from(name)
}

/// Read the verified digest recorded beside an installed GGUF, when one
/// exists (a pre-verification install has none — `None`, never an
/// error).
pub(crate) fn read_digest(file: &Path) -> Option<String> {
    let text = std::fs::read_to_string(digest_sidecar(file)).ok()?;
    let digest = text.trim();
    is_sha256_hex(digest).then(|| digest.to_owned())
}

/// The quant tag read from a GGUF file name (`model-Q4_K_M.gguf` →
/// `Q4_K_M`) — the LAST `-`/`.`-separated segment that reads as a quant
/// (quants sit at the tail by Hub convention). Uppercased.
pub(crate) fn quant_of(file_name: &str) -> Option<String> {
    let stem = file_name.strip_suffix(".gguf").unwrap_or(file_name);
    stem.split(['-', '.'])
        .filter(|seg| is_quant_token(seg))
        .next_back()
        .map(str::to_uppercase)
}

/// Does one file-name segment read as a quant (`q4_k_m` · `iq4_xs` ·
/// `f16`)? A `q`/`iq` prefix must be followed by a DIGIT — `qwen3` is a
/// model family, not a quant.
fn is_quant_token(seg: &str) -> bool {
    let s = seg.to_ascii_lowercase();
    if matches!(s.as_str(), "f16" | "f32" | "bf16") {
        return true;
    }
    s.strip_prefix("iq")
        .or_else(|| s.strip_prefix('q'))
        .is_some_and(|rest| {
            rest.chars().next().is_some_and(|c| c.is_ascii_digit())
                && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

/// Human-readable byte size (`4.2 GiB` · `812.0 MiB` · `42 B`).
#[must_use]
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    #[allow(clippy::cast_precision_loss)] // display rounding only
    let mut value = bytes as f64;
    let mut unit = "B";
    for next in UNITS {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    format!("{value:.1} {unit}")
}

/// Resolve `nika model serve --model <arg>`: paths (existing OR merely
/// path-shaped) pass through — `serve` owns the missing-path verdict
/// per build axis (the bin_smoke-pinned #482 contract); anything
/// id-shaped resolves against the canonical models dir
/// (`owner/repo[:QUANT]` or a bare file stem). The production seam.
///
/// # Errors
///
/// A teaching refusal on the id lane only — a miss lists what IS
/// installed, an ambiguous id lists the exact ids to pick from.
pub fn resolve_serve_model(arg: &str) -> Result<PathBuf, String> {
    if Path::new(arg).is_file() || path_shaped(arg) {
        // Pre-root passthrough: a path arg must reach `serve` even on
        // a machine where HOME cannot resolve.
        return Ok(PathBuf::from(arg));
    }
    let root = models_root()?;
    resolve_at(&root, arg)
}

/// [`resolve_serve_model`] over an explicit root (the testable core).
pub(crate) fn resolve_at(root: &Path, arg: &str) -> Result<PathBuf, String> {
    if Path::new(arg).is_file() {
        return Ok(PathBuf::from(arg));
    }
    if path_shaped(arg) {
        // A path-shaped arg flows THROUGH even when missing: `serve`
        // owns that verdict PER BUILD AXIS (the default build teaches
        // the `local-infer` recipe, a feature build's plan() names the
        // missing file) — the bin_smoke-pinned #482 contract. Refusing
        // here would shadow the build-recipe teach.
        return Ok(PathBuf::from(arg));
    }
    let items = installed(root);
    if arg.contains('/') {
        let mref = parse_model_ref(arg)?;
        return resolve_ref(&items, &mref);
    }
    // A bare file stem — the id `serve` reports in responses.
    let matches: Vec<&InstalledGguf> = items
        .iter()
        .filter(|m| m.stem().eq_ignore_ascii_case(arg))
        .collect();
    match matches[..] {
        [one] => Ok(one.path.clone()),
        [] => Err(missing_refusal(arg, &items)),
        _ => Err(pick_refusal(
            &format!("`{arg}` names several pulled files"),
            &matches,
        )),
    }
}

/// Resolve a parsed `owner/repo[:QUANT]` against what is installed.
fn resolve_ref(items: &[InstalledGguf], mref: &ModelRef) -> Result<PathBuf, String> {
    let repo: Vec<&InstalledGguf> = items
        .iter()
        .filter(|m| m.repo_id == mref.repo_id())
        .collect();
    if repo.is_empty() {
        return Err(missing_refusal(&mref.repo_id(), items));
    }
    match &mref.quant {
        Some(tag) => {
            let matches: Vec<&InstalledGguf> = repo
                .iter()
                .copied()
                .filter(|m| quant_matches(m, tag))
                .collect();
            match matches[..] {
                [one] => Ok(one.path.clone()),
                [] => Err(pick_refusal(
                    &format!("{} has no `{tag}` locally", mref.repo_id()),
                    &repo,
                )),
                _ => Err(pick_refusal(
                    &format!("`{tag}` matches several files of {}", mref.repo_id()),
                    &matches,
                )),
            }
        }
        None => match repo[..] {
            [one] => Ok(one.path.clone()),
            _ => Err(pick_refusal(
                &format!("{} has several GGUFs — pick one", mref.repo_id()),
                &repo,
            )),
        },
    }
}

/// Does an installed GGUF answer to a quant tag (case-insensitive —
/// parsed quant first, file-name substring as the fallback)?
fn quant_matches(m: &InstalledGguf, tag: &str) -> bool {
    m.quant().is_some_and(|q| q.eq_ignore_ascii_case(tag))
        || m.file_name
            .to_ascii_lowercase()
            .contains(&tag.to_ascii_lowercase())
}

/// Does a file name carry the `.gguf` extension (any case)?
fn is_gguf(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
}

/// Does the argument read as a file path rather than a model id?
fn path_shaped(arg: &str) -> bool {
    is_gguf(arg) || arg.starts_with('.') || arg.starts_with('/') || arg.starts_with('~')
}

/// The pick-one refusal: `head`, then each candidate as its exact
/// `owner/repo:QUANT` id with its size.
fn pick_refusal(head: &str, candidates: &[&InstalledGguf]) -> String {
    let mut text = format!("model: {head}:\n");
    for m in candidates {
        let _ = writeln!(text, "    {}  ·  {}", m.serve_id(), human_size(m.size));
    }
    text.push_str("  fix: name one id exactly (nika model list shows them all)\n");
    text
}

/// The no-match refusal — the installed LIST is the teaching surface.
fn missing_refusal(what: &str, items: &[InstalledGguf]) -> String {
    let mut text = format!("model: no local model matches `{what}`\n");
    if items.is_empty() {
        text.push_str(
            "  nothing pulled yet — nika model pull <owner/repo> (e.g. nika model pull \
             unsloth/Qwen3-4B-Instruct-2507-GGUF)\n",
        );
    } else {
        text.push_str("  installed:\n");
        for m in items {
            let _ = writeln!(text, "    {}  ·  {}", m.serve_id(), human_size(m.size));
        }
        let _ = writeln!(text, "  pull it: nika model pull {what}");
    }
    text
}

/// `nika model list` — the production seam. Never fails: an empty
/// store is a teaching line, a HOME-less machine gets the root refusal
/// text (informational either way).
#[must_use]
pub fn list() -> String {
    match models_root() {
        Ok(root) => list_at(&root),
        Err(refusal) => refusal,
    }
}

/// [`list`] over an explicit root (the testable core).
pub(crate) fn list_at(root: &Path) -> String {
    let items = installed(root);
    // The ONE dir, printed once at top — resolver and downloader both
    // read it, so what lists here is what `serve --model <id>` finds.
    let mut text = format!("models · {}\n", root.display());
    if items.is_empty() {
        text.push_str(
            "  none yet — nika model pull <owner/repo> (e.g. nika model pull \
             unsloth/Qwen3-4B-Instruct-2507-GGUF)",
        );
        return text;
    }
    let id_width = items.iter().map(|m| m.serve_id().len()).max().unwrap_or(0);
    // Per-family voice, same law as the pull receipt (#521): only a
    // POSITIVE other-family sniff earns a marker — qwen3 and unknown
    // stay bare (silence = the default just works).
    let mut any_servable = false;
    for m in &items {
        let family =
            crate::gguf::sniff_architecture(&m.path).filter(|arch| arch != crate::SERVE_FAMILY);
        if family.is_none() {
            any_servable = true;
        }
        let note = family.map_or(String::new(), |arch| format!("  ·  {arch} — runner-only"));
        // A pulled GGUF that verified against the Hub's sha256 shows
        // the proof (first 12 hex — the sidecar carries the whole).
        let verified = read_digest(&m.path)
            .map(|d| format!("  ·  sha256 ✓ {}", &d[..12]))
            .unwrap_or_default();
        let _ = writeln!(
            text,
            "  {:<id_width$}  ·  {:>9}  ·  {}{note}{verified}",
            m.serve_id(),
            human_size(m.size),
            m.file_name,
        );
    }
    if any_servable {
        if crate::SERVES {
            text.push_str(
                "\nserve one: nika model serve --model <id>  ·  reclaim: nika model rm <id>",
            );
        } else {
            // B-6c — a serve-less binary names the build that serves,
            // never the verb this one lacks (the gauntlet law « never
            // teach what THIS binary cannot do »).
            text.push_str(
                "\nserve: this binary has no local inference — cargo build -p nika-cli \
                 --features local-infer (Apple GPU: --features metal)  ·  \
                 reclaim: nika model rm <id>",
            );
        }
    } else {
        // Every row positively sniffs another family — `serve one:`
        // would be the false-receipt class. Point at the runners.
        text.push_str(
            "\nserve these via a local runner (ollama · llama.cpp · lmstudio)  ·  \
             reclaim: nika model rm <id>",
        );
    }
    text
}

/// `nika model rm <id>` — the production seam.
///
/// # Errors
///
/// A teaching refusal: a no-match lists what IS installed, an
/// ambiguous id lists the exact ids, a filesystem failure names it.
pub fn rm(id: &str) -> Result<String, String> {
    rm_at(&models_root()?, id)
}

/// [`rm`] over an explicit root (the testable core).
pub(crate) fn rm_at(root: &Path, id: &str) -> Result<String, String> {
    let items = installed(root);
    match find_rm_target(root, &items, id)? {
        RmTarget::Repo(dir, repo_id) => remove_repo(&dir, &repo_id),
        RmTarget::File(gguf) => remove_gguf_and_sweep(&gguf),
    }
}

/// What `rm <id>` names: a whole repo dir, or one GGUF.
enum RmTarget {
    Repo(PathBuf, String),
    File(InstalledGguf),
}

/// Resolve the removal target — a no-match refuses with the installed
/// list as the teaching surface.
fn find_rm_target(root: &Path, items: &[InstalledGguf], id: &str) -> Result<RmTarget, String> {
    if id.contains('/') {
        let mref = parse_model_ref(id)?;
        return match &mref.quant {
            // `owner/repo` reclaims the whole model dir (every quant +
            // the tokenizer beside them).
            None => {
                let dir = mref.dir(root);
                if dir.is_dir() {
                    Ok(RmTarget::Repo(dir, mref.repo_id()))
                } else {
                    Err(missing_refusal(id, items))
                }
            }
            // `owner/repo:QUANT` reclaims that one file.
            Some(tag) => {
                let matches: Vec<&InstalledGguf> = items
                    .iter()
                    .filter(|m| m.repo_id == mref.repo_id() && quant_matches(m, tag))
                    .collect();
                match matches[..] {
                    [one] => Ok(RmTarget::File(one.clone())),
                    [] => Err(missing_refusal(id, items)),
                    _ => Err(pick_refusal(
                        &format!("`{tag}` matches several files of {}", mref.repo_id()),
                        &matches,
                    )),
                }
            }
        };
    }
    // A bare file stem from `list`.
    let matches: Vec<&InstalledGguf> = items
        .iter()
        .filter(|m| m.stem().eq_ignore_ascii_case(id))
        .collect();
    match matches[..] {
        [one] => Ok(RmTarget::File(one.clone())),
        [] => Err(missing_refusal(id, items)),
        _ => Err(pick_refusal(
            &format!("`{id}` names several pulled files"),
            &matches,
        )),
    }
}

/// Remove a whole model dir, reporting the bytes it held.
fn remove_repo(dir: &Path, repo_id: &str) -> Result<String, String> {
    let held = files(dir);
    let freed: u64 = held.iter().map(|(_, _, size)| size).sum();
    match std::fs::remove_dir_all(dir) {
        Ok(()) => {
            prune_owner_level(dir);
            Ok(format!(
                "removed {repo_id} ({} file(s) · freed {})",
                held.len(),
                human_size(freed)
            ))
        }
        Err(e) => Err(format!(
            "model rm: cannot remove {} ({e})\n  fix: check permissions\n",
            dir.display()
        )),
    }
}

/// Remove one GGUF; when it was the repo's last, sweep the dir (an
/// orphan tokenizer serves nothing).
fn remove_gguf_and_sweep(gguf: &InstalledGguf) -> Result<String, String> {
    if let Err(e) = std::fs::remove_file(&gguf.path) {
        return Err(format!(
            "model rm: cannot remove {} ({e})\n  fix: check permissions\n",
            gguf.path.display()
        ));
    }
    let mut text = format!(
        "removed {} (freed {})",
        gguf.path.display(),
        human_size(gguf.size)
    );
    if let Some(dir) = gguf.path.parent()
        && !files(dir).iter().any(|(name, ..)| is_gguf(name))
    {
        let _ = std::fs::remove_dir_all(dir);
        prune_owner_level(dir);
        text.push_str("\n  (the repo's last GGUF — its dir swept, tokenizer with it)");
    }
    Ok(text)
}

/// Best-effort prune of a now-empty `<owner>` level (fails silently
/// when other repos remain — exactly what `remove_dir` gives).
fn prune_owner_level(repo_dir: &Path) {
    if let Some(owner) = repo_dir.parent() {
        let _ = std::fs::remove_dir(owner);
    }
}

/// A ref refusal that teaches the grammar.
fn refuse_ref(arg: &str, why: &str) -> String {
    format!(
        "model: `{arg}` is not a model reference ({why})\n  fix: the form is \
         owner/repo[:QUANT] — e.g. unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // The model-serve tests' exact fixture idiom (CARGO_TARGET_TMPDIR is
    // unset for lib unit tests — fall back beside the build dir).
    #[allow(clippy::disallowed_methods)]
    fn temp_root(name: &str) -> PathBuf {
        let base = std::env::var_os("CARGO_TARGET_TMPDIR").map_or_else(
            || {
                std::env::current_dir()
                    .expect("current dir")
                    .join("target")
                    .join("tmp")
            },
            PathBuf::from,
        );
        let dir = base.join(format!("nika-models-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    /// Lay one fake GGUF (with content = `size` bytes) under the root.
    fn plant(root: &Path, owner: &str, repo: &str, file: &str, size: usize) -> PathBuf {
        let dir = root.join(owner).join(repo);
        std::fs::create_dir_all(&dir).expect("model dir");
        let path = dir.join(file);
        std::fs::write(&path, vec![0u8; size]).expect("gguf fixture");
        path
    }

    // -- ref grammar ------------------------------------------------

    #[test]
    fn parses_bare_and_quant_refs() {
        let bare = parse_model_ref("unsloth/Qwen3-4B-Instruct-2507-GGUF").expect("bare parses");
        assert_eq!(bare.owner, "unsloth");
        assert_eq!(bare.name, "Qwen3-4B-Instruct-2507-GGUF");
        assert_eq!(bare.quant, None);
        assert_eq!(bare.repo_id(), "unsloth/Qwen3-4B-Instruct-2507-GGUF");

        let tagged = parse_model_ref("unsloth/gpt-oss-20b-GGUF:Q4_K_M").expect("quant parses");
        assert_eq!(tagged.quant.as_deref(), Some("Q4_K_M"));

        let dotted = parse_model_ref("bartowski/Llama-3.2-1B:IQ4_XS").expect("dots parse");
        assert_eq!(dotted.name, "Llama-3.2-1B");
        assert_eq!(dotted.quant.as_deref(), Some("IQ4_XS"));
    }

    #[test]
    fn refuses_malformed_refs_teaching_the_form() {
        // Each row: the bad ref — every refusal must teach the grammar.
        for bad in [
            "",
            "no-slash",
            "/repo",
            "owner/",
            "a/b/c",
            "owner/repo:",
            "owner/repo:Q4:Q5",
            "ow ner/repo",
            "owner/re po",
            "owner/repo:Q4 K M",
            "-owner/repo",
            ".owner/repo",
        ] {
            let refusal = parse_model_ref(bad).expect_err(bad);
            assert!(
                refusal.contains("owner/repo"),
                "`{bad}` must teach the form · got: {refusal}"
            );
        }
    }

    // -- quant + size rendering --------------------------------------

    #[test]
    fn quant_reads_from_hub_convention_file_names() {
        assert_eq!(
            quant_of("qwen3-4b-instruct-q4_k_m.gguf").as_deref(),
            Some("Q4_K_M")
        );
        assert_eq!(quant_of("model.Q8_0.gguf").as_deref(), Some("Q8_0"));
        assert_eq!(quant_of("Llama-3.2-1B-F16.gguf").as_deref(), Some("F16"));
        assert_eq!(
            quant_of("gpt-oss-20b-IQ4_XS.gguf").as_deref(),
            Some("IQ4_XS")
        );
        // The LAST matching segment wins (quants sit at the tail).
        assert_eq!(
            quant_of("q3-family-model-Q4_K_M.gguf").as_deref(),
            Some("Q4_K_M")
        );
        // A plain name carries no quant — and `qwen3` must not read as one.
        assert_eq!(quant_of("qwen3-tokenizer.gguf"), None);
    }

    #[test]
    fn human_size_speaks_binary_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KiB");
        assert_eq!(human_size(4 * 1024 * 1024 * 1024), "4.0 GiB");
        assert_eq!(human_size(2_890_000_000), "2.7 GiB");
    }

    // -- installed walk ----------------------------------------------

    #[test]
    fn installed_walks_two_levels_ggufs_only_sorted() {
        let root = temp_root("walk");
        plant(&root, "qwen", "b-repo", "model-q4_k_m.gguf", 8);
        plant(&root, "qwen", "a-repo", "model-q8_0.gguf", 4);
        plant(&root, "acme", "z", "z-f16.gguf", 2);
        // Non-GGUF siblings and stray top-level files never list.
        plant(&root, "qwen", "a-repo", "tokenizer.json", 1);
        std::fs::write(root.join("stray.gguf"), b"x").expect("stray");

        let items = installed(&root);
        let ids: Vec<String> = items
            .iter()
            .map(|m| format!("{}/{}", m.repo_id, m.file_name))
            .collect();
        assert_eq!(
            ids,
            [
                "acme/z/z-f16.gguf",
                "qwen/a-repo/model-q8_0.gguf",
                "qwen/b-repo/model-q4_k_m.gguf",
            ]
        );
        assert_eq!(items[0].size, 2);
        assert_eq!(items[2].stem(), "model-q4_k_m");
        assert_eq!(items[2].quant().as_deref(), Some("Q4_K_M"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn installed_on_a_missing_root_is_empty_never_fatal() {
        let root = temp_root("empty");
        let missing = root.join("never-created");
        assert!(installed(&missing).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    // -- by-id resolution (the serve seam) ---------------------------

    #[test]
    fn resolve_passes_an_existing_file_through() {
        let root = temp_root("passthrough");
        let gguf = plant(&root, "q", "r", "weights.gguf", 4);
        let resolved = resolve_at(&root, &gguf.to_string_lossy()).expect("path passes");
        assert_eq!(resolved, gguf);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_finds_the_single_gguf_by_repo_id() {
        let root = temp_root("by-id");
        let gguf = plant(&root, "Qwen", "Qwen3-4B-GGUF", "qwen3-4b-q4_k_m.gguf", 4);
        let resolved = resolve_at(&root, "Qwen/Qwen3-4B-GGUF").expect("resolves");
        assert_eq!(resolved, gguf);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_picks_the_quant_among_several() {
        let root = temp_root("quant-pick");
        plant(&root, "u", "m", "m-q4_k_m.gguf", 4);
        let q8 = plant(&root, "u", "m", "m-q8_0.gguf", 8);
        let resolved = resolve_at(&root, "u/m:Q8_0").expect("quant resolves");
        assert_eq!(resolved, q8);
        // Case-insensitive tag.
        let lower = resolve_at(&root, "u/m:q8_0").expect("lowercase tag resolves");
        assert_eq!(lower, q8);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_refuses_ambiguity_listing_the_quants() {
        let root = temp_root("ambiguous");
        plant(&root, "u", "m", "m-q4_k_m.gguf", 4);
        plant(&root, "u", "m", "m-q8_0.gguf", 8);
        let refusal = resolve_at(&root, "u/m").expect_err("must refuse");
        assert!(refusal.contains("Q4_K_M"), "{refusal}");
        assert!(refusal.contains("Q8_0"), "{refusal}");
        assert!(refusal.contains("u/m:"), "{refusal}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_refuses_a_missing_model_teaching_what_is_there() {
        let root = temp_root("missing");
        plant(&root, "here", "now", "now-q4_k_m.gguf", 4);
        let refusal = resolve_at(&root, "absent/model").expect_err("must refuse");
        assert!(
            refusal.contains("here/now"),
            "the refusal lists what IS there · got: {refusal}"
        );
        assert!(refusal.contains("nika model pull"), "{refusal}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_teaches_pull_when_nothing_is_installed() {
        let root = temp_root("nothing");
        let refusal = resolve_at(&root, "absent/model").expect_err("must refuse");
        assert!(refusal.contains("nika model pull"), "{refusal}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn resolve_finds_a_bare_stem_and_passes_a_path_shaped_miss_through() {
        let root = temp_root("stem");
        let gguf = plant(&root, "u", "m", "qwen3-4b-q4_k_m.gguf", 4);
        let resolved = resolve_at(&root, "qwen3-4b-q4_k_m").expect("stem resolves");
        assert_eq!(resolved, gguf);
        // A path-shaped argument flows THROUGH even when missing —
        // `serve` owns that verdict per build axis (default build =
        // the local-infer recipe · feature build = plan()'s missing-
        // file teach; the bin_smoke-pinned #482 contract).
        let passed = resolve_at(&root, "./missing/file.gguf").expect("path miss passes through");
        assert_eq!(passed, PathBuf::from("./missing/file.gguf"));
        let _ = std::fs::remove_dir_all(root);
    }

    // -- list ---------------------------------------------------------

    #[test]
    fn list_prints_the_dir_once_with_id_quant_size_rows() {
        let root = temp_root("list");
        plant(&root, "u", "m", "m-q4_k_m.gguf", 2048);
        let out = list_at(&root);
        assert!(
            out.contains(&root.display().to_string()),
            "the dir prints once at top · got: {out}"
        );
        assert!(out.contains("u/m"), "{out}");
        assert!(out.contains("Q4_K_M"), "{out}");
        assert!(out.contains("2.0 KiB"), "{out}");
        if crate::SERVES {
            assert!(out.contains("nika model serve"), "{out}");
        } else {
            assert!(
                out.contains("no local inference"),
                "a serve-less binary names the build, never the verb: {out}"
            );
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn list_when_empty_teaches_pull() {
        let root = temp_root("list-empty");
        let out = list_at(&root);
        assert!(out.contains("nika model pull"), "{out}");
        let _ = std::fs::remove_dir_all(root);
    }

    /// A minimal GGUF v3 header declaring `general.architecture` —
    /// the same builder shape as the gguf sniffer's own tests.
    fn gguf_header(arch: &str) -> Vec<u8> {
        let mut b = 0x4655_4747u32.to_le_bytes().to_vec();
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&1u64.to_le_bytes());
        let key = b"general.architecture";
        b.extend_from_slice(&(key.len() as u64).to_le_bytes());
        b.extend_from_slice(key);
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&(arch.len() as u64).to_le_bytes());
        b.extend_from_slice(arch.as_bytes());
        b
    }

    /// The list speaks per-family (the #521 receipt law): a positive
    /// other-family row is marked runner-only; qwen3 and unsniffable
    /// rows stay bare, and `serve one:` survives while ANY row serves.
    /// All-other-family flips the closing line to the runners.
    #[test]
    fn list_marks_other_families_and_keeps_serve_line_honest() {
        let root = temp_root("list-family");
        let qwen = plant(&root, "q", "good", "good-q8_0.gguf", 0);
        std::fs::write(&qwen, gguf_header("qwen3")).expect("qwen3 header");
        let llama = plant(&root, "b", "smol", "smol-q4_k_m.gguf", 0);
        std::fs::write(&llama, gguf_header("llama")).expect("llama header");
        let out = list_at(&root);
        assert!(out.contains("llama — runner-only"), "{out}");
        assert!(
            !out.contains("qwen3 — runner-only"),
            "the servable row stays bare: {out}"
        );
        if crate::SERVES {
            assert!(out.contains("serve one: nika model serve"), "{out}");
        } else {
            assert!(
                out.contains("no local inference"),
                "a serve-less binary names the build, never the verb: {out}"
            );
        }

        // Only the llama left → the serve promise would be false.
        std::fs::remove_dir_all(root.join("q")).expect("rm qwen dir");
        let out = list_at(&root);
        assert!(
            !out.contains("serve one: nika model serve"),
            "no false promise: {out}"
        );
        assert!(out.contains("local runner (ollama"), "{out}");
        let _ = std::fs::remove_dir_all(root);
    }

    // -- rm -------------------------------------------------------------

    #[test]
    fn rm_by_repo_id_removes_the_whole_model_dir() {
        let root = temp_root("rm-repo");
        plant(&root, "u", "m", "m-q4_k_m.gguf", 4);
        plant(&root, "u", "m", "m-q8_0.gguf", 8);
        plant(&root, "u", "m", "tokenizer.json", 1);
        let out = rm_at(&root, "u/m").expect("removes");
        assert!(!root.join("u").join("m").exists(), "dir removed");
        assert!(out.contains("freed"), "{out}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rm_by_quant_removes_one_gguf_keeping_the_rest() {
        let root = temp_root("rm-quant");
        plant(&root, "u", "m", "m-q4_k_m.gguf", 4);
        let q8 = plant(&root, "u", "m", "m-q8_0.gguf", 8);
        rm_at(&root, "u/m:Q8_0").expect("removes");
        assert!(!q8.exists(), "the named quant is gone");
        assert!(root.join("u").join("m").join("m-q4_k_m.gguf").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rm_sweeps_the_dir_when_the_last_gguf_leaves() {
        let root = temp_root("rm-sweep");
        plant(&root, "u", "m", "m-q4_k_m.gguf", 4);
        plant(&root, "u", "m", "tokenizer.json", 1);
        rm_at(&root, "u/m:Q4_K_M").expect("removes");
        assert!(
            !root.join("u").join("m").exists(),
            "a gguf-empty model dir sweeps (the orphan tokenizer with it)"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rm_refuses_a_no_match_with_the_list_as_the_teaching_surface() {
        let root = temp_root("rm-miss");
        plant(&root, "here", "now", "now-q4_k_m.gguf", 4);
        let refusal = rm_at(&root, "absent/model").expect_err("must refuse");
        assert!(
            refusal.contains("here/now"),
            "the refusal lists what IS there · got: {refusal}"
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
