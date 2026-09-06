// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `ContextEnvelope` — ONE resolution of « where is this session »
//! (mega-plan §3 ownership: L4 `nika-cli` owns the resolve; the hook
//! surface — `.agents/plugins/nika/scripts/session-context.sh` — carries
//! the same law on the shell side, fixed by P0-14). The law:
//!
//! - the payload/handler cwd is the ONLY folder evidence — the process
//!   cwd is NEVER evidence of a project; absent · invalid · unreachable
//!   → CHAT-ONLY, no detection, no fallback;
//! - a candidate sitting in a SUBDIR of a repo resolves UP to the git
//!   root, and the expansion is always traced in the evidence (never a
//!   silent rewrite);
//! - multi-root never picks root 0 implicitly — the envelope withholds
//!   `canonical_root` until [`ContextEnvelope::select_root`] names one;
//! - the nonce binds consent + inventory to the root they were granted
//!   for: root change → nonce change → re-consent;
//! - package/git/workflow roots and the execution cwd are DISTINCT
//!   fields — consumers never conflate them;
//! - ssh · container · worktree · read-only are represented, not guessed.
//!
//! [`resolve`] is pure: the impure half (env vars, host facts) arrives
//! as [`EnvFacts`], collected by the caller (or [`EnvFacts::detect`]).
//!
//! WIRED (W2): [`crate::welcome`] is the first consumer. The items the
//! doctor + hook waves will eat next carry their own scoped allowances
//! below — the module-level blanket is gone.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The session's mode — the ONE decision every consumer branches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContextMode {
    /// No reliable folder evidence: chat only, zero workspace claim.
    ChatOnly,
    /// A validated folder binds the session.
    Workspace,
}

/// Where the folder evidence came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceSource {
    /// The host's session-open folder selection.
    HostSelection,
    /// The directory of the host's active file.
    ActiveFile,
    /// An explicit cwd (CLI flag · hook payload field).
    ExplicitCwd,
    /// No folder was handed at all.
    #[default]
    None,
}

/// The evidence record: the source PLUS any subdir→root expansion (an
/// expansion is always displayed, never silent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Evidence {
    /// How the candidate cwd reached us.
    pub(crate) source: EvidenceSource,
    /// Set when the candidate sat in a SUBDIR of the resolved root —
    /// carries the original subdir so the expansion stays traceable.
    pub(crate) expanded_from: Option<PathBuf>,
}

/// Host environment facts — the impure half, collected once by the
/// caller so [`resolve`] stays pure + testable.
#[derive(Debug, Clone, Default)]
pub(crate) struct EnvFacts {
    /// How the candidate cwd was obtained.
    pub(crate) evidence: EvidenceSource,
    /// Every root the host opened (0/1 = single-root · >1 = multi-root,
    /// where NO root is ever chosen implicitly).
    pub(crate) workspace_roots: Vec<PathBuf>,
    /// SSH/remote session identity (a label, e.g. `"ssh"` — never the
    /// raw connection string, which leaks into transcripts).
    pub(crate) remote_identity: Option<String>,
    /// Container identity (e.g. `"docker"`).
    pub(crate) container_identity: Option<String>,
    /// Writability override (known-read-only mounts · the test seam) —
    /// `None` = probe the root's permission bits.
    pub(crate) writable: Option<bool>,
    /// Whether the workspace inventory scan completed (a truncated walk
    /// is never « complete » — P0-4's law rides here too).
    pub(crate) inventory_complete: bool,
}

impl EnvFacts {
    /// Detect the host facts from the environment (remote · container).
    /// The evidence source + roots stay the CALLER's knowledge — only
    /// the host knows how it named the folder.
    #[allow(clippy::disallowed_methods)] // presence-only reads of NON-secret session vars
    #[must_use]
    pub(crate) fn detect() -> Self {
        let remote = std::env::var_os("SSH_CONNECTION")
            .or_else(|| std::env::var_os("SSH_TTY"))
            .map(|_| "ssh".to_owned());
        let container = if Path::new("/.dockerenv").exists() {
            Some("docker".to_owned())
        } else {
            std::env::var("container").ok()
        };
        Self {
            remote_identity: remote,
            container_identity: container,
            ..Self::default()
        }
    }
}

