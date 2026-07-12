// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika model pull` — first-class Hugging Face acquisition for the
//! native path (issue #146).
//!
//! One command from the Hub to a sovereign in-process run: resolve the
//! repo's file tree (sizes BEFORE any download), choose the GGUF
//! (`Q4_K_M` default · explicit via `:TAG`), stream it into the ONE
//! canonical models dir ([`crate::store`]), and bring `tokenizer.json`
//! along — the exact sibling layout `nika model serve` loads.
//!
//! # Why the HOUSE http seam, not the `hf-hub` crate (decision · honest)
//!
//! `hf-hub` was evaluated and NOT taken, on three architectural refusals
//! (not a licence failure):
//! 1. **Layer discipline** — `deny.toml` layer-bans `reqwest` to its
//!    wrappers so every outbound byte rides `nika-http`'s four-layer
//!    SSRF defense; `hf-hub` embeds its OWN `ureq` stack, a second HTTP
//!    path around that seam.
//! 2. **Testability** — `hf-hub` accepts no client injection; the
//!    kernel `HttpGetDyn`/`HttpPostDyn` traits make this whole module
//!    mockable (the `registry::` #452 precedent).
//! 3. **The one-dir law** — `hf-hub` imposes its snapshot cache layout
//!    (`models--owner--repo/snapshots/<rev>/…`); this issue exists
//!    because pull and load once read DIFFERENT dirs, so the downloader
//!    writes the resolver's layout directly.
//!
//! Resume rides a `<file>.part` + `Range:` request (the Hub serves
//! ranges); an interrupted pull re-runs from where it stopped. The
//! fetch is CLI-level, like `registry:` pulls — a workflow's `permits:`
//! never govern it. `HF_TOKEN` (when set) authenticates gated repos;
//! `nika-http` strips the header on the cross-origin CDN redirect (the
//! signed URL carries its own grant).

// Progress DURING a blocking download cannot be deferred through the
// returned receipt — the same sanctioned exemption `serve` carries.
#![allow(clippy::disallowed_macros, clippy::print_stderr)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use nika_kernel::http::{HttpGetDyn, HttpPostDyn, HttpRequest, HttpStreamResponse};

use crate::store::{self, ModelRef};

/// The Hugging Face Hub origin (metadata + `resolve/` downloads; the
/// CDN hop arrives as a redirect `nika-http` re-vets per hop).
const HUB: &str = "https://huggingface.co";

/// The Hub default quant (the issue's lock): picked when the repo tags
/// one and no explicit `:TAG` was given.
pub(crate) const DEFAULT_QUANT: &str = "Q4_K_M";

/// Sizes at or above this confirm before downloading (`--yes` bypasses).
pub(crate) const CONFIRM_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// One file row of the Hub tree listing (`GET api/models/<repo>/tree/main`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct TreeEntry {
    /// `"file"` or `"directory"` — only files are candidates.
    #[serde(rename = "type")]
    pub kind: String,
    /// The path inside the repo (top-level GGUF convention).
    pub path: String,
    /// Size in bytes (the confirm gate reads it BEFORE any download).
    #[serde(default)]
    pub size: u64,
}

/// The confirm-gate verdict over a size (pure — the prompt I/O lives in
/// the blocking seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmGate {
    /// Under the threshold, or explicitly `--yes`.
    Proceed,
    /// Over the threshold on a terminal: ask.
    Prompt,
    /// Over the threshold, no terminal, no `--yes`: refuse (the CI shape).
    RefuseNeedsYes,
}

/// Gate a download of `size` bytes: `yes` bypasses, small sizes pass,
/// big sizes prompt on a terminal and refuse in a pipe.
pub(crate) fn confirm_gate(size: u64, yes: bool, interactive: bool) -> ConfirmGate {
    if yes || size < CONFIRM_BYTES {
        ConfirmGate::Proceed
    } else if interactive {
        ConfirmGate::Prompt
    } else {
        ConfirmGate::RefuseNeedsYes
    }
}

/// Parse the tree listing JSON into file rows (directories drop here).
pub(crate) fn parse_tree(body: &[u8], repo_id: &str) -> Result<Vec<TreeEntry>, Refusal> {
    let entries: Vec<TreeEntry> = serde_json::from_slice(body).map_err(|e| {
        refuse(format!(
            "model pull: the Hub's file listing for {repo_id} did not parse ({e})\n  fix: \
             check the id on huggingface.co — `owner/repo`, exactly\n"
        ))
    })?;
    Ok(entries.into_iter().filter(|e| e.kind == "file").collect())
}

/// Choose THE GGUF the ref names: explicit `:TAG` matches its quant
/// (case-insensitively); no tag prefers [`DEFAULT_QUANT`], then a
/// repo's single GGUF; every miss refuses LISTING the quants that exist.
pub(crate) fn choose_gguf<'a>(
    entries: &'a [TreeEntry],
    mref: &ModelRef,
) -> Result<&'a TreeEntry, Refusal> {
    let ggufs: Vec<&TreeEntry> = entries
        .iter()
        .filter(|e| e.path.to_ascii_lowercase().ends_with(".gguf"))
        .collect();
    if ggufs.is_empty() {
        return Err(refuse(format!(
            "model pull: {} has no GGUF file\n  fix: pick a GGUF repo (quantized mirrors \
             usually end in -GGUF — search \"{} gguf\" on huggingface.co)\n",
            mref.repo_id(),
            mref.name
        )));
    }
    if let Some(tag) = &mref.quant {
        return pick_tagged(&ggufs, mref, tag);
    }
    let defaults: Vec<&TreeEntry> = ggufs
        .iter()
        .copied()
        .filter(|e| entry_matches_tag(e, DEFAULT_QUANT))
        .collect();
    if let [one] = defaults[..] {
        return Ok(one);
    }
    if defaults.len() > 1 {
        return Err(quant_menu(mref, &defaults, None));
    }
    if let [one] = ggufs[..] {
        return Ok(one);
    }
    Err(quant_menu(mref, &ggufs, None))
}

