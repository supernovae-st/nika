// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The session runtime — the loop a terminal drives: one turn in, one
//! outcome out. A turn is a slash command, a Nika fact (answered from
//! the engine, zero tokens), or a free-text question the selected
//! intelligence reasons over through the broker's bundle and under the
//! guard's reading. No temporary workflow, no trace for a chat turn, no
//! hidden shell.

use std::path::Path;

use crate::broker::ContextBroker;
use crate::guard::KnownWorld;
use crate::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use crate::reasoner::{ReasonError, SessionReasoner};
use crate::snapshot::ProjectSnapshot;

/// The durable half of the conversation — decisions, not chat.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct IntentDraft {
    /// The goal, as first stated.
    pub goal: Option<String>,
    /// Decisions the human made in words.
    pub decisions: Vec<String>,
    /// Questions still open.
    pub unresolved: Vec<String>,
}

/// What one turn produced.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TurnOutcome {
    /// The intelligence's reply, read by the guard.
    Reply(String),
    /// A fact from the engine (no model asked).
    Facts(String),
    /// The help card.
    Help(String),
    /// The human closed the session.
    Quit,
    /// The turn was refused, with the reason and the fix.
    Refusal(String),
    /// The session asks the first screen again — the NEXT line is the
    /// answer ([`SessionRuntime::choose`]).
    Ask(String),
}

/// The help card — the few survivors, and the law that everything
/// meaningful is reachable in words.
pub const HELP: &str = "text                 ask, in words · these answer from the engine, no AI asked: your workflows · a file's verdict (« is X valid »)
                     · the builtins · the providers · an example or template for a job · a code (« explain NIKA-… »)
                     · what Nika calls a node, step, trigger, secret, action · the rest goes to your chosen intelligence
/intelligence        the AI this session reasons with · asks the first screen again, the next line is your answer
/help                this card
/quit                close the session
Name a workflow file in your question to let the session read it (only files under the root are ever read).";

/// How many recent turns ride the next prompt.
const RECENT_TURNS: usize = 8;

/// How a door builds the reasoner for a resolved choice.
pub type ReasonerFactory = Box<dyn Fn(&ResolvedSessionIntelligence) -> Box<dyn SessionReasoner>>;

/// The session over one project, one intelligence, one reasoner.
pub struct SessionRuntime {
    /// The project as observed at open.
    pub snapshot: ProjectSnapshot,
    /// The intelligence the human chose, judged against this machine.
    pub intelligence: ResolvedSessionIntelligence,
    /// The durable intent.
    pub intent: IntentDraft,
    reasoner: Box<dyn SessionReasoner>,
    broker: ContextBroker,
    known: KnownWorld,
    recent: Vec<(String, String)>,
    census: Option<IntelligenceCensus>,
    home: Option<std::path::PathBuf>,
    factory: Option<ReasonerFactory>,
}

impl std::fmt::Debug for SessionRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionRuntime")
            .field("root", &self.snapshot.root)
            .field("intelligence", &self.intelligence.kind)
            .field("reasoner", &self.reasoner.name())
            .finish_non_exhaustive()
    }
}

impl SessionRuntime {
    /// Open a session in `cwd` with the chosen intelligence and its reasoner.
    #[must_use]
    pub fn open(
        cwd: &Path,
        intelligence: ResolvedSessionIntelligence,
        reasoner: Box<dyn SessionReasoner>,
    ) -> Self {
        let snapshot = ProjectSnapshot::observe(cwd);
        let broker = ContextBroker::new(snapshot.root.clone());
        let known = KnownWorld::installed(&snapshot.root);
        Self {
            snapshot,
            intelligence,
            intent: IntentDraft {
                goal: None,
                decisions: Vec::new(),
                unresolved: Vec::new(),
            },
            reasoner,
            broker,
            known,
            recent: Vec::new(),
            census: None,
            home: None,
            factory: None,
        }
    }