/// The resolved context — serializable (the hook/envelope wire surface).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ContextEnvelope {
    /// Chat-only vs workspace — the one branch.
    pub(crate) mode: ContextMode,
    /// The candidate as the host named it (render form · empty in
    /// chat-only-without-candidate).
    pub(crate) display_path: String,
    /// The resolved root, canonicalized — `None` in chat-only AND in
    /// multi-root before [`Self::select_root`] names one explicitly.
    pub(crate) canonical_root: Option<PathBuf>,
    /// The evidence record (source + expansion trace).
    pub(crate) evidence: Evidence,
    /// The folder the host named, validated + canonicalized (may be a
    /// subdir of the repo — distinct from the git root BY DESIGN).
    /// Empty in chat-only.
    pub(crate) project_root: PathBuf,
    /// The git toplevel when the candidate sits inside a repo.
    pub(crate) git_root: Option<PathBuf>,
    /// The nearest ancestor carrying a `.nika/` store (bounded at the
    /// git root), when one exists.
    pub(crate) workflow_root: Option<PathBuf>,
    /// The dir executions run in — the validated candidate, NEVER the
    /// process cwd (`None` in chat-only).
    pub(crate) execution_cwd: Option<PathBuf>,
    /// Every root the host opened, canonicalized (len > 1 = multi-root).
    pub(crate) workspace_roots: Vec<PathBuf>,
    /// SSH/remote session identity, when detected.
    pub(crate) remote_identity: Option<String>,
    /// Container identity, when detected.
    pub(crate) container_identity: Option<String>,
    /// The gitdir target when `.git` is a FILE (worktree · submodule).
    pub(crate) worktree_identity: Option<String>,
    /// Whether the resolved root is writable (read-only mounts say no).
    pub(crate) writable: bool,
    /// Whether the workspace inventory completed — void the moment the
    /// nonce changes.
    pub(crate) inventory_complete: bool,
    /// Stable hash of the root — consent + inventory granted under one
    /// nonce are VOID under another (root change = re-consent).
    pub(crate) nonce: String,
}

impl ContextEnvelope {
    /// True when the host opened several roots and none was named yet:
    /// the consumer MUST ask (root 0 is never the silent pick).
    ///
    /// Consumed by the experience router: `welcome`'s experience block
    /// maps a `true` here to `MultiRootUnselected`, which routes to the
    /// `select_root` action. (Not a doc link — the consumer is a
    /// private fn, and rustdoc cannot resolve one across modules.)
    #[must_use]
    pub(crate) fn requires_explicit_root(&self) -> bool {
        self.mode == ContextMode::Workspace
            && self.workspace_roots.len() > 1
            && self.canonical_root.is_none()
    }

    /// Name the root explicitly (the multi-root door). Refuses a root
    /// the host never opened; on accept, re-binds the nonce.
    ///
    /// The ASK half of the door now ships (see
    /// [`Self::requires_explicit_root`]); this is the ANSWER half, and
    /// it stays parked until a surface can carry the person's pick —
    /// `welcome` takes no root argument, so wiring it here would be a
    /// silent choice, which is the one thing the door forbids. Tests
    /// pin the behaviour meanwhile.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "the ANSWER half of the multi-root door — the ask half ships in \
                      the router; no surface carries a root argument yet, and wiring \
                      it without one would make the silent pick the door forbids"
        )
    )]
    pub(crate) fn select_root(&mut self, root: &Path) -> bool {
        let Ok(canon) = root.canonicalize() else {
            return false;
        };
        if !self.workspace_roots.contains(&canon) {
            return false;
        }
        self.canonical_root = Some(canon);
        self.nonce = nonce_for(self.canonical_root.as_ref(), &self.workspace_roots);
        true
    }
}