/// The explicit-`:TAG` arm of [`choose_gguf`].
fn pick_tagged<'a>(
    ggufs: &[&'a TreeEntry],
    mref: &ModelRef,
    tag: &str,
) -> Result<&'a TreeEntry, Refusal> {
    let matches: Vec<&TreeEntry> = ggufs
        .iter()
        .copied()
        .filter(|e| entry_matches_tag(e, tag))
        .collect();
    match matches[..] {
        [one] => Ok(one),
        [] => Err(quant_menu(mref, ggufs, Some(tag))),
        _ => Err(quant_menu(mref, &matches, Some(tag))),
    }
}

/// Does a tree entry's file name answer to a quant tag?
fn entry_matches_tag(e: &TreeEntry, tag: &str) -> bool {
    let file = file_name(&e.path);
    store::quant_of(file).is_some_and(|q| q.eq_ignore_ascii_case(tag))
        || file
            .to_ascii_lowercase()
            .contains(&tag.to_ascii_lowercase())
}

/// The pick-a-quant refusal: each candidate as its exact pull command.
fn quant_menu(mref: &ModelRef, ggufs: &[&TreeEntry], asked: Option<&str>) -> Refusal {
    let head = match asked {
        Some(tag) => format!(
            "model pull: `{tag}` does not name exactly one GGUF of {}",
            mref.repo_id()
        ),
        None => format!(
            "model pull: {} ships several GGUFs and none is the {DEFAULT_QUANT} default",
            mref.repo_id()
        ),
    };
    let mut text = format!("{head} — the menu:\n");
    for e in ggufs {
        let file = file_name(&e.path);
        let tag = store::quant_of(file).unwrap_or_else(|| file.to_owned());
        let _ = writeln!(
            text,
            "    nika model pull {}:{tag}  ·  {}",
            mref.repo_id(),
            store::human_size(e.size)
        );
    }
    refuse(text)
}

/// The repo's top-level `tokenizer.json`, when it ships one.
pub(crate) fn tokenizer_entry(entries: &[TreeEntry]) -> Option<&TreeEntry> {
    entries.iter().find(|e| e.path == "tokenizer.json")
}

/// The file name of a repo path (`sub/dir/w.gguf` → `w.gguf`) — the
/// basename ONLY ever reaches the disk (repo paths are remote input).
/// A basename carrying `\\` or spelling `..` is REJECTED to a fixed
/// name: on Windows `Path::join` treats `\\` as a separator, so a
/// crafted tree entry like `x\\..\\evil.gguf` (ends `.gguf`, no `/`)
/// would otherwise escape the models dir. Unix never splits on `\\`,
/// so the same law on both platforms costs nothing and closes the
/// mirror-controlled traversal edge.
fn file_name(repo_path: &str) -> &str {
    let base = repo_path.rsplit('/').next().unwrap_or(repo_path);
    if base.contains('\\') || base == ".." {
        "rejected-remote-name"
    } else {
        base
    }
}

/// Does a remote-supplied string shape as a Hub `owner/repo` id?
/// `base_model` is REMOTE data that flows into a URL path — anything
/// outside `[A-Za-z0-9._-]/[A-Za-z0-9._-]` is dropped, so a crafted
/// card cannot steer the request off the two API endpoints.
fn valid_repo_id(id: &str) -> bool {
    let Some((owner, name)) = id.split_once('/') else {
        return false;
    };
    let ok = |s: &str| {
        !s.is_empty()
            && !s.starts_with('.')
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    };
    ok(owner) && ok(name)
}

/// Borrow `tokenizer.json` from the GGUF repo's declared base model,
/// landing it BESIDE the GGUF (the sibling layout `serve` loads).
/// Returns the base repo id on success; every miss (no card · no base
/// tree · base ships none · transport) answers `None` — the caller
/// prints the teaching note instead.
async fn borrow_base_tokenizer<H: HttpGetDyn + HttpPostDyn>(
    scout: &Puller<H>,
    mref: &ModelRef,
) -> Option<String> {
    let base_id = scout.base_model(mref).await?;
    let (owner, name) = base_id.split_once('/')?;
    let base = ModelRef {
        owner: owner.to_owned(),
        name: name.to_owned(),
        quant: None,
    };
    let entries = scout.tree(&base).await.ok()?;
    let tok = tokenizer_entry(&entries)?.clone();
    let beside = mref.dir(&scout.root);
    scout.download_into(&base, &tok, &beside).await.ok()?;
    Some(base_id)
}

/// The Hub acquisition client over the injected kernel http seam.
pub(crate) struct Puller<H> {
    http: H,
    root: PathBuf,
    token: Option<String>,
    progress: bool,
}

impl<H: HttpGetDyn + HttpPostDyn> Puller<H> {
    pub(crate) fn new(http: H, root: PathBuf, token: Option<String>, progress: bool) -> Self {
        Self {
            http,
            root,
            token,
            progress,
        }
    }

    /// Fetch the repo's file tree (`path` + `size` per file) — the
    /// metadata the size print + confirm gate read BEFORE any download.
    pub(crate) async fn tree(&self, mref: &ModelRef) -> Result<Vec<TreeEntry>, Refusal> {
        let url = format!("{HUB}/api/models/{}/{}/tree/main", mref.owner, mref.name);
        let mut request = HttpRequest::get(url);
        request.headers = self.auth_headers();
        let response = self
            .http
            .get(request)
            .await
            .map_err(|e| transport_refusal(&mref.repo_id(), &e))?;
        match response.status {
            200 => parse_tree(&response.body, &mref.repo_id()),
            401 | 403 => Err(gated_refusal(
                &mref.repo_id(),
                response.status,
                self.token.is_some(),
            )),
            404 => Err(refuse(format!(
                "model pull: {} is not on the Hub (404)\n  fix: check the id — owner/repo, \
                 exactly as huggingface.co spells it\n",
                mref.repo_id()
            ))),
            status => Err(refuse(format!(
                "model pull: the Hub answered {status} for {}\n  fix: try again shortly — or \
                 open huggingface.co/{} in a browser\n",
                mref.repo_id(),
                mref.repo_id()
            ))),
        }
    }