    /// Open a session that can re-choose its intelligence in-session:
    /// the census it judges against, the home the choice is kept under,
    /// and the door's reasoner factory.
    #[must_use]
    pub fn open_with(
        cwd: &Path,
        census: IntelligenceCensus,
        pref: &UserIntelligencePreference,
        home: Option<&Path>,
        factory: ReasonerFactory,
    ) -> Self {
        let intelligence = ResolvedSessionIntelligence::resolve(pref, &census);
        let reasoner = factory(&intelligence);
        let mut session = Self::open(cwd, intelligence, reasoner);
        session.census = Some(census);
        session.home = home.map(Path::to_path_buf);
        session.factory = Some(factory);
        session
    }

    /// The answer to the first screen asked in-session (`/intelligence`):
    /// the choice is judged, kept under the home when one exists, and the
    /// reasoner rebuilt — refused with its fix when this machine cannot
    /// serve it, and the previous choice stands.
    pub fn choose(&mut self, answer: &str) -> TurnOutcome {
        let (Some(census), Some(factory)) = (&self.census, &self.factory) else {
            return TurnOutcome::Refusal(
                "this session cannot re-choose its intelligence — quit and open `nika` again"
                    .to_owned(),
            );
        };
        let pref = match census.choose(answer) {
            Ok(pref) => pref,
            Err(why) => return TurnOutcome::Refusal(format!("{why} · the previous choice stands")),
        };
        let resolved = ResolvedSessionIntelligence::resolve(&pref, census);
        let kept = match &self.home {
            Some(home) => pref
                .save(home)
                .map(|()| "kept")
                .unwrap_or("holds for this session only"),
            None => "holds for this session only",
        };
        self.reasoner = factory(&resolved);
        self.intelligence = resolved;
        TurnOutcome::Facts(format!("{} · {kept}", self.intelligence_line()))
    }

    /// The one line that names the path and where the context goes.
    fn intelligence_line(&self) -> String {
        let locus = self.intelligence.locus.line();
        let name = self.reasoner.name();
        if matches!(self.intelligence.kind, IntelligenceKind::None) || locus.starts_with(&name) {
            format!("intelligence: {locus}")
        } else {
            format!("intelligence: {name} · {locus}")
        }
    }

    /// The banner a terminal prints at open: the root, the path, the locus.
    #[must_use]
    pub fn banner(&self) -> String {
        let readiness = match &self.intelligence.why {
            Some(why) => format!("\n  ⚠ {why}"),
            None => String::new(),
        };
        format!(
            "nika · session\n  root: {}\n  {}{readiness}\n  /help for the card · /quit to close",
            self.snapshot.root.display(),
            self.intelligence_line()
        )
    }

    /// One turn.
    pub fn turn(&mut self, input: &str) -> TurnOutcome {
        let input = input.trim();
        match input {
            "" => return TurnOutcome::Facts(String::new()),
            "/quit" | "/exit" => return TurnOutcome::Quit,
            "/help" => return TurnOutcome::Help(HELP.to_owned()),
            "/intelligence" => {
                return match &self.census {
                    Some(census) => TurnOutcome::Ask(format!(
                        "{}\n{}",
                        self.intelligence_card(),
                        census.first_screen()
                    )),
                    None => TurnOutcome::Facts(self.intelligence_card()),
                };
            }
            _ => {}
        }
        if let Some(fact) = crate::facts::answer(input, &self.snapshot, &self.snapshot.root) {
            self.remember(input, &fact);
            return TurnOutcome::Facts(fact);
        }
        if !self.intelligence.ready {
            let why =
                self.intelligence.why.clone().unwrap_or_else(|| {
                    "this session has no conversational intelligence".to_owned()
                });
            return TurnOutcome::Refusal(format!(
                "{why} — the facts still answer (workflows · builtins · providers · check · explain)"
            ));
        }
        if self.intent.goal.is_none() {
            self.intent.goal = Some(input.to_owned());
        }
        let named = named_files(input);
        let bundle = self.broker.bundle(
            &self.snapshot,
            self.intent.goal.as_deref(),
            &named,
            &self.intelligence.locus.line(),
        );
        let prompt = ContextBroker::prompt(&bundle, &self.recent, input);
        match self.reasoner.reason(&prompt) {
            Ok(reply) => {
                let findings = self.known.audit(&reply.text);
                let shown = KnownWorld::correct(&reply.text, &findings);
                self.remember(input, &shown);
                TurnOutcome::Reply(shown)
            }
            Err(ReasonError::NoIntelligence) => TurnOutcome::Refusal(
                "no conversational intelligence — the facts still answer (workflows · builtins · providers · check · explain) · `/intelligence` to choose a path".to_owned(),
            ),
            Err(e) => TurnOutcome::Refusal(format!("{e} — the choice stands (`/intelligence` to change it); nothing was substituted")),
        }
    }