/// Resolve the session context from the candidate cwd + host facts.
/// Pure: every impure input arrives as a parameter.
#[must_use]
pub(crate) fn resolve(candidate: Option<&Path>, facts: &EnvFacts) -> ContextEnvelope {
    // The chat-only law (P0-14 · handoff §6.2): absent · invalid ·
    // unreachable → chat_only. The process cwd is NEVER consulted.
    let Some(candidate) = candidate else {
        return chat_only(String::new(), facts);
    };
    let display = candidate.display().to_string();
    let Ok(canon) = candidate.canonicalize() else {
        return chat_only(display, facts);
    };
    if !canon.is_dir() {
        return chat_only(display, facts);
    }

    let (git_root, worktree_identity) =
        find_git_root(&canon).map_or((None, None), |(root, gitdir)| (Some(root), gitdir));
    let workflow_root = find_workflow_root(&canon, git_root.as_deref());
    // A subdir of a repo resolves UP to the git root; the expansion is
    // traced in the evidence, never rewritten silently.
    let expanded_from = match &git_root {
        Some(root) if *root != canon => Some(canon.clone()),
        _ => None,
    };
    let mut roots = normalize_roots(&facts.workspace_roots);
    let multi_root = roots.len() > 1;
    // Multi-root: NO implicit choice — canonical_root stays None until
    // select_root names one. Single-root: the resolved root binds.
    let canonical_root = if multi_root {
        None
    } else {
        let resolved = git_root.clone().unwrap_or_else(|| canon.clone());
        if roots.is_empty() {
            roots.push(resolved.clone());
        }
        Some(resolved)
    };
    let writable = facts
        .writable
        .unwrap_or_else(|| probe_writable(git_root.as_ref().unwrap_or(&canon)));
    ContextEnvelope {
        mode: ContextMode::Workspace,
        display_path: display,
        nonce: nonce_for(canonical_root.as_ref(), &roots),
        canonical_root,
        evidence: Evidence {
            source: facts.evidence,
            expanded_from,
        },
        project_root: canon.clone(),
        git_root,
        workflow_root,
        execution_cwd: Some(canon),
        workspace_roots: roots,
        remote_identity: facts.remote_identity.clone(),
        container_identity: facts.container_identity.clone(),
        worktree_identity,
        writable,
        inventory_complete: facts.inventory_complete,
    }
}

/// The chat-only envelope: zero workspace claim, whatever the host
/// handed. `display` keeps the rejected candidate visible (a claim
/// names its proof, even a refused one).
fn chat_only(display: String, facts: &EnvFacts) -> ContextEnvelope {
    ContextEnvelope {
        mode: ContextMode::ChatOnly,
        display_path: display,
        canonical_root: None,
        evidence: Evidence {
            source: EvidenceSource::None,
            expanded_from: None,
        },
        project_root: PathBuf::new(),
        git_root: None,
        workflow_root: None,
        execution_cwd: None,
        workspace_roots: normalize_roots(&facts.workspace_roots),
        remote_identity: facts.remote_identity.clone(),
        container_identity: facts.container_identity.clone(),
        worktree_identity: None,
        writable: false,
        inventory_complete: false,
        nonce: nonce_for(None, &facts.workspace_roots),
    }
}

/// Canonicalize the host's roots, dropping the dead ones (a root that
/// does not resolve is no root).
fn normalize_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .filter_map(|r| r.canonicalize().ok().filter(|c| c.is_dir()))
        .collect()
}

/// Walk the ancestors for `.git` — a DIR is a plain repo, a FILE is a
/// worktree/submodule (`gitdir: <path>`); the gitdir target rides as
/// the worktree identity. No shell out: the walk is the truth.
#[must_use]
pub fn find_git_root(start: &Path) -> Option<(PathBuf, Option<String>)> {
    for ancestor in start.ancestors() {
        let dot = ancestor.join(".git");
        if dot.is_dir() {
            return Some((ancestor.to_path_buf(), None));
        }
        if dot.is_file() {
            let gitdir = std::fs::read_to_string(&dot).ok().and_then(|content| {
                content
                    .lines()
                    .next()
                    .and_then(|line| line.strip_prefix("gitdir:"))
                    .map(|target| target.trim().to_owned())
            });
            return Some((ancestor.to_path_buf(), gitdir));
        }
    }
    None
}

/// The nearest ancestor carrying a `.nika/` store — bounded at the git
/// root (the store lives INSIDE the repo, never above it).
fn find_workflow_root(start: &Path, git_root: Option<&Path>) -> Option<PathBuf> {
    for ancestor in start.ancestors() {
        if ancestor.join(".nika").is_dir() {
            return Some(ancestor.to_path_buf());
        }
        if Some(ancestor) == git_root {
            return None;
        }
    }
    None
}