    /// Stream one repo file into the model's dir. Resumes a `<file>.part`
    /// via `Range:` (206 appends · 200 restarts) and renames into place
    /// only when the byte count matches the tree's declared size.
    /// Returns the landing path + the byte offset the transfer resumed
    /// from (`0` = a fresh pull) — the receipt tells the truth about
    /// what happened (a resumed pull must not read like a fresh one).
    /// The repo's declared source model (`cardData.base_model` on the
    /// Hub's model-info API) — where the tokenizer borrow looks.
    /// ADVISORY: any miss (non-200 · no card · an id that does not
    /// shape as `owner/repo`) answers `None`, never an error.
    pub(crate) async fn base_model(&self, mref: &ModelRef) -> Option<String> {
        let mut request =
            HttpRequest::get(format!("{HUB}/api/models/{}/{}", mref.owner, mref.name));
        request.headers = self.auth_headers();
        let response = self.http.get(request).await.ok()?;
        if response.status != 200 {
            return None;
        }
        let v: serde_json::Value = serde_json::from_slice(&response.body).ok()?;
        let card = v.get("cardData")?.get("base_model")?;
        // The Hub allows both a string and a list — take the first.
        let id = match card {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(a) => a.first()?.as_str()?.to_owned(),
            _ => return None,
        };
        valid_repo_id(&id).then_some(id)
    }

    pub(crate) async fn download(
        &self,
        mref: &ModelRef,
        entry: &TreeEntry,
    ) -> Result<(PathBuf, u64), Refusal> {
        let dir = mref.dir(&self.root);
        self.download_into(mref, entry, &dir).await
    }

    /// [`Self::download`] with an explicit destination dir — the
    /// tokenizer borrow fetches from the BASE repo but lands beside the
    /// GGUF (the sibling layout `serve` loads is the one law).
    pub(crate) async fn download_into(
        &self,
        mref: &ModelRef,
        entry: &TreeEntry,
        dir: &Path,
    ) -> Result<(PathBuf, u64), Refusal> {
        let file = file_name(&entry.path);
        std::fs::create_dir_all(dir).map_err(|e| {
            refuse(format!(
                "model pull: cannot create {} ({e})\n  fix: check permissions on the models dir\n",
                dir.display()
            ))
        })?;
        let dest = dir.join(file);
        let part = part_path(&dest);
        let resume_from = std::fs::metadata(&part).map_or(0, |m| m.len());

        let mut request = HttpRequest::get(format!(
            "{HUB}/{}/{}/resolve/main/{}",
            mref.owner, mref.name, entry.path
        ));
        request.headers = self.auth_headers();
        if resume_from > 0 {
            // The Hub serves ranges — an interrupted pull re-runs from
            // where it stopped instead of re-paying the gigabytes.
            request
                .headers
                .insert("range".to_owned(), format!("bytes={resume_from}-"));
            eprintln!("  resuming {file} at {}", store::human_size(resume_from));
        }
        let response = self
            .http
            .send_streaming(request)
            .await
            .map_err(|e| transport_refusal(&mref.repo_id(), &e))?;
        let (out, start) = begin_write(
            response.status,
            &part,
            resume_from,
            &mref.repo_id(),
            self.token.is_some(),
            file,
        )?;
        let written = self.drain(response, out, &part, start, entry.size).await?;
        if written != entry.size {
            return Err(refuse(format!(
                "model pull: got {} of {} for {file}\n  fix: re-run the pull — it resumes \
                 from where this stopped (the .part stays)\n",
                store::human_size(written),
                store::human_size(entry.size)
            )));
        }
        std::fs::rename(&part, &dest).map_err(|e| {
            refuse(format!(
                "model pull: cannot move {} into place ({e})\n  fix: check the models dir\n",
                part.display()
            ))
        })?;
        Ok((dest, start))
    }

    /// Write the stream to the `.part` file, ticking progress; returns
    /// the total byte count (resume start included).
    async fn drain(
        &self,
        response: HttpStreamResponse,
        mut out: std::fs::File,
        part: &Path,
        start: u64,
        total: u64,
    ) -> Result<u64, Refusal> {
        let mut body = response.body;
        let mut written = start;
        let mut last_tick = start;
        while let Some(item) = std::future::poll_fn(|cx| body.as_mut().poll_next(cx)).await {
            let chunk = item.map_err(|e| {
                refuse(format!(
                    "model pull: the transfer broke mid-stream ({e})\n  fix: re-run the pull \
                     — it resumes from the .part\n"
                ))
            })?;
            out.write_all(&chunk).map_err(|e| {
                refuse(format!(
                    "model pull: cannot write {} ({e})\n  fix: check disk space\n",
                    part.display()
                ))
            })?;
            written = written.saturating_add(chunk.len() as u64);
            if self.progress && written.saturating_sub(last_tick) >= PROGRESS_EVERY {
                eprint!(
                    "\r  {} / {}",
                    store::human_size(written),
                    store::human_size(total)
                );
                last_tick = written;
            }
        }
        if self.progress && last_tick > start {
            eprintln!(
                "\r  {} / {}",
                store::human_size(written),
                store::human_size(total)
            );
        }
        out.sync_all().map_err(|e| {
            refuse(format!(
                "model pull: cannot flush {} ({e})\n  fix: check disk space\n",
                part.display()
            ))
        })?;
        Ok(written)
    }

    /// `Authorization: Bearer` rides only when `HF_TOKEN` is set —
    /// `nika-http` strips it on the cross-origin CDN redirect.
    fn auth_headers(&self) -> BTreeMap<String, String> {
        let mut headers = BTreeMap::new();
        if let Some(token) = &self.token {
            headers.insert("authorization".to_owned(), format!("Bearer {token}"));
        }
        headers
    }
}