    fn intelligence_card(&self) -> String {
        format!(
            "{}\n{}",
            self.intelligence_line(),
            self.intelligence
                .why
                .as_deref()
                .map_or(String::new(), |w| format!("  ⚠ {w}\n"))
        )
    }

    fn remember(&mut self, user: &str, assistant: &str) {
        self.recent.push((user.to_owned(), assistant.to_owned()));
        if self.recent.len() > RECENT_TURNS {
            self.recent.remove(0);
        }
    }
}

/// The workflow or project files an input names.
fn named_files(input: &str) -> Vec<String> {
    input
        .split(|c: char| {
            c.is_whitespace()
                || c == '`'
                || c == '"'
                || c == '\''
                || c == ','
                || c == '?'
                || c == '('
                || c == ')'
        })
        .filter(|t| t.ends_with(".nika.yaml") || *t == "nika.yaml")
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::intelligence::{DataLocus, IntelligenceKind};
    use crate::reasoner::{NoReasoner, Reply, ScriptedReasoner};

    /// A seat reasoner whose name is the seat itself, as the harness one is.
    struct Seat(&'static str);

    impl SessionReasoner for Seat {
        fn name(&self) -> String {
            self.0.to_owned()
        }

        fn reason(&mut self, _prompt: &str) -> Result<Reply, crate::reasoner::ReasonError> {
            Ok(Reply {
                text: "seated".to_owned(),
                usage_observed: false,
            })
        }
    }

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("alpha.nika.yaml"),
            "nika: alpha\nmodel: mock/echo\ntasks:\n  t:\n    infer: { prompt: \"sk-live-ABCDEFGH123456\", max_tokens: 10 }\n",
        )
        .expect("a");
        dir
    }

    fn ready(kind: IntelligenceKind, locus: DataLocus) -> ResolvedSessionIntelligence {
        ResolvedSessionIntelligence {
            kind,
            model: None,
            locus,
            ready: true,
            why: None,
        }
    }

    /// A chat turn never writes a temp workflow nor a trace: the tree is
    /// untouched after three turns.
    #[test]
    fn a_turn_writes_nothing() {
        let dir = tree();
        let reasoner = ScriptedReasoner::new(vec!["Sure.".to_owned()]);
        let mut s = SessionRuntime::open(
            dir.path(),
            ready(
                IntelligenceKind::Local {
                    provider: "ollama".to_owned(),
                },
                DataLocus::Local,
            ),
            Box::new(reasoner),
        );
        let before: Vec<_> = std::fs::read_dir(dir.path())
            .expect("dir")
            .flatten()
            .map(|e| e.path())
            .collect();
        let _ = s.turn("what workflows are here?");
        let _ = s.turn("explain what alpha.nika.yaml does");
        assert!(
            matches!(s.turn("/help"), TurnOutcome::Help(ref card) if card.contains("no AI asked") && card.contains("what Nika calls")),
            "the card names the shapes that answer without a model"
        );
        let after: Vec<_> = std::fs::read_dir(dir.path())
            .expect("dir")
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(before, after, "no temp file, no .nika/ tree");
        assert!(!dir.path().join(".nika").exists());
    }

    /// The reasoner receives only the bundle: the grounding, the facts,
    /// the file the human named (redacted), the turn — never the
    /// environment, never a file the human did not name.
    #[test]
    fn the_reasoner_receives_only_the_bundle() {
        let dir = tree();
        std::fs::write(
            dir.path().join("secret.nika.yaml"),
            "nika: hidden\ntasks: {}\n",
        )
        .expect("hidden");
        let mut s = SessionRuntime::open(
            dir.path(),
            ready(
                IntelligenceKind::Api {
                    provider: "mistral".to_owned(),
                },
                DataLocus::Metered {
                    provider: "mistral".to_owned(),
                },
            ),
            Box::new(ScriptedReasoner::new(vec!["It reads a file.".to_owned()])),
        );
        let out = s.turn("what does alpha.nika.yaml do?");
        assert!(
            matches!(out, TurnOutcome::Reply(ref t) if t.contains("It reads a file.")),
            "{out:?}"
        );
        // reach the scripted reasoner's record through a second session? the
        // reasoner is boxed: assert on the prompt shape via a fresh reasoner
        let mut probe = ScriptedReasoner::new(vec!["x".to_owned()]);
        let snapshot = ProjectSnapshot::observe(dir.path());
        let broker = ContextBroker::new(snapshot.root.clone());
        let bundle = broker.bundle(
            &snapshot,
            Some("goal"),
            &["alpha.nika.yaml".to_owned()],
            "metered",
        );
        let prompt = ContextBroker::prompt(&bundle, &[], "what does alpha.nika.yaml do?");
        let _ = probe.reason(&prompt);
        let seen = &probe.seen[0];
        assert!(
            seen.contains("Never invent Nika syntax"),
            "the identity core rides"
        );
        assert!(
            seen.contains("File `alpha.nika.yaml`"),
            "the named file rides"
        );
        assert!(
            !seen.contains("nika: hidden"),
            "an unnamed file never rides"
        );
        assert!(
            !seen.contains("sk-live-ABCDEFGH123456"),
            "the secret never rides"
        );
        assert!(
            !seen.contains("PATH=") && !seen.contains("OPENAI_API_KEY=") && !seen.contains("HOME="),
            "the environment never rides"
        );
    }

    /// An invented workflow language in the reply is corrected before the
    /// human sees it; a claim of ignorance too.
    #[test]
    fn an_invented_grammar_is_corrected_before_the_human_sees_it() {
        let dir = tree();
        let invented = "Here is your workflow:\n```yaml\nversion: 1\nsteps:\n  - fetch_internet: https://x\n```\nUse `nika:telegram` to notify. I don't know Nika's exact syntax.";
        let mut s = SessionRuntime::open(
            dir.path(),
            ready(
                IntelligenceKind::Local {
                    provider: "ollama".to_owned(),
                },
                DataLocus::Local,
            ),
            Box::new(ScriptedReasoner::new(vec![invented.to_owned()])),
        );
        let out = s.turn("make me a workflow that fetches a site and notifies telegram");
        let TurnOutcome::Reply(text) = out else {
            panic!("{out:?}");
        };
        assert!(
            text.contains("grounding (the installed engine disagrees"),
            "{text}"
        );
        assert!(text.contains("`steps` is not a workflow field"), "{text}");
        assert!(text.contains("`nika:telegram` is not a builtin"), "{text}");
        assert!(
            text.contains("the installed engine's canon is available"),
            "{text}"
        );
        assert_eq!(
            s.intent.goal.as_deref(),
            Some("make me a workflow that fetches a site and notifies telegram")
        );
    }

    /// Without conversational intelligence the facts still answer and a
    /// free-text turn is refused with the fix, never routed elsewhere.
    #[test]
    fn without_intelligence_the_facts_stay_and_free_text_is_refused() {
        let dir = tree();
        let mut s = SessionRuntime::open(
            dir.path(),
            ready(IntelligenceKind::None, DataLocus::None),
            Box::new(NoReasoner),
        );
        assert!(
            matches!(s.turn("which builtins exist?"), TurnOutcome::Facts(ref t) if t.contains("nika:read"))
        );
        assert!(
            matches!(s.turn("write a haiku"), TurnOutcome::Refusal(ref t) if t.contains("no conversational intelligence"))
        );
        assert!(matches!(s.turn("/quit"), TurnOutcome::Quit));
        assert!(s.banner().contains("no conversational AI"));
        assert_eq!(
            s.banner().matches("no conversational AI").count(),
            1,
            "the path is named once: {}",
            s.banner()
        );
    }

    /// `/intelligence` asks the first screen again in-session; the next
    /// line is the answer, kept under the home, the reasoner rebuilt; an
    /// unserved pick is refused and the previous choice stands.
    #[test]
    fn the_intelligence_can_be_rechosen_in_session() {
        let dir = tree();
        let home = tempfile::tempdir().expect("home");
        let census = IntelligenceCensus {
            seats: vec![crate::intelligence::SeatSeen {
                id: "codex".to_owned(),
                product_present: true,
                configured: true,
            }],
            api_keys: vec![],
            locals: vec![],
        };
        let pref = UserIntelligencePreference::new(IntelligenceKind::None, None);
        let factory: ReasonerFactory = Box::new(|resolved| match &resolved.kind {
            IntelligenceKind::None => Box::new(NoReasoner),
            _ => Box::new(ScriptedReasoner::new(vec!["seated".to_owned()])),
        });
        let mut s =
            SessionRuntime::open_with(dir.path(), census, &pref, Some(home.path()), factory);
        assert!(matches!(s.turn("hello"), TurnOutcome::Refusal(_)));
        let TurnOutcome::Ask(screen) = s.turn("/intelligence") else {
            panic!("asks");
        };
        assert!(screen.contains("Choose which AI"), "{screen}");
        assert!(
            matches!(s.choose("2"), TurnOutcome::Refusal(ref t) if t.contains("previous choice stands"))
        );
        assert!(
            matches!(s.turn("hello"), TurnOutcome::Refusal(_)),
            "still none"
        );
        assert!(
            matches!(s.choose("1"), TurnOutcome::Facts(ref t) if t.contains("codex") && t.contains("kept"))
        );
        assert!(matches!(s.turn("hello"), TurnOutcome::Reply(ref t) if t.contains("seated")));
        let back = UserIntelligencePreference::load(home.path()).expect("kept under the home");
        assert_eq!(
            back.kind,
            IntelligenceKind::Harness {
                seat: "codex".to_owned()
            }
        );
    }

    /// An explicit choice this machine cannot serve refuses every
    /// free-text turn with its fix — the facts still answer.
    #[test]
    fn an_unserved_choice_refuses_with_its_fix() {
        let dir = tree();
        let unserved = ResolvedSessionIntelligence {
            kind: IntelligenceKind::Harness {
                seat: "claude-code".to_owned(),
            },
            model: None,
            locus: DataLocus::Remote {
                product: "claude-code".to_owned(),
            },
            ready: false,
            why: Some("`claude-code` is not installed on this machine — install it".to_owned()),
        };
        let mut s = SessionRuntime::open(dir.path(), unserved, Box::new(Seat("claude-code")));
        assert!(s.banner().contains("⚠ `claude-code` is not installed"));
        assert!(
            s.banner().contains("intelligence: claude-code · uses")
                && !s.banner().contains("claude-code · claude-code"),
            "the seat is named once: {}",
            s.banner()
        );
        assert!(
            matches!(s.turn("hello"), TurnOutcome::Refusal(ref t) if t.contains("not installed"))
        );
        assert!(matches!(
            s.turn("what workflows are here?"),
            TurnOutcome::Facts(_)
        ));
    }
}