/// Writable = the permission bits say so (no probe writes on a mirror).
fn probe_writable(root: &Path) -> bool {
    std::fs::metadata(root)
        .map(|m| !m.permissions().readonly())
        .unwrap_or(false)
}

/// The nonce — FNV-1a 64 over the canonical root (or, multi-root
/// unresolved, over the sorted roots set): stable for one root, void
/// the moment the root changes. Not cryptographic — a BINDING, not a
/// secret.
fn nonce_for(canonical_root: Option<&PathBuf>, workspace_roots: &[PathBuf]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    if let Some(root) = canonical_root {
        feed(root.as_os_str().as_encoded_bytes());
    } else {
        let mut sorted: Vec<&PathBuf> = workspace_roots.iter().collect();
        sorted.sort();
        for root in sorted {
            feed(root.as_os_str().as_encoded_bytes());
            feed(b"\n");
        }
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch dir per test (pid × tag), cleaned by the test.
    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nika-ctx-env-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    fn canon(p: &Path) -> PathBuf {
        p.canonicalize().expect("canonicalize")
    }

    #[test]
    fn chat_only_without_candidate() {
        let env = resolve(None, &EnvFacts::default());
        assert_eq!(env.mode, ContextMode::ChatOnly);
        assert_eq!(env.canonical_root, None);
        assert_eq!(env.execution_cwd, None, "never the process cwd");
        assert_eq!(env.evidence.source, EvidenceSource::None);
        assert!(!env.writable);
    }

    #[test]
    fn chat_only_on_invalid_candidate() {
        let missing = Path::new("/definitely/not/a/real/dir/nika-ctx-env");
        let facts = EnvFacts {
            evidence: EvidenceSource::ExplicitCwd,
            ..EnvFacts::default()
        };
        let env = resolve(Some(missing), &facts);
        assert_eq!(env.mode, ContextMode::ChatOnly, "invalid → chat_only");
        assert_eq!(env.canonical_root, None);
        assert_eq!(env.execution_cwd, None);
        assert!(
            env.display_path.contains("nika-ctx-env"),
            "the rejected candidate stays visible"
        );
    }

    #[test]
    fn chat_only_on_file_candidate() {
        let dir = tmpdir("file");
        let file = dir.join("a.nika.yaml");
        std::fs::write(&file, "x").expect("write");
        let env = resolve(Some(&file), &EnvFacts::default());
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(env.mode, ContextMode::ChatOnly, "a file is no folder");
    }

    #[test]
    fn workspace_without_git() {
        let dir = tmpdir("plain");
        let facts = EnvFacts {
            evidence: EvidenceSource::HostSelection,
            ..EnvFacts::default()
        };
        let env = resolve(Some(&dir), &facts);
        let root = canon(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(env.mode, ContextMode::Workspace);
        assert_eq!(env.canonical_root, Some(root.clone()));
        assert_eq!(env.project_root, root.clone());
        assert_eq!(env.git_root, None);
        assert_eq!(env.execution_cwd, Some(root.clone()));
        assert_eq!(env.workspace_roots, vec![root]);
        assert_eq!(env.evidence.source, EvidenceSource::HostSelection);
        assert_eq!(env.evidence.expanded_from, None);
        assert!(env.writable, "a fresh tmp dir is writable");
        assert!(!env.nonce.is_empty());
    }

    #[test]
    fn subdir_expands_to_git_root_with_trace() {
        let dir = tmpdir("expand");
        std::fs::create_dir_all(dir.join(".git")).expect("mkdir .git");
        let deep = dir.join("crates").join("nika-cli");
        std::fs::create_dir_all(&deep).expect("mkdir deep");
        let facts = EnvFacts {
            evidence: EvidenceSource::ExplicitCwd,
            ..EnvFacts::default()
        };
        let env = resolve(Some(&deep), &facts);
        let root = canon(&dir);
        let deep_canon = canon(&deep);
        let display = deep.display().to_string();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(env.mode, ContextMode::Workspace);
        assert_eq!(env.git_root, Some(root.clone()));
        assert_eq!(env.canonical_root, Some(root), "resolves UP to the root");
        assert_eq!(
            env.project_root, deep_canon,
            "project root stays the named folder"
        );
        assert_eq!(
            env.evidence.expanded_from,
            Some(deep_canon),
            "the subdir→root expansion is traced"
        );
        assert_eq!(env.display_path, display, "display keeps the as-named path");
    }

    #[test]
    fn worktree_gitdir_file_is_identity() {
        let dir = tmpdir("worktree");
        std::fs::write(
            dir.join(".git"),
            "gitdir: /var/git/main/.git/worktrees/wt\n",
        )
        .expect("write .git");
        let env = resolve(Some(&dir), &EnvFacts::default());
        let root = canon(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(env.git_root, Some(root));
        assert_eq!(
            env.worktree_identity,
            Some("/var/git/main/.git/worktrees/wt".to_owned()),
            "the gitdir target is represented"
        );
    }

    #[test]
    fn read_only_root_says_not_writable() {
        let dir = tmpdir("ro");
        let facts = EnvFacts {
            writable: Some(false),
            ..EnvFacts::default()
        };
        let env = resolve(Some(&dir), &facts);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(env.mode, ContextMode::Workspace);
        assert!(!env.writable, "the read-only fact is honored");
    }

    #[test]
    fn multi_root_never_picks_implicitly() {
        let a = tmpdir("multi-a");
        let b = tmpdir("multi-b");
        let facts = EnvFacts {
            evidence: EvidenceSource::HostSelection,
            workspace_roots: vec![a.clone(), b.clone()],
            ..EnvFacts::default()
        };
        let mut env = resolve(Some(&a), &facts);
        let root_b = canon(&b);
        let nonce_before = env.nonce.clone();
        assert_eq!(env.mode, ContextMode::Workspace);
        assert_eq!(env.canonical_root, None, "root 0 is never the silent pick");
        assert!(env.requires_explicit_root());
        assert!(
            !env.select_root(Path::new("/definitely/not/opened")),
            "a root the host never opened is refused"
        );
        assert!(env.select_root(&b), "an opened root is accepted");
        assert_eq!(env.canonical_root, Some(root_b));
        assert!(!env.requires_explicit_root());
        assert_ne!(env.nonce, nonce_before, "the choice re-binds the nonce");
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
    }

    #[test]
    fn nonce_binds_the_root() {
        let a = tmpdir("nonce-a");
        let b = tmpdir("nonce-b");
        let env_a = resolve(Some(&a), &EnvFacts::default());
        let env_a2 = resolve(Some(&a), &EnvFacts::default());
        let env_b = resolve(Some(&b), &EnvFacts::default());
        std::fs::remove_dir_all(&a).ok();
        std::fs::remove_dir_all(&b).ok();
        assert_eq!(env_a.nonce, env_a2.nonce, "stable for one root");
        assert_ne!(env_a.nonce, env_b.nonce, "root change → nonce change");
    }

    #[test]
    fn workflow_root_is_the_nika_store_ancestor() {
        let dir = tmpdir("wfroot");
        std::fs::create_dir_all(dir.join(".git")).expect("mkdir .git");
        std::fs::create_dir_all(dir.join(".nika")).expect("mkdir .nika");
        let deep = dir.join("src");
        std::fs::create_dir_all(&deep).expect("mkdir src");
        let env = resolve(Some(&deep), &EnvFacts::default());
        let root = canon(&dir);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(env.workflow_root, Some(root));
    }

    #[test]
    fn detect_reads_only_presence() {
        let facts = EnvFacts::detect();
        assert_eq!(
            facts.evidence,
            EvidenceSource::None,
            "detect never invents evidence"
        );
        assert!(
            facts.workspace_roots.is_empty(),
            "roots stay the caller's knowledge"
        );
        assert!(
            !facts.inventory_complete,
            "no scan ran, nothing is complete"
        );
        // Coherence with the actual session vars, whatever they are.
        #[allow(clippy::disallowed_methods)] // presence-only, mirroring detect()
        let ssh =
            std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some();
        assert_eq!(facts.remote_identity.is_some(), ssh);
    }

    #[test]
    fn envelope_serializes_roundtrip() {
        let dir = tmpdir("serde");
        std::fs::create_dir_all(dir.join(".git")).expect("mkdir .git");
        let env = resolve(Some(&dir), &EnvFacts::default());
        std::fs::remove_dir_all(&dir).ok();
        let json = serde_json::to_string(&env).expect("serialize");
        let back: ContextEnvelope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(env, back);
        assert!(json.contains("\"mode\":\"workspace\""));
    }
}