/// Progress tick granularity (stderr TTY only).
const PROGRESS_EVERY: u64 = 64 * 1024 * 1024;

/// Open the `.part` per the server's answer: `200` restarts from zero
/// (the server ignored/refused the range), `206` appends; auth/missing
/// statuses refuse with their teach.
fn begin_write(
    status: u16,
    part: &Path,
    resume_from: u64,
    repo_id: &str,
    has_token: bool,
    file: &str,
) -> Result<(std::fs::File, u64), Refusal> {
    let open_refusal = |e: std::io::Error| {
        refuse(format!(
            "model pull: cannot write {} ({e})\n  fix: check permissions/disk on the models dir\n",
            part.display()
        ))
    };
    match status {
        200 => Ok((std::fs::File::create(part).map_err(open_refusal)?, 0)),
        206 => Ok((
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(part)
                .map_err(open_refusal)?,
            resume_from,
        )),
        401 | 403 => Err(gated_refusal(repo_id, status, has_token)),
        404 => Err(refuse(format!(
            "model pull: {file} is gone from the repo (404)\n  fix: re-run `nika model pull \
             {repo_id}` — the tree may have moved under you\n"
        ))),
        s => Err(refuse(format!(
            "model pull: the Hub answered {s} for {file}\n  fix: try again shortly\n"
        ))),
    }
}

/// The access teach — a 401/403 is an ACCESS answer, not a bug. The Hub
/// hides existence: a MISSING repo also answers 401 (verified live
/// 2026-07-12), so the teach names the typo lane BEFORE the token lane.
fn gated_refusal(repo_id: &str, status: u16, has_token: bool) -> Refusal {
    let fix = if has_token {
        "check the id spelling first — then the token's scope (read) and that your \
         account accepted the repo's licence"
    } else {
        "check the id spelling first (a repo that does not exist answers 401 too); if \
         it IS gated, accept its licence on huggingface.co and export HF_TOKEN=<your \
         token> (Settings → Access Tokens · read scope)"
    };
    refuse(format!(
        "model pull: the Hub refuses {repo_id} ({status} — gated, private, or no such \
         repo)\n  fix: {fix}\n"
    ))
}

/// A network-level refusal — the resume note keeps an interrupted pull cheap.
fn transport_refusal(repo_id: &str, e: &nika_kernel::http::HttpError) -> Refusal {
    refuse(format!(
        "model pull: cannot reach the Hub for {repo_id} ({e})\n  fix: check the network — an \
         interrupted pull resumes from its .part\n"
    ))
}

/// `<dest>.part` — the in-flight download beside its final name.
fn part_path(dest: &Path) -> PathBuf {
    let mut os = dest.as_os_str().to_owned();
    os.push(".part");
    PathBuf::from(os)
}

/// `nika model pull <owner/repo[:QUANT]>` — the production blocking seam
/// (parse → tree → choose → confirm → download GGUF + tokenizer).
/// `Ok` is the receipt (or the operator's clean abort); `Err` is an
/// environment-class refusal the CLI maps to exit `3`.
///
/// # Errors
///
/// Every refusal teaches its fix: a malformed ref, a HOME-less machine,
/// an unreachable/gated/absent repo, a quant menu, the over-threshold
/// no-terminal confirm, or a broken/short transfer (the `.part` stays).
pub fn run(arg: &str, yes: bool) -> Result<String, Refusal> {
    let mref = store::parse_model_ref(arg)?;
    let root = store::models_root()?;
    pull_over_network(&mref, &root, arg, yes)
}

/// The full network flow (`Ok` = receipt/abort · `Err` = refusal).
fn pull_over_network(
    mref: &ModelRef,
    root: &Path,
    arg: &str,
    yes: bool,
) -> Result<String, Refusal> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            refuse(format!(
                "model pull: cannot start the fetch runtime ({e})\n"
            ))
        })?;
    let token = hf_token();
    let interactive = interactive_terminal();

    // Metadata first (sizes BEFORE any download) — default transport caps.
    let meta = Puller::new(house_http(None)?, root.to_path_buf(), token.clone(), false);
    let entries = runtime.block_on(meta.tree(mref))?;
    let entry = choose_gguf(&entries, mref)?.clone();
    let tokenizer = tokenizer_entry(&entries).cloned();
    let file = file_name(&entry.path).to_owned();
    let dest = mref.dir(root).join(&file);

    // The size prints BEFORE anything downloads (the issue's lock).
    eprintln!(
        "{} · {file} · {}",
        mref.repo_id(),
        store::human_size(entry.size)
    );
    let mut already = false;
    let mut resumed_from: u64 = 0;
    if std::fs::metadata(&dest).is_ok_and(|m| m.len() == entry.size) {
        already = true;
        eprintln!("  already present — nothing to download");
    } else {
        match confirm_gate(entry.size, yes, interactive) {
            ConfirmGate::Proceed => {}
            ConfirmGate::Prompt => {
                if !prompt_proceed(entry.size)? {
                    return Ok("aborted — nothing downloaded".to_owned());
                }
            }
            ConfirmGate::RefuseNeedsYes => {
                return Err(refuse(format!(
                    "model pull: {file} is {} — over the {} confirm threshold, and no \
                     terminal to ask\n  fix: re-run with --yes (the CI shape)\n",
                    store::human_size(entry.size),
                    store::human_size(CONFIRM_BYTES)
                )));
            }
        }
        let puller = Puller::new(
            house_http(Some(&entry))?,
            root.to_path_buf(),
            token.clone(),
            interactive,
        );
        let (_, from) = runtime.block_on(puller.download(mref, &entry))?;
        resumed_from = from;
    }
    let tokenizer_note = tokenizer_beside(&runtime, mref, root, token, tokenizer.as_ref())?;
    Ok(receipt(
        arg,
        mref,
        &dest,
        already,
        resumed_from,
        &tokenizer_note,
    ))
}

/// Land `tokenizer.json` beside the GGUF (the sibling layout `serve`
/// loads) and say what happened: the repo's own sidecar downloads
/// silently; a repo that ships none tries the declared base model
/// (the #518 universal friction — GGUF mirrors routinely ship no
/// tokenizer); one already beside stays silent (a note would
/// contradict the disk); every borrow miss degrades to the teaching
/// note.
fn tokenizer_beside(
    runtime: &tokio::runtime::Runtime,
    mref: &ModelRef,
    root: &Path,
    token: Option<String>,
    tokenizer: Option<&TreeEntry>,
) -> Result<String, Refusal> {
    match tokenizer {
        Some(tok) => {
            let tok_dest = mref.dir(root).join(file_name(&tok.path));
            if std::fs::metadata(&tok_dest).is_err() {
                let side = Puller::new(house_http(None)?, root.to_path_buf(), token, false);
                let _ = runtime.block_on(side.download(mref, tok))?;
            }
            Ok(String::new())
        }
        None if std::fs::metadata(mref.dir(root).join("tokenizer.json")).is_ok() => {
            Ok(String::new())
        }
        None => {
            let scout = Puller::new(house_http(None)?, root.to_path_buf(), token, false);
            Ok(
                match runtime.block_on(borrow_base_tokenizer(&scout, mref)) {
                    Some(base) => {
                        format!("\n  tokenizer.json borrowed from {base} (this repo ships none)")
                    }
                    None => "\n  note: the repo ships no tokenizer.json — `nika model serve` \
                         needs one beside the GGUF (name yours with --tokenizer <path>)"
                        .to_owned(),
                },
            )
        }
    }
}

/// The pull receipt: where it landed + the exact next commands. A
/// resumed transfer says so (`resumed at <offset>`) — the receipt is
/// the surface scripts and logs read, and a resume must not read like
/// a fresh pull (the live-Hub pin asserts this). The serve line speaks
/// per-family: promising `nika model serve` on a GGUF the v1 loader
/// refuses (llama · phi · …) would be a false receipt.
fn receipt(
    arg: &str,
    mref: &ModelRef,
    dest: &Path,
    already: bool,
    resumed_from: u64,
    tokenizer_note: &str,
) -> String {
    let verb = if already {
        "present".to_owned()
    } else if resumed_from > 0 {
        format!("pulled (resumed at {})", store::human_size(resumed_from))
    } else {
        "pulled".to_owned()
    };
    // Advisory sniff: an unknown family (None — exotic writer · future
    // header) keeps the promise; only a POSITIVE other-family read
    // rewords it (the loader still validates at load).
    let serve_line = match crate::gguf::sniff_architecture(dest) {
        Some(arch) if arch != crate::SERVE_FAMILY => format!(
            "serve:    `{arch}`-family GGUF — `nika model serve` is {}-only today; \
             point your local runner at the file above (ollama · llama.cpp · lmstudio)",
            crate::SERVE_FAMILY
        ),
        _ => format!("serve it: nika model serve --model {arg}"),
    };
    format!(
        "{verb} {}\n  {}\n  {serve_line}\n  manage:   nika \
         model list · nika model rm {arg}{tokenizer_note}",
        mref.repo_id(),
        dest.display()
    )
}

/// The house transport (`nika-http` — rustls · SSRF floor · manual
/// redirects): default caps for metadata, a body-sized cap for the GGUF
/// stream (a body larger than the DECLARED size is a tamper signal).
// `HttpConfig` is `#[non_exhaustive]` → field assignment, not a struct
// literal (the registry seam's exact idiom).
#[allow(clippy::field_reassign_with_default)]
fn house_http(stream_of: Option<&TreeEntry>) -> Result<nika_http::ReqwestHttp, Refusal> {
    let mut config = nika_http::HttpConfig::default();
    if let Some(entry) = stream_of {
        config.max_response_bytes = entry.size.saturating_add(64 * 1024 * 1024);
    }
    nika_http::ReqwestHttp::with_config(config).map_err(|e| {
        refuse(format!(
            "model pull: cannot initialize the fetch client ({e})\n"
        ))
    })
}

/// `HF_TOKEN` — the sanctioned env boundary (the `NIKA_MCP_TOKEN` seam):
/// operator config crossing into a client hold, read once, never printed.
#[allow(clippy::disallowed_methods)]
fn hf_token() -> Option<String> {
    std::env::var("HF_TOKEN").ok().filter(|t| !t.is_empty())
}

/// Both ends of a conversation present? (the prompt needs stdin AND a
/// visible question).
fn interactive_terminal() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Ask on stderr, read one line from stdin — `y`/`yes` proceeds.
fn prompt_proceed(size: u64) -> Result<bool, Refusal> {
    eprint!("  download {}? [y/N] ", store::human_size(size));
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).map_err(|e| {
        refuse(format!(
            "model pull: cannot read the confirmation ({e})\n  fix: re-run with --yes\n"
        ))
    })?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

/// A teaching refusal — the CLI adapter maps it to the environment
/// exit class (`3`).
pub(crate) type Refusal = String;

/// The semantic marker: every `refuse(...)` site is a refusal, never a
/// receipt (the identity keeps the sites greppable).
fn refuse(text: String) -> Refusal {
    text
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::task::{Context, Poll};

    use bytes::Bytes;
    use futures_core::Stream;
    use nika_kernel::http::{HttpError, HttpResponse, HttpStreamResponse};

    use super::*;

    // -- the tokenizer borrow (base_model fallback) ----------------------

    /// The basename law holds on remote tree paths: `/` splits, a
    /// backslash basename (the Windows `Path::join` separator — the
    /// mirror-controlled traversal edge) rejects to a fixed name, and
    /// `..` never reaches a join.
    #[test]
    fn file_name_takes_the_basename_and_rejects_traversal_shapes() {
        assert_eq!(file_name("sub/dir/w.gguf"), "w.gguf");
        assert_eq!(file_name("w.gguf"), "w.gguf");
        assert_eq!(file_name("x\\..\\evil.gguf"), "rejected-remote-name");
        assert_eq!(file_name("a/.."), "rejected-remote-name");
        assert_eq!(file_name("tokenizer.json"), "tokenizer.json");
    }

    #[test]
    fn valid_repo_id_admits_hub_ids_and_drops_crafted_ones() {
        assert!(valid_repo_id("Qwen/Qwen3-0.6B"));
        assert!(valid_repo_id("HuggingFaceTB/SmolLM2-135M-Instruct"));
        for bad in [
            "no-slash",
            "a/b/c",
            "a//",
            "/b",
            "../etc/passwd",
            "a/../b",
            "a/b?x=1",
            "a/b c",
            ".git/config",
        ] {
            assert!(!valid_repo_id(bad), "{bad} must not pass");
        }
    }

    /// The whole borrow: info names the base · the base's tree ships a
    /// tokenizer · it lands BESIDE the GGUF (never in the base's dir).
    #[tokio::test]
    async fn borrow_lands_the_base_tokenizer_beside_the_gguf() {
        let http = StreamHttp::default()
            .get_ok(200, r#"{"cardData":{"base_model":"Qwen/Qwen3-0.6B"}}"#)
            .get_ok(200, r#"[{"type":"file","size":9,"path":"tokenizer.json"}]"#)
            .stream_ok(200, Some(9), &[b"{\"tok\":1}"]);
        let root = temp_root("borrow-base");
        let scout = Puller::new(http, root.clone(), None, false);
        let gguf_repo = mref("Qwen/Qwen3-0.6B-GGUF");
        let base = borrow_base_tokenizer(&scout, &gguf_repo).await;
        assert_eq!(base.as_deref(), Some("Qwen/Qwen3-0.6B"));
        let beside = gguf_repo.dir(&root).join("tokenizer.json");
        assert_eq!(std::fs::read(&beside).expect("beside"), b"{\"tok\":1}");
        assert!(
            !root.join("Qwen").join("Qwen3-0.6B").exists(),
            "nothing lands in the base repo's dir"
        );
        let sent = scout.http.sent();
        assert!(
            sent[0].url.ends_with("/api/models/Qwen/Qwen3-0.6B-GGUF"),
            "{}",
            sent[0].url
        );
        assert!(
            sent[1]
                .url
                .contains("/api/models/Qwen/Qwen3-0.6B/tree/main"),
            "{}",
            sent[1].url
        );
        assert!(
            sent[2]
                .url
                .contains("/Qwen/Qwen3-0.6B/resolve/main/tokenizer.json"),
            "the bytes come FROM the base repo: {}",
            sent[2].url
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// Advisory to the bone: a card without `base_model` answers `None`.
    #[tokio::test]
    async fn borrow_degrades_to_none_without_a_base_model() {
        let http = StreamHttp::default().get_ok(200, r#"{"cardData":{}}"#);
        let root = temp_root("borrow-none");
        let scout = Puller::new(http, root.clone(), None, false);
        assert_eq!(borrow_base_tokenizer(&scout, &mref("u/m")).await, None);
        let _ = std::fs::remove_dir_all(root);
    }

    /// The receipt's serve line speaks per-family: a llama GGUF at the
    /// destination stops the `serve it:` promise (the v1 loader would
    /// refuse it) and points at a local runner instead; an absent or
    /// unsniffable file keeps the promise (advisory only).
    #[test]
    fn receipt_serve_line_speaks_per_family() {
        let root = temp_root("receipt-family");
        let mr = mref("u/m");
        let dir = mr.dir(&root);
        std::fs::create_dir_all(&dir).expect("dir");
        let dest = dir.join("w.gguf");
        // A minimal GGUF v3 header declaring general.architecture=llama.
        let mut b = 0x4655_4747u32.to_le_bytes().to_vec();
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&0u64.to_le_bytes());
        b.extend_from_slice(&1u64.to_le_bytes());
        let key = b"general.architecture";
        b.extend_from_slice(&(key.len() as u64).to_le_bytes());
        b.extend_from_slice(key);
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(&5u64.to_le_bytes());
        b.extend_from_slice(b"llama");
        std::fs::write(&dest, &b).expect("gguf fixture");

        let text = receipt("u/m", &mr, &dest, false, 0, "");
        assert!(text.contains("`llama`-family GGUF"), "family note: {text}");
        assert!(
            !text.contains("serve it: nika model serve"),
            "no false promise: {text}"
        );
        assert!(text.contains("nika model rm u/m"), "manage stays: {text}");

        // Unsniffable (not a GGUF) → advisory keeps the promise.
        std::fs::write(&dest, b"not a gguf").expect("overwrite");
        let text = receipt("u/m", &mr, &dest, false, 0, "");
        assert!(
            text.contains("serve it: nika model serve --model u/m"),
            "promise kept on unknown: {text}"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    fn entry(kind: &str, path: &str, size: u64) -> TreeEntry {
        TreeEntry {
            kind: kind.to_owned(),
            path: path.to_owned(),
            size,
        }
    }

    fn mref(arg: &str) -> ModelRef {
        store::parse_model_ref(arg).expect("test ref parses")
    }

    // The model-serve tests' exact fixture idiom.
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
        let dir = base.join(format!("nika-pull-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    // -- the injectable seam double (GET queue + streaming queue) -----

    /// A canned streaming body.
    struct ChunkStream(VecDeque<Result<Bytes, HttpError>>);

    impl Stream for ChunkStream {
        type Item = Result<Bytes, HttpError>;
        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.0.pop_front())
        }
    }

    /// One canned `send_streaming` answer.
    struct CannedStream {
        status: u16,
        content_length: Option<u64>,
        chunks: Vec<Bytes>,
    }

    /// The house-seam double: `get` answers from one queue,
    /// `send_streaming` from another; every request is recorded (the
    /// `MockHttp` idiom — that mock does not stream, hence this one).
    #[derive(Default)]
    struct StreamHttp {
        gets: Mutex<VecDeque<Result<HttpResponse, HttpError>>>,
        streams: Mutex<VecDeque<CannedStream>>,
        requests: Mutex<Vec<HttpRequest>>,
    }

    impl StreamHttp {
        fn get_ok(self, status: u16, body: &str) -> Self {
            self.gets
                .lock()
                .expect("gets")
                .push_back(Ok(HttpResponse::new(
                    status,
                    BTreeMap::new(),
                    Bytes::copy_from_slice(body.as_bytes()),
                    String::new(),
                )));
            self
        }

        fn stream_ok(self, status: u16, content_length: Option<u64>, chunks: &[&[u8]]) -> Self {
            self.streams
                .lock()
                .expect("streams")
                .push_back(CannedStream {
                    status,
                    content_length,
                    chunks: chunks.iter().map(|c| Bytes::copy_from_slice(c)).collect(),
                });
            self
        }

        fn sent(&self) -> Vec<HttpRequest> {
            self.requests.lock().expect("requests").clone()
        }
    }

    impl HttpGetDyn for StreamHttp {
        async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.requests.lock().expect("requests").push(request);
            self.gets
                .lock()
                .expect("gets")
                .pop_front()
                .unwrap_or_else(|| {
                    Err(HttpError::Other {
                        reason: "StreamHttp: get queue exhausted".into(),
                    })
                })
        }
    }

    impl HttpPostDyn for StreamHttp {
        async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
            self.requests.lock().expect("requests").push(request);
            Err(HttpError::Other {
                reason: "StreamHttp: pull never POSTs".into(),
            })
        }

        async fn send_streaming(
            &self,
            request: HttpRequest,
        ) -> Result<HttpStreamResponse, HttpError> {
            self.requests.lock().expect("requests").push(request);
            let canned = self
                .streams
                .lock()
                .expect("streams")
                .pop_front()
                .ok_or_else(|| HttpError::Other {
                    reason: "StreamHttp: stream queue exhausted".into(),
                })?;
            let chunks: VecDeque<Result<Bytes, HttpError>> =
                canned.chunks.into_iter().map(Ok).collect();
            Ok(HttpStreamResponse::new(
                canned.status,
                BTreeMap::new(),
                String::new(),
                canned.content_length,
                Box::pin(ChunkStream(chunks)),
            ))
        }
    }

    // -- tree parse + choice -------------------------------------------

    #[test]
    fn parse_tree_keeps_files_drops_directories() {
        let body = br#"[
            {"type":"file","oid":"a","size":9,"path":"model-q4_k_m.gguf"},
            {"type":"directory","oid":"b","size":0,"path":"assets"},
            {"type":"file","oid":"c","size":3,"path":"tokenizer.json"}
        ]"#;
        let entries = parse_tree(body, "u/m").expect("parses");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "model-q4_k_m.gguf");
        assert_eq!(entries[0].size, 9);
    }

    #[test]
    fn parse_tree_refuses_non_json_with_the_repo_named() {
        let refusal = parse_tree(b"<html>rate limited</html>", "u/m").expect_err("refuses");
        assert!(refusal.contains("u/m"), "{refusal}");
    }

    #[test]
    fn choose_prefers_the_default_quant_then_the_single_gguf() {
        let entries = [
            entry("file", "model-Q4_K_M.gguf", 100),
            entry("file", "model-Q8_0.gguf", 200),
            entry("file", "tokenizer.json", 3),
        ];
        let chosen = choose_gguf(&entries, &mref("u/m")).expect("default quant");
        assert_eq!(chosen.path, "model-Q4_K_M.gguf");

        let single = [
            entry("file", "weights-F16.gguf", 50),
            entry("file", "README.md", 1),
        ];
        let chosen = choose_gguf(&single, &mref("u/m")).expect("single gguf");
        assert_eq!(chosen.path, "weights-F16.gguf");
    }

    #[test]
    fn choose_honors_an_explicit_tag_case_insensitively() {
        let entries = [
            entry("file", "model-Q4_K_M.gguf", 100),
            entry("file", "model-Q8_0.gguf", 200),
        ];
        let chosen = choose_gguf(&entries, &mref("u/m:q8_0")).expect("tag matches");
        assert_eq!(chosen.path, "model-Q8_0.gguf");
    }

    #[test]
    fn choose_refuses_listing_the_quants_that_exist() {
        // Several GGUFs, none the default → the refusal lists the menu.
        let entries = [
            entry("file", "model-Q5_K_M.gguf", 100),
            entry("file", "model-Q8_0.gguf", 200),
        ];
        let refusal = choose_gguf(&entries, &mref("u/m")).expect_err("ambiguous");
        assert!(refusal.contains("Q5_K_M"), "{refusal}");
        assert!(refusal.contains("Q8_0"), "{refusal}");
        assert!(refusal.contains(":Q5_K_M"), "teach the tag form: {refusal}");

        // An explicit tag that matches nothing → same teaching shape.
        let refusal = choose_gguf(&entries, &mref("u/m:Q2_K")).expect_err("no such tag");
        assert!(refusal.contains("Q2_K"), "{refusal}");
        assert!(refusal.contains("Q5_K_M"), "{refusal}");

        // No GGUF at all → say so (this repo is not a GGUF repo).
        let none = [entry("file", "model.safetensors", 100)];
        let refusal = choose_gguf(&none, &mref("u/m")).expect_err("no gguf");
        assert!(refusal.contains("no GGUF"), "{refusal}");
    }

    #[test]
    fn tokenizer_rides_along_when_the_repo_ships_one() {
        let entries = [
            entry("file", "model-Q4_K_M.gguf", 100),
            entry("file", "tokenizer.json", 3),
        ];
        assert_eq!(
            tokenizer_entry(&entries).map(|e| e.path.as_str()),
            Some("tokenizer.json")
        );
        assert!(tokenizer_entry(&entries[..1]).is_none());
    }

    // -- the confirm gate (pure) ---------------------------------------

    #[test]
    fn confirm_gate_prompts_big_sizes_and_refuses_them_in_pipes() {
        assert_eq!(
            confirm_gate(CONFIRM_BYTES - 1, false, false),
            ConfirmGate::Proceed
        );
        assert_eq!(
            confirm_gate(CONFIRM_BYTES, true, false),
            ConfirmGate::Proceed
        );
        assert_eq!(
            confirm_gate(CONFIRM_BYTES, false, true),
            ConfirmGate::Prompt
        );
        assert_eq!(
            confirm_gate(CONFIRM_BYTES, false, false),
            ConfirmGate::RefuseNeedsYes
        );
    }

    // -- the download path (over the injected seam) ---------------------

    #[tokio::test]
    async fn tree_hits_the_hub_api_with_the_bearer_when_a_token_rides() {
        let http =
            StreamHttp::default().get_ok(200, r#"[{"type":"file","size":1,"path":"a.gguf"}]"#);
        let root = temp_root("tree-auth");
        let puller = Puller::new(http, root.clone(), Some("hf_secret".to_owned()), false);
        let entries = puller.tree(&mref("u/m")).await.expect("tree resolves");
        assert_eq!(entries.len(), 1);
        let sent = puller.http.sent();
        assert_eq!(sent.len(), 1);
        assert!(
            sent[0]
                .url
                .contains("huggingface.co/api/models/u/m/tree/main"),
            "{}",
            sent[0].url
        );
        assert_eq!(
            sent[0].headers.get("authorization").map(String::as_str),
            Some("Bearer hf_secret")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn tree_translates_401_into_the_hf_token_teach() {
        let http = StreamHttp::default().get_ok(401, r#"{"error":"gated"}"#);
        let root = temp_root("tree-gated");
        let puller = Puller::new(http, root.clone(), None, false);
        let refusal = puller.tree(&mref("u/m")).await.expect_err("gated refuses");
        assert!(refusal.contains("HF_TOKEN"), "{refusal}");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn download_streams_into_the_canonical_dir_and_renames() {
        let http = StreamHttp::default().stream_ok(200, Some(9), &[b"hello ", b"wor", b""]);
        let root = temp_root("dl-happy");
        let puller = Puller::new(http, root.clone(), None, false);
        let (dest, resumed_from) = puller
            .download(&mref("u/m"), &entry("file", "w-q4_k_m.gguf", 9))
            .await
            .expect("download completes");
        assert_eq!(resumed_from, 0, "a fresh pull reports no resume offset");
        assert_eq!(dest, root.join("u").join("m").join("w-q4_k_m.gguf"));
        assert_eq!(std::fs::read(&dest).expect("dest"), b"hello wor");
        assert!(
            !root.join("u").join("m").join("w-q4_k_m.gguf.part").exists(),
            "the .part renamed into place"
        );
        // No token → no authorization header leaves this machine.
        let sent = puller.http.sent();
        assert!(!sent[0].headers.contains_key("authorization"));
        assert!(
            sent[0]
                .url
                .contains("huggingface.co/u/m/resolve/main/w-q4_k_m.gguf"),
            "{}",
            sent[0].url
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn download_resumes_a_part_file_with_a_range_request() {
        let http = StreamHttp::default().stream_ok(206, Some(5), &[b"world"]);
        let root = temp_root("dl-resume");
        let dir = root.join("u").join("m");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("w.gguf.part"), b"hello").expect("part fixture");
        let puller = Puller::new(http, root.clone(), None, false);
        let (dest, resumed_from) = puller
            .download(&mref("u/m"), &entry("file", "w.gguf", 10))
            .await
            .expect("resume completes");
        assert_eq!(resumed_from, 5, "the 206 resume reports its offset");
        assert_eq!(std::fs::read(&dest).expect("dest"), b"helloworld");
        assert!(!dir.join("w.gguf.part").exists(), ".part renamed away");
        let sent = puller.http.sent();
        assert_eq!(
            sent[0].headers.get("range").map(String::as_str),
            Some("bytes=5-"),
            "the resume rides a Range header"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn download_restarts_when_the_server_answers_200_to_a_resume() {
        let http = StreamHttp::default().stream_ok(200, Some(5), &[b"fresh"]);
        let root = temp_root("dl-restart");
        let dir = root.join("u").join("m");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("w.gguf.part"), b"stale-part").expect("part fixture");
        let puller = Puller::new(http, root.clone(), None, false);
        let (dest, resumed_from) = puller
            .download(&mref("u/m"), &entry("file", "w.gguf", 5))
            .await
            .expect("restart completes");
        assert_eq!(resumed_from, 0, "a 200 restart is not a resume");
        assert_eq!(std::fs::read(&dest).expect("dest"), b"fresh");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn download_keeps_the_part_and_teaches_resume_on_a_short_stream() {
        let http = StreamHttp::default().stream_ok(200, Some(10), &[b"only4"]);
        let root = temp_root("dl-short");
        let puller = Puller::new(http, root.clone(), None, false);
        let refusal = puller
            .download(&mref("u/m"), &entry("file", "w.gguf", 10))
            .await
            .expect_err("short stream refuses");
        assert!(refusal.contains("re-run"), "{refusal}");
        assert!(
            root.join("u").join("m").join("w.gguf.part").exists(),
            "the .part STAYS for the resume"
        );
        assert!(!root.join("u").join("m").join("w.gguf").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn download_translates_403_into_the_gated_teach() {
        let http = StreamHttp::default().stream_ok(403, None, &[]);
        let root = temp_root("dl-gated");
        let puller = Puller::new(http, root.clone(), None, false);
        let refusal = puller
            .download(&mref("u/m"), &entry("file", "w.gguf", 5))
            .await
            .expect_err("403 refuses");
        assert!(refusal.contains("HF_TOKEN"), "{refusal}");
        let _ = std::fs::remove_dir_all(root);
    }
}
