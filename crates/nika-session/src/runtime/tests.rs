// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The runtime's conversation tests: turns, facts, the guard, consent,
//! the run request, the gate, the identity a remote host drives by — over
//! the ONE compiler. Work reaches the compiler and its candidate is what a
//! consent lands; a reply is words and never becomes a file; consent is
//! never a run; an explicit run line runs only on a clean check on disk.

use super::*;
use crate::intelligence::{DataLocus, IntelligenceKind};
use crate::reasoner::{NoReasoner, Reply, ScriptedReasoner};

/// A Ready intent: no question, no model, no seat — the compiler's own
/// deterministic reading (the authoring suite proves the round itself).
const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";
/// Where the session lands COPY's candidate in a root without `workflows/`.
const COPY_DEST: &str = "compiled-workflow.nika";
/// An intent whose model the compiler must ask for.
const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";
/// Work the deterministic reader cannot settle alone.
const UNSETTLED: &str = "Read ./a.md and do something clever with it, then write ./b.md";
/// A line that reads as no work at all: the conversation's.
const SMALL_TALK: &str = "hello there, how are you today?";
/// A check-clean workflow that pauses at a human gate: the answer is
/// bound and gates the write (an unbound gate is refused at check).
const GATE: &str = "nika: gate\npermits: { fs: { read: [\"./draft.md\"], write: [\"./final.md\"] }, tools: [\"nika:read\", \"nika:prompt\", \"nika:write\"] }\ntasks:\n  read_draft:\n    invoke: { tool: \"nika:read\", args: { path: \"./draft.md\" } }\n  approve:\n    invoke: { tool: \"nika:prompt\", args: { mode: confirm, message: \"Write final.md?\" } }\n  write_final:\n    after: { approve: success }\n    with: { go: \"${{ tasks.approve.output }}\", text: \"${{ tasks.read_draft.output }}\" }\n    when: \"${{ with.go == true }}\"\n    invoke: { tool: \"nika:write\", args: { path: \"./final.md\", content: \"${{ with.text }}\" } }\n";
/// The event a run paused at that gate leaves in its trace.
const PAUSED: &str = "{\"kind\":\"workflow_paused\",\"fields\":[{\"key\":\"task\",\"value\":\"approve\"},{\"key\":\"mode\",\"value\":\"confirm\"},{\"key\":\"message\",\"value\":\"Write final.md?\"}]}\n";
/// A workflow with findings (no `permits:` block for its program).
const DIRTY: &str =
    "nika: drifted\ntasks:\n  t:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n";

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
        dir.path().join("alpha.nika"),
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

/// A session on a harness seat (words only): authoring stays deterministic.
fn ready_with(dir: &Path, replies: Vec<&str>) -> SessionRuntime {
    let seated = ResolvedSessionIntelligence {
        kind: IntelligenceKind::Harness {
            seat: "codex".to_owned(),
        },
        model: None,
        locus: DataLocus::Remote {
            product: "codex".to_owned(),
        },
        ready: true,
        why: None,
    };
    SessionRuntime::open(
        dir,
        seated,
        Box::new(ScriptedReasoner::new(
            replies.into_iter().map(str::to_owned).collect(),
        )),
    )
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
    let _ = s.turn("explain what alpha.nika does");
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
    std::fs::write(dir.path().join("secret.nika"), "nika: hidden\ntasks: {}\n").expect("hidden");
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
    let out = s.turn("what does alpha.nika do?");
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
        &["alpha.nika".to_owned()],
        "metered",
    );
    let prompt = ContextBroker::prompt(&bundle, &[], "what does alpha.nika do?");
    let _ = probe.reason(&prompt);
    let seen = &probe.seen[0];
    assert!(
        seen.contains("Never invent Nika syntax"),
        "the identity core rides"
    );
    assert!(seen.contains("File `alpha.nika`"), "the named file rides");
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

/// Without conversational intelligence the facts still answer; a line
/// that reads as no work is refused with the fix, never routed elsewhere;
/// a line that reads as work is compiled all the same — the question, the
/// proposal and the honest incomplete need no model.
#[test]
fn without_intelligence_the_facts_stay_free_text_is_refused_and_work_compiles() {
    let dir = tree();
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    );
    assert!(
        matches!(s.turn("which builtins exist?"), TurnOutcome::Facts(ref t) if t.contains("nika:read"))
    );
    for no_work in [SMALL_TALK, "tell me a joke about comets"] {
        let TurnOutcome::Refusal(why) = s.turn(no_work) else {
            panic!("no work in the line: the conversation owns it, and none serves");
        };
        assert_eq!(why.class, RefusalClass::NoIntelligence, "{why}");
        assert!(why.text.contains("no conversational intelligence"), "{why}");
    }
    // An imperative line is work, never a reply: the compiler reads it
    // and asks for what it cannot invent, even with no intelligence.
    let TurnOutcome::Question { key, .. } = s.turn("write a haiku") else {
        panic!("an imperative line reaches the compiler");
    };
    assert_eq!(key, "model");
    assert!(matches!(s.turn("cancel"), TurnOutcome::Facts(ref t) if t.contains("discarded")));
    // Work compiles without any intelligence: the candidate is proposed.
    let TurnOutcome::Proposal { preview, .. } = s.turn(COPY) else {
        panic!("an explicit intent is Ready without a seat");
    };
    assert!(
        preview.starts_with("Nika proposes `compiled-workflow.nika`:"),
        "{preview}"
    );
    assert!(matches!(s.consent("no"), TurnOutcome::Facts(ref t) if t.contains("discarded")));
    // A hole the compiler cannot invent is asked, not refused.
    let TurnOutcome::Question { key, .. } = s.turn(DRAFT) else {
        panic!("the compiler's question needs no intelligence");
    };
    assert_eq!(key, "model");
    assert!(matches!(s.turn("mock/echo"), TurnOutcome::Proposal { .. }));
    assert!(matches!(s.consent("no"), TurnOutcome::Facts(_)));
    // Work the reader cannot settle is an honest incomplete naming the fix.
    let TurnOutcome::Facts(text) = s.turn(UNSETTLED) else {
        panic!("no seat: the reasons are stated, nothing is invented");
    };
    assert!(
        text.starts_with("I read this as work but cannot build it yet"),
        "{text}"
    );
    assert!(
        text.contains("no conversational intelligence") && text.contains("`/intelligence`"),
        "the incomplete names the seat that would read it: {text}"
    );
    assert!(
        !dir.path().join(COPY_DEST).exists(),
        "two discarded proposals and an incomplete wrote nothing"
    );
    assert!(matches!(s.turn("/quit"), TurnOutcome::Quit));
    assert!(
        s.banner().contains("What do you want to automate?"),
        "the banner is the human's question: {}",
        s.banner()
    );
    assert!(
        !s.banner().contains("conversational AI") && !s.banner().contains("authoring"),
        "the engine's facts are not on the banner: {}",
        s.banner()
    );
    assert!(s.status().contains("no conversational AI"));
    assert_eq!(
        s.status().matches("no conversational AI").count(),
        1,
        "the path is named once: {}",
        s.status()
    );
    assert!(
        s.status().contains("authoring · deterministic"),
        "the status names the seat authoring reasons with: {}",
        s.status()
    );
}

/// The first run opens without a choice: the facts and the deterministic
/// compiler answer at once; the first turn that needs an intelligence asks
/// the first screen in context and keeps the line, a typo keeps it
/// waiting, `cancel` drops it without a choice, and a choice resumes it
/// exactly as typed under the chosen intelligence.
#[test]
fn an_unchosen_session_asks_in_context_and_resumes_the_waiting_line() {
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
    let factory: ReasonerFactory = Box::new(|resolved| match &resolved.kind {
        IntelligenceKind::None => Box::new(NoReasoner),
        _ => Box::new(ScriptedReasoner::new(vec!["seated".to_owned()])),
    });
    let mut s = SessionRuntime::open_unchosen(dir.path(), census, Some(home.path()), factory);
    assert!(!s.intelligence_chosen());
    assert!(!s.pending_choice());
    assert!(s.status().contains("not chosen yet"), "{}", s.status());
    // The facts and work need no choice.
    assert!(
        matches!(s.turn("what workflows are here?"), TurnOutcome::Facts(ref t) if t.contains("alpha.nika"))
    );
    assert!(
        matches!(s.turn(COPY), TurnOutcome::Proposal { .. }),
        "deterministic work compiles before any choice"
    );
    assert!(!s.pending_choice(), "nothing asked so far");
    // The first line only an intelligence answers asks, in context.
    let TurnOutcome::Ask(screen) = s.turn(SMALL_TALK) else {
        panic!("asks in context");
    };
    assert!(
        screen.contains("Nika needs an intelligence for this part")
            && screen.contains("to answer this in words")
            && screen.contains("resumes after the choice")
            && screen.contains("4  No AI"),
        "{screen}"
    );
    assert!(
        !screen.contains("Choose which AI"),
        "not the cold first screen: {screen}"
    );
    assert!(s.pending_choice());
    // A typo keeps the screen and the line.
    assert!(
        matches!(s.choose("9"), TurnOutcome::Refusal(ref r) if r.text.contains("not a choice"))
    );
    assert!(s.pending_choice(), "the choice still waits after a typo");
    // A cancel drops the line, chooses nothing, and the session goes on.
    assert!(
        matches!(s.choose("cancel"), TurnOutcome::Facts(ref t) if t.contains("not sent anywhere"))
    );
    assert!(!s.pending_choice() && !s.intelligence_chosen());
    assert!(
        UserIntelligencePreference::load(home.path()).is_none(),
        "nothing kept on a cancel"
    );
    // Asked again, a choice resumes the very line under the intelligence.
    assert!(matches!(s.turn(SMALL_TALK), TurnOutcome::Ask(_)));
    let TurnOutcome::Resumed { notice, outcome } = s.choose("1") else {
        panic!("the choice resumes the waiting line");
    };
    assert!(
        notice.contains("codex") && notice.contains("kept"),
        "{notice}"
    );
    assert!(
        matches!(*outcome, TurnOutcome::Reply(ref t) if t.contains("seated")),
        "the waiting line ran under the chosen intelligence: {outcome:?}"
    );
    assert!(s.intelligence_chosen() && !s.pending_choice());
    assert!(UserIntelligencePreference::load(home.path()).is_some());
    // Chosen, the session never asks again on its own.
    assert!(matches!(s.turn(SMALL_TALK), TurnOutcome::Reply(_)));
}

/// Choosing « no AI » in context resumes the line too: the honest refusal
/// that names the facts, never a silent drop.
#[test]
fn choosing_no_intelligence_in_context_resumes_with_the_facts() {
    let dir = tree();
    let factory: ReasonerFactory = Box::new(|_| Box::new(NoReasoner));
    let mut s =
        SessionRuntime::open_unchosen(dir.path(), IntelligenceCensus::empty(), None, factory);
    assert!(matches!(s.turn(SMALL_TALK), TurnOutcome::Ask(_)));
    let TurnOutcome::Resumed { notice, outcome } = s.choose("4") else {
        panic!("resumes");
    };
    assert!(notice.contains("no conversational AI"), "{notice}");
    assert!(
        matches!(*outcome, TurnOutcome::Refusal(ref r) if r.class == RefusalClass::NoIntelligence && r.text.contains("facts still answer")),
        "{outcome:?}"
    );
    assert!(s.intelligence_chosen());
    assert!(
        matches!(s.turn(SMALL_TALK), TurnOutcome::Refusal(_)),
        "an explicit none is never re-asked"
    );
}

/// The review reads in sections a human decides on (Does · Runs · Can
/// touch · Changes · Needs · nothing has run yet); `/meaning` beside the
/// proposal lists the request clause by clause from the compiler's ledger
/// and HOLDS the proposal; the status line names where the automation
/// stands at every step, from the machine's own facts.
#[test]
fn the_review_reads_in_sections_and_meaning_holds_the_proposal() {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), "brief\n").expect("brief");
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    );
    assert_eq!(s.status_line(), "", "nothing under way at open");
    let TurnOutcome::Proposal { id, preview } = s.turn(COPY) else {
        panic!("a copy is Ready");
    };
    for section in [
        "Does\n",
        "Runs\n  when you ask (« run it »)",
        "Can touch\n  external effects · none",
        "human approval at run · none",
        "Changes\n  + `compiled-workflow.nika` · ",
        "Needs\n  nothing more from you",
        "Nothing has run yet · `yes` saves these exact bytes",
        "`/meaning` your request clause by clause",
    ] {
        assert!(
            preview.contains(section),
            "missing « {section} » in:\n{preview}"
        );
    }
    assert!(
        s.status_line()
            .starts_with("Ready for review · `compiled-workflow.nika`"),
        "{}",
        s.status_line()
    );
    // Meaning beside the proposal: the ledger's clauses, the proposal held.
    let TurnOutcome::Held {
        id: held,
        preview: meaning,
    } = s.consent("/meaning")
    else {
        panic!("meaning holds the proposal");
    };
    assert_eq!(held, id);
    assert!(
        meaning.contains("Meaning · your request, clause by clause")
            && meaning.contains("✓ « write it to ./out/copy.md »")
            && meaning.contains("a task that runs (`write_output`)")
            && meaning.contains("the proposal still waits"),
        "{meaning}"
    );
    assert!(
        !meaning.contains("1/1") && !meaning.contains('%'),
        "no score: {meaning}"
    );
    assert_eq!(s.pending_proposal(), Some(id), "held, not decided");
    assert!(matches!(
        s.consent("what did you understand?"),
        TurnOutcome::Held { .. }
    ));
    // A yes lands the bytes: the status says saved, checked, not run.
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(ref t) if t.contains("applied")));
    assert!(
        s.status_line().starts_with(
            "Saved · checked · not active · nothing has run · `compiled-workflow.nika`"
        ),
        "{}",
        s.status_line()
    );
    // After the proposal, `/meaning` is an aside over the last reading.
    assert!(
        matches!(s.turn("/meaning"), TurnOutcome::Aside(ref t) if t.contains("clause by clause"))
    );
}

/// A schedule stated in the request is kept beside the program: saving
/// activates nothing; « activate » asks the three values the sentence did
/// not state (time zone · missed policy · ceiling), proposes the
/// declaration as a project change, and a yes writes `nika.yaml` — which
/// the cadence grammar and the project vocabulary both read back; the
/// status then says declared, not proven active.
///
/// The shared first act: a daily copy stated in words, proposed, saved by
/// a yes; the save declares nothing.
fn saved_daily_copy(dir: &Path) -> SessionRuntime {
    std::fs::create_dir_all(dir.join("notes")).expect("notes");
    std::fs::write(dir.join("notes/brief.md"), "brief\n").expect("brief");
    let mut s = SessionRuntime::open(
        dir,
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    );
    assert!(
        matches!(s.turn("activate"), TurnOutcome::Refusal(ref r) if r.class == RefusalClass::WrongState),
        "nothing to activate before a workflow is saved"
    );
    let TurnOutcome::Proposal { preview, .. } =
        s.turn("Chaque matin à 8h, lis ./notes/brief.md et écris-le dans ./out/copie.md")
    else {
        panic!("a stated cadence is Ready with a trigger requirement beside it");
    };
    assert!(
        preview.contains(
            "↗ daily at 08:00 (« chaque matin à 8h ») · a schedule to activate AFTER saving"
        ),
        "{preview}"
    );
    assert!(
        preview.contains("Needs\n  ↗ the schedule or trigger above · bound when you activate"),
        "{preview}"
    );
    let TurnOutcome::Facts(saved) = s.consent("oui") else {
        panic!("the yes saves the program");
    };
    assert!(
        saved.contains("Saved · checked · not active · nothing has run")
            && saved.contains("say « activate » to declare « chaque matin à 8h »"),
        "{saved}"
    );
    assert!(
        !dir.join("nika.yaml").exists(),
        "saving the workflow declared nothing"
    );
    s
}

#[test]
fn a_schedule_is_declared_only_through_the_human_s_gestures() {
    let dir = tree();
    let mut s = saved_daily_copy(dir.path());
    // The activation conversation: three typed values, each its own line.
    let TurnOutcome::Question { key, question } = s.turn("activate") else {
        panic!("activate asks first");
    };
    assert_eq!(key, "project.timezone");
    assert_eq!(
        s.pending_activation(),
        Some("project.timezone"),
        "the shells read the waiting value from the runtime: the prompt is `reply ›`"
    );
    assert_eq!(
        s.status_line(),
        "Needs one value to declare the schedule · `project.timezone`"
    );
    assert!(
        question.contains("Which time zone") && question.contains("(2 more after this one)"),
        "{question}"
    );
    assert!(
        matches!(s.turn("why?"), TurnOutcome::Aside(ref t) if t.contains("Declared is not active"))
    );
    assert!(
        matches!(s.turn("Paris"), TurnOutcome::Refusal(ref r) if r.text.contains("Area/City")),
        "a bare city is not a zone"
    );
    let TurnOutcome::Question { key, .. } = s.turn("Europe/Paris") else {
        panic!("the missed policy is asked next");
    };
    assert_eq!(key, "project.missed");
    let TurnOutcome::Question { key, .. } = s.turn("1") else {
        panic!("the ceiling is asked last");
    };
    assert_eq!(key, "project.ceiling");
    assert!(
        matches!(s.turn("free"), TurnOutcome::Refusal(_)),
        "a ceiling is an amount"
    );
    let TurnOutcome::Proposal { id, preview } = s.turn("0.20") else {
        panic!("the declaration is proposed, never written on the answer");
    };
    assert!(
        preview.contains("Nika proposes to declare the schedule in `nika.yaml`")
            && preview.contains("TZ=Europe/Paris 0 8 * * *")
            && preview.contains("if missed · rattraper-une-fois")
            && preview.contains("ceiling · $0.2 per scheduled run")
            && preview.contains("Declared is not active"),
        "{preview}"
    );
    assert_eq!(s.pending_proposal(), Some(id));
    assert!(
        !dir.path().join("nika.yaml").exists(),
        "proposed, not written"
    );
    let TurnOutcome::Facts(declared) = s.consent("yes") else {
        panic!("the yes writes the declaration");
    };
    assert!(
        declared.contains("applied · wrote `nika.yaml`")
            && declared.contains("Declared in `nika.yaml` · not active"),
        "{declared}"
    );
    let file = std::fs::read_to_string(dir.path().join("nika.yaml")).expect("nika.yaml");
    assert!(
        file.contains("arm:")
            && file.contains("workflow: compiled-workflow.nika")
            && file.contains("cadence: \"TZ=Europe/Paris 0 8 * * *\"")
            && file.contains("plafond: 0.2")
            && file.contains("manqué: rattraper-une-fois"),
        "{file}"
    );
}

/// The declaration Nika wrote reads in both grammars (the cadence
/// registry and the project file), the status line says declared and
/// not proven active, and a second activation never rewrites a list.
#[test]
fn a_declared_schedule_reads_in_both_grammars_and_is_never_rewritten() {
    let dir = tree();
    let mut s = saved_daily_copy(dir.path());
    let _ = s.turn("activate");
    let _ = s.turn("Europe/Paris");
    let _ = s.turn("1");
    let TurnOutcome::Proposal { .. } = s.turn("0.20") else {
        panic!("the declaration is proposed");
    };
    let TurnOutcome::Facts(_) = s.consent("yes") else {
        panic!("the yes writes the declaration");
    };
    let file = std::fs::read_to_string(dir.path().join("nika.yaml")).expect("nika.yaml");
    // Both grammars read the file Nika wrote.
    let registry =
        nika_cadence::parse::parse_registry(&file).expect("the cadence grammar reads it");
    assert_eq!(registry.beat_count(), 1);
    let (_, project) = nika_vocab::project::discover(dir.path())
        .expect("discover")
        .expect("a project");
    assert_eq!(project.arm().len(), 1);
    assert!(
        s.status_line().starts_with("Declared · ") && s.status_line().contains("not proven active"),
        "{}",
        s.status_line()
    );
    // A second activation of the same workflow never rewrites the list.
    assert!(matches!(s.turn("activate"), TurnOutcome::Question { .. }));
    let _ = s.turn("Europe/Paris");
    let _ = s.turn("2");
    assert!(
        matches!(s.turn("0.10"), TurnOutcome::Facts(ref t) if t.contains("already declares an `arm:` list")),
        "an existing list is never rewritten by Nika"
    );
}

/// The status line during a question names the answer the automation
/// needs; before any work it is empty; a discussion line leaves it so.
#[test]
fn the_status_line_names_the_next_gesture() {
    let dir = tree();
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    );
    assert!(matches!(s.turn(DRAFT), TurnOutcome::Question { .. }));
    assert!(
        s.status_line().starts_with("Needs one answer · "),
        "{}",
        s.status_line()
    );
    assert!(matches!(s.turn("cancel"), TurnOutcome::Facts(_)));
    assert_eq!(s.status_line(), "");
    assert!(
        matches!(s.turn("/meaning"), TurnOutcome::Aside(ref t) if t.contains("clause by clause")),
        "the last reading stays readable after a cancel"
    );
}

/// « why? » beside an authoring question explains it from the compiler's
/// own words and holds it: nothing is answered, the same question waits,
/// and `cancel` still drops the round. The raw key stays out of the
/// question's line and lives in the explanation.
#[test]
fn why_beside_a_question_explains_it_and_holds_it() {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), "brief\n").expect("brief");
    let mut s = SessionRuntime::open(
        dir.path(),
        ready(IntelligenceKind::None, DataLocus::None),
        Box::new(NoReasoner),
    );
    let TurnOutcome::Question { key, question } = s.turn(DRAFT) else {
        panic!("a draft asks its model");
    };
    assert_eq!(key, "model");
    assert!(
        !question.contains("(`model`)") && question.contains("`why?` explains"),
        "{question}"
    );
    let TurnOutcome::Aside(text) = s.turn("why?") else {
        panic!("a side question is an aside");
    };
    assert!(
        text.contains("fills `model`") && text.contains("the question still waits"),
        "{text}"
    );
    assert!(text.contains("what you asked"), "the goal is named: {text}");
    assert_eq!(
        s.pending_question().map(|q| q.key.as_str()),
        Some("model"),
        "the aside consumed nothing"
    );
    assert!(matches!(s.turn("pourquoi ?"), TurnOutcome::Aside(_)));
    assert!(matches!(s.turn("/why"), TurnOutcome::Aside(_)));
    assert!(matches!(s.turn("cancel"), TurnOutcome::Facts(ref t) if t.contains("discarded")));
    assert!(s.pending_question().is_none());
    assert!(
        matches!(s.turn("/why"), TurnOutcome::Facts(ref t) if t.contains("nothing waits")),
        "nothing pending: a fact"
    );
}

/// « why? » beside a run's gate says what the answer lets happen, from the
/// workflow's own bytes, and the gate keeps waiting.
#[test]
fn why_beside_a_gate_explains_it_and_holds_it() {
    let dir = tree();
    std::fs::write(dir.path().join("draft.md"), "the draft\n").expect("draft");
    std::fs::write(dir.path().join("gate.nika"), GATE).expect("gate");
    let mut s = ready_with(dir.path(), vec![]);
    assert!(matches!(
        s.turn("run gate.nika"),
        TurnOutcome::RunRequested { .. }
    ));
    let store = dir.path().join(".nika").join("traces");
    std::fs::create_dir_all(&store).expect("store");
    let trace = store.join("paused.ndjson");
    std::fs::write(&trace, PAUSED).expect("trace");
    assert!(matches!(
        s.observe_run(4, Some(&trace)),
        TurnOutcome::GateAsk { .. }
    ));
    let TurnOutcome::Aside(text) = s.answer_gate("why?") else {
        panic!("a side question beside the gate is an aside");
    };
    assert!(
        text.contains("paused at `approve`") && text.contains("write_final · nika:write"),
        "the gated task is named from the bytes: {text}"
    );
    assert!(
        text.contains("nothing after the gate has happened yet"),
        "{text}"
    );
    assert!(s.waiting_gate().is_some(), "the gate still waits");
    assert!(matches!(s.turn("/why"), TurnOutcome::Aside(_)));
    assert!(
        matches!(s.answer_gate("yes"), TurnOutcome::ResumeRequested { .. }),
        "the answer after the aside resumes the run"
    );
}

/// A path that does not answer leaves a recovery card — what happened,
/// what is kept, what did not happen, the ways on — and « what happened? »
/// repeats it from memory: exactly one call was made.
#[test]
fn a_failed_intelligence_leaves_a_recovery_card_repeated_without_a_call() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Failing(Arc<AtomicUsize>);
    impl SessionReasoner for Failing {
        fn name(&self) -> String {
            "mistral API".to_owned()
        }
        fn reason(&mut self, _prompt: &str) -> Result<Reply, crate::reasoner::ReasonError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(crate::reasoner::ReasonError::Provider(
                "rate limited (HTTP 429) after 4 round-trips".to_owned(),
            ))
        }
    }
    let dir = tree();
    let calls = Arc::new(AtomicUsize::new(0));
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
        Box::new(Failing(Arc::clone(&calls))),
    );
    let TurnOutcome::Refusal(card) = s.turn(SMALL_TALK) else {
        panic!("the failure is a refusal");
    };
    assert_eq!(card.class, RefusalClass::IntelligenceRefused);
    assert!(
        card.text.contains("I couldn't use mistral API")
            && card.text.contains("HTTP 429")
            && card.text.contains("I still have")
            && card.text.contains("your request: «")
            && card
                .text
                .contains("Nothing was written and nothing was sent elsewhere")
            && card.text.contains("/intelligence"),
        "{}",
        card.text
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let TurnOutcome::Facts(again) = s.turn("what happened?") else {
        panic!("the card repeats");
    };
    assert_eq!(again, card.text);
    assert!(matches!(s.turn("de quoi ?"), TurnOutcome::Facts(ref t) if *t == card.text));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "repeating the card never calls the path again"
    );
    assert!(
        matches!(s.turn("what workflows are here?"), TurnOutcome::Facts(ref t) if t.contains("alpha.nika")),
        "the facts still answer after a failure"
    );
}

/// Work the deterministic reader cannot settle asks the first screen in
/// context with the authoring reason; the choice resumes the request.
#[test]
fn unsettled_work_asks_in_context_with_the_authoring_reason() {
    let dir = tree();
    std::fs::write(dir.path().join("a.md"), "alpha").expect("a");
    let factory: ReasonerFactory = Box::new(|_| Box::new(NoReasoner));
    let mut s =
        SessionRuntime::open_unchosen(dir.path(), IntelligenceCensus::empty(), None, factory);
    let TurnOutcome::Ask(screen) = s.turn(UNSETTLED) else {
        panic!("asks in context");
    };
    assert!(
        screen.contains("to finish reading this request"),
        "the authoring reason: {screen}"
    );
    let TurnOutcome::Resumed { outcome, .. } = s.choose("4") else {
        panic!("resumes");
    };
    assert!(
        matches!(*outcome, TurnOutcome::Facts(_)),
        "under no seat the request is an honest incomplete: {outcome:?}"
    );
}

/// The compiler's candidate is a proposal: nothing is written until the
/// consent line says yes; `no` discards; a new turn discards a pending
/// set; the next yes lands the exact bytes and the real check follows —
/// and consent is never a run.
#[test]
fn a_proposal_lands_only_on_consent() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    let TurnOutcome::Proposal { preview, .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    assert!(
        preview.starts_with("Nika proposes `compiled-workflow.nika`:"),
        "the review above: {preview}"
    );
    assert!(
        preview.contains(&format!("proposed change · {COPY}")),
        "the header names this turn's request: {preview}"
    );
    assert!(
        preview.contains("creates `compiled-workflow.nika`") && preview.contains("clean ✔"),
        "{preview}"
    );
    assert!(
        !dir.path().join(COPY_DEST).exists(),
        "nothing written before consent"
    );
    assert!(matches!(s.consent("no"), TurnOutcome::Facts(ref t) if t.contains("discarded")));
    assert!(
        matches!(s.turn("1"), TurnOutcome::Facts(ref t) if t.contains("already chosen")),
        "a bare digit is the first-screen reflex, never a message for the seat"
    );
    assert!(!dir.path().join(COPY_DEST).exists(), "no means nothing");
    assert!(
        matches!(s.consent("yes"), TurnOutcome::Refusal(ref r) if r.text.contains("nothing is pending"))
    );
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(
        matches!(s.turn("what workflows are here?"), TurnOutcome::Facts(_)),
        "a new turn"
    );
    assert!(
        matches!(s.consent("yes"), TurnOutcome::Refusal(_)),
        "the new turn discarded the proposal"
    );
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    let TurnOutcome::Facts(report) = s.consent("yes") else {
        panic!("applied — and never a run");
    };
    assert!(
        report.contains("applied · wrote `compiled-workflow.nika`") && report.contains("clean ✔"),
        "{report}"
    );
    assert!(
        report.contains("say « run it »"),
        "consent lands; the run is the next explicit line: {report}"
    );
    let on_disk = std::fs::read_to_string(dir.path().join(COPY_DEST)).expect("landed");
    assert!(
        on_disk.contains("nika:read")
            && on_disk.contains("./notes/brief.md")
            && on_disk.contains("nika:write")
            && on_disk.contains("./out/copy.md"),
        "the compiler's bytes: {on_disk}"
    );
    assert!(
        s.snapshot
            .workflows
            .iter()
            .any(|w| w.path.ends_with(COPY_DEST)),
        "the snapshot sees it"
    );
}

/// A question at the consent prompt is answered and the proposal held;
/// the next `yes` still lands it.
#[test]
fn a_question_at_the_consent_prompt_holds_the_proposal() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    let TurnOutcome::Held { preview: text, .. } = s.consent("what is permits?") else {
        panic!("held");
    };
    assert!(text.contains("boundary"), "{text}");
    let TurnOutcome::Held { preview: text, .. } =
        s.consent("what will this read and write when it runs?")
    else {
        panic!("held");
    };
    assert!(
        text.contains("when it runs:")
            && text.contains("./notes/brief.md")
            && text.contains("./out/copy.md"),
        "the set's own effects: {text}"
    );
    let TurnOutcome::Held { preview: text, .. } = s.consent("hmm") else {
        panic!("held");
    };
    assert!(
        text.contains("not a consent") && text.contains("still waits"),
        "{text}"
    );
    assert!(!dir.path().join(COPY_DEST).exists());
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(ref t) if t.contains("applied")));
    assert!(dir.path().join(COPY_DEST).exists());
}

/// After a run in a git repository, the missing ignore line is named
/// once; a `.gitignore` that keeps the traces out silences it.
#[test]
fn the_trace_hygiene_note_names_the_missing_ignore_line() {
    let dir = tree();
    std::fs::create_dir_all(dir.path().join(".git")).expect("a git root");
    let mut s = ready_with(dir.path(), vec![]);
    assert!(s.snapshot.git_root.is_some(), "a git root");
    let TurnOutcome::Facts(line) = s.observe_run(0, Some(Path::new(".nika/traces/t.ndjson")))
    else {
        panic!("an observation");
    };
    assert!(line.contains("not ignored by git here"), "{line}");
    std::fs::write(dir.path().join(".gitignore"), "target/\n.nika/traces/\n").expect("ignore");
    let TurnOutcome::Facts(line) = s.observe_run(0, Some(Path::new(".nika/traces/t.ndjson")))
    else {
        panic!("an observation");
    };
    assert!(!line.contains("not ignored"), "{line}");
}

/// The run is requested ONLY by an explicit run line, after a clean
/// on-disk check of the workflow last accepted — a consent never runs, a
/// ceiling in the line is honored, findings on disk stop it, and the
/// door's observation becomes a fact.
#[test]
fn a_run_is_requested_only_on_a_clean_check() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    let TurnOutcome::Facts(report) = s.consent("yes") else {
        panic!("a consent lands the bytes and is never a run");
    };
    assert!(
        report.contains("clean ✔") && report.contains("say « run it »"),
        "{report}"
    );
    let TurnOutcome::RunRequested { report, run } = s.turn("run it with a ceiling of 0.05") else {
        panic!("a clean check requests the run of the accepted workflow");
    };
    assert!(report.contains("clean ✔"), "{report}");
    assert_eq!(run.workflow, PathBuf::from(COPY_DEST));
    assert!(run.vars.is_empty());
    assert!(
        (run.max_cost_usd - 0.05).abs() < f64::EPSILON,
        "the ceiling named in the run line"
    );
    assert_eq!(
        ceiling_in("create it and run it once with a ceiling of 0.05"),
        Some(0.05)
    );
    assert_eq!(ceiling_in("run it, cap $0.10 please"), Some(0.10));
    assert_eq!(ceiling_in("run it --max-cost-usd 1"), Some(1.0));
    assert_eq!(ceiling_in("run it --max-cost-usd=0.5"), Some(0.5));
    assert_eq!(ceiling_in("run it once"), None, "no number, the default");
    assert_eq!(
        ceiling_in("write 3 tasks and run it"),
        None,
        "a count is not a ceiling"
    );
    let TurnOutcome::Facts(observed) = s.observe_run(0, Some(Path::new(".nika/traces/t.ndjson")))
    else {
        panic!("an observation");
    };
    assert!(
        observed.contains("exit 0 · succeeded") && observed.contains("t.ndjson"),
        "{observed}"
    );
    // The accepted file drifted on disk since the consent: the check that
    // gates the run is the file as it is now, never the bytes accepted.
    std::fs::write(dir.path().join(COPY_DEST), DIRTY).expect("drift");
    let TurnOutcome::Facts(report) = s.turn("run it") else {
        panic!("findings stop the run before it starts");
    };
    assert!(
        report.contains("findings ✖") && report.contains("the run was not started"),
        "{report}"
    );
    assert!(report.contains("NIKA-AUTH-006"), "{report}");
}

/// A paused run returns to the session as a question; the human's line
/// becomes the resume the door runs; nothing answers for them.
#[test]
fn a_paused_run_asks_the_human_and_the_answer_resumes_it() {
    let dir = tree();
    std::fs::write(dir.path().join("draft.md"), "the draft\n").expect("draft");
    std::fs::write(dir.path().join("gate.nika"), GATE).expect("gate");
    let mut s = ready_with(dir.path(), vec![]);
    let TurnOutcome::RunRequested { run, .. } = s.turn("run gate.nika") else {
        panic!("a named, check-clean gated workflow is requested");
    };
    assert_eq!(run.workflow, PathBuf::from("gate.nika"));
    let store = dir.path().join(".nika").join("traces");
    std::fs::create_dir_all(&store).expect("store");
    let trace = store.join("paused.ndjson");
    std::fs::write(&trace, PAUSED).expect("trace");
    let TurnOutcome::GateAsk { id, question } = s.observe_run(4, Some(&trace)) else {
        panic!("the gate is asked");
    };
    assert_eq!(id, GateId::new(&trace, "approve"));
    assert!(
        question.contains("paused for a human answer") && question.contains("Write final.md?"),
        "{question}"
    );
    assert!(
        matches!(s.answer_gate(""), TurnOutcome::Refusal(ref r) if r.text.contains("nothing answers for you"))
    );
    let TurnOutcome::ResumeRequested {
        workflow,
        trace: t,
        answer,
    } = s.answer_gate("yes")
    else {
        panic!("the resume");
    };
    assert_eq!(workflow, PathBuf::from("gate.nika"));
    assert_eq!(t, trace);
    assert_eq!(answer, "approve=true");
    assert!(
        matches!(s.answer_gate("yes"), TurnOutcome::Refusal(_)),
        "answered once"
    );
    assert!(
        matches!(s.observe_run(0, Some(&trace)), TurnOutcome::Facts(_)),
        "a completed resume is a fact"
    );
    assert!(
        !dir.path().join("final.md").exists(),
        "the session requests; only the door executes"
    );
}

/// The destination is the session's choice, never a model's: a reply
/// naming a path outside the root is words (nothing lands anywhere), and
/// a real candidate lands under `workflows/` when the project keeps one.
#[test]
fn the_session_chooses_the_destination_never_the_model() {
    let dir = tree();
    std::fs::create_dir(dir.path().join(crate::review::WORKFLOWS_DIR)).expect("workflows dir");
    let evil = "```yaml path=../evil.nika\nnika: evil\ntasks: {}\n```\n";
    let mut s = ready_with(dir.path(), vec![evil]);
    let TurnOutcome::Reply(text) = s.turn("what is a violet comet?") else {
        panic!("a reply is words");
    };
    assert!(text.contains("evil.nika"), "shown as words: {text}");
    assert!(
        s.pending_proposal().is_none(),
        "a reply is never a proposal"
    );
    assert!(
        !dir.path().join("../evil.nika").exists() && !dir.path().join("evil.nika").exists(),
        "a path a model named lands nowhere"
    );
    assert!(
        matches!(s.consent("yes"), TurnOutcome::Refusal(_)),
        "nothing pending"
    );
    let TurnOutcome::Proposal { preview, .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    assert!(
        preview.starts_with("Nika proposes `workflows/compiled-workflow.nika`:")
            && preview.contains("creates `workflows/compiled-workflow.nika`"),
        "the session's destination: {preview}"
    );
    let TurnOutcome::Facts(report) = s.consent("yes") else {
        panic!("applied");
    };
    assert!(
        report.contains("applied · wrote `workflows/compiled-workflow.nika`"),
        "{report}"
    );
    assert!(
        dir.path()
            .join(crate::review::WORKFLOWS_DIR)
            .join(COPY_DEST)
            .is_file()
    );
    assert!(!dir.path().join(COPY_DEST).exists());
}

/// `/intelligence` asks the first screen again in-session; the next
/// line is the answer, kept under the home, the reasoner rebuilt and the
/// authoring seat re-derived; an unserved pick is refused and the
/// previous choice stands.
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
    let mut s = SessionRuntime::open_with(dir.path(), census, &pref, Some(home.path()), factory);
    assert!(matches!(s.turn(SMALL_TALK), TurnOutcome::Refusal(_)));
    let TurnOutcome::Ask(screen) = s.turn("/intelligence") else {
        panic!("asks");
    };
    assert!(screen.contains("Choose which AI"), "{screen}");
    assert!(
        matches!(s.choose("2"), TurnOutcome::Refusal(ref r) if r.text.contains("previous choice stands"))
    );
    assert!(
        matches!(s.turn(SMALL_TALK), TurnOutcome::Refusal(_)),
        "still none"
    );
    let TurnOutcome::Facts(chosen) = s.choose("1") else {
        panic!("the choice is kept");
    };
    assert!(
        chosen.contains("codex") && chosen.contains("kept"),
        "{chosen}"
    );
    assert!(
        chosen.contains("authoring · deterministic"),
        "a harness seat reasons in words; authoring stays deterministic: {chosen}"
    );
    assert!(matches!(s.turn(SMALL_TALK), TurnOutcome::Reply(ref t) if t.contains("seated")));
    let back = UserIntelligencePreference::load(home.path()).expect("kept under the home");
    assert_eq!(
        back.kind,
        IntelligenceKind::Harness {
            seat: "codex".to_owned()
        }
    );
}

/// An explicit choice this machine cannot serve refuses every
/// conversational turn with its fix — the facts still answer, and work
/// still compiles (the compiler needs no seat).
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
    assert!(
        s.banner().contains("⚠ `claude-code` is not installed"),
        "an unserved choice is the one warning the banner carries: {}",
        s.banner()
    );
    assert!(
        s.status().contains("intelligence: claude-code · uses")
            && !s.status().contains("claude-code · claude-code"),
        "the seat is named once: {}",
        s.status()
    );
    assert!(
        s.status().contains("authoring · deterministic"),
        "{}",
        s.status()
    );
    assert!(
        matches!(s.turn(SMALL_TALK), TurnOutcome::Refusal(ref r) if r.text.contains("not installed"))
    );
    assert!(matches!(
        s.turn("what workflows are here?"),
        TurnOutcome::Facts(_)
    ));
    assert!(
        matches!(s.turn(COPY), TurnOutcome::Proposal { .. }),
        "work compiles without the seat"
    );
}

/// The freeze audit · a stale apply (the file appeared on disk after the
/// preview) leaves the proposal UNDECIDED: nothing was written, and a
/// retry by identity reads `wrong_state`, never `already_consumed` —
/// « its effect happened once » would be a lie.
#[test]
fn a_stale_apply_leaves_the_proposal_undecided() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    std::fs::write(dir.path().join(COPY_DEST), "nika: raced\n").expect("the race");
    let TurnOutcome::Refusal(stale) = s.consent_to(&id, "yes") else {
        panic!("stale");
    };
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
    assert_eq!(
        std::fs::read_to_string(dir.path().join(COPY_DEST)).expect("still there"),
        "nika: raced\n",
        "nothing was applied"
    );
    let TurnOutcome::Refusal(again) = s.consent_to(&id, "yes") else {
        panic!("wrong state");
    };
    assert_eq!(
        again.class,
        RefusalClass::WrongState,
        "undecided, never consumed: {again}"
    );
    assert!(s.pending_proposal().is_none());
}

/// A remote host judges by identity (ADR-133): a consent naming a
/// proposal that is not the one waiting is stale and applies nothing;
/// the same proposal consents once (and lands, never runs); the same gate
/// answers once.
#[test]
fn a_remote_host_drives_the_machine_by_identity() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("a proposal");
    };
    assert_eq!(s.pending_proposal().as_ref(), Some(&id));
    let other = ProposalId::of("another preview");
    let TurnOutcome::Refusal(stale) = s.consent_to(&other, "yes") else {
        panic!("stale");
    };
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
    assert_eq!(
        s.pending_proposal().as_ref(),
        Some(&id),
        "a stale consent leaves the proposal waiting"
    );
    assert!(!dir.path().join(COPY_DEST).exists(), "nothing was applied");
    assert!(
        matches!(s.consent_to(&id, "yes"), TurnOutcome::Facts(_)),
        "the named consent lands the set and never runs it"
    );
    assert!(dir.path().join(COPY_DEST).is_file());
    let TurnOutcome::Refusal(again) = s.consent_to(&id, "yes") else {
        panic!("consumed");
    };
    assert_eq!(again.class, RefusalClass::AlreadyConsumed, "{again}");
    assert!(s.pending_proposal().is_none());
    let TurnOutcome::Refusal(none) = s.consent("yes") else {
        panic!("nothing pending");
    };
    assert!(none.text.contains("nothing is pending"), "{none}");
    let TurnOutcome::Refusal(foreign) = s.consent_to(&other, "yes") else {
        panic!("wrong state");
    };
    assert_eq!(foreign.class, RefusalClass::WrongState, "{foreign}");
    assert!(
        foreign.text.contains(&other.to_string()) && !foreign.text.contains(&id.to_string()),
        "the refusal names the caller's id, never the last decided one: {foreign}"
    );
    assert!(s.waiting_gate().is_none());
    let gate = GateId::new(Path::new("never.ndjson"), "gate");
    let TurnOutcome::Refusal(no_gate) = s.answer_gate_for(&gate, "yes") else {
        panic!("no gate");
    };
    assert_eq!(no_gate.class, RefusalClass::WrongState, "{no_gate}");
}

/// The result and the proof read the trace's own frames: after the door
/// observes a finished run whose trace this session can read, the facts
/// lead (what was produced, read, the honest cost) and the door's line
/// stays beneath as the remembered fact; `/proof` then judges the chain
/// through the verify door and says what it does not prove. Before any
/// run, `/proof` says where a proof will come from.
#[test]
fn a_finished_run_reads_as_a_result_and_proof_reads_its_trace() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    assert!(
        matches!(s.turn("/proof"), TurnOutcome::Facts(ref t) if t.starts_with("No run observed in this session yet")),
        "before any run, the door to a proof is named"
    );
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(matches!(s.consent("yes"), TurnOutcome::Facts(_)));
    assert!(matches!(s.turn("run it"), TurnOutcome::RunRequested { .. }));
    // The door ran it: a real trace of the deterministic copy (engine
    // 0.120.3) and the file it wrote, as the door leaves them.
    let store = dir.path().join(".nika").join("traces");
    std::fs::create_dir_all(&store).expect("store");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/traces/copy.ndjson"),
        store.join("t.ndjson"),
    )
    .expect("trace");
    std::fs::create_dir_all(dir.path().join("out")).expect("out");
    std::fs::write(
        dir.path().join("out/copie.md"),
        "# Brief\n\nLe lancement passe en octobre.\n",
    )
    .expect("artefact");
    let TurnOutcome::Facts(result) = s.observe_run(0, Some(Path::new(".nika/traces/t.ndjson")))
    else {
        panic!("an observation");
    };
    assert!(
        result.starts_with(&format!(
            "Done · `{COPY_DEST}` · 11 ms · 2 tasks ran · nothing sent elsewhere"
        )),
        "{result}"
    );
    assert!(
        result.contains("\n  produced · ./out/copie.md (40 B)"),
        "{result}"
    );
    assert!(result.contains("\n  read · ./notes/brief.md"), "{result}");
    assert!(
        result.contains("cost · nothing metered · no model was asked"),
        "{result}"
    );
    assert!(
        result.contains("run observed · exit 0 · succeeded · trace `.nika/traces/t.ndjson`"),
        "the door's line stays beneath: {result}"
    );
    assert_eq!(
        result.matches("produced ·").count(),
        1,
        "the produced fact is said once, from the frames: {result}"
    );
    let TurnOutcome::Facts(proof) = s.turn("/proof") else {
        panic!("a proof");
    };
    assert!(proof.starts_with("Proof · "), "{proof}");
    assert!(
        proof.contains("\n  workflow · compiled-workflow · bytes sha256 5d1bf591…0730"),
        "{proof}"
    );
    assert!(
        proof.contains("\n  chain · OK — 13 events · chain intact · head 1cf484e5…7f01"),
        "{proof}"
    );
    assert!(
        proof.contains("written · ./out/copie.md · 40 B · sha256 "),
        "{proof}"
    );
    assert!(
        proof.contains("does not prove · that the content is right"),
        "{proof}"
    );
    assert!(
        s.status_line().starts_with("Done · the run succeeded"),
        "{}",
        s.status_line()
    );
}

/// Leaving is always one line away: `/quit` at the consent prompt drops
/// the proposal and writes nothing; at the first screen it chooses
/// nothing; at a gate it leaves the gate waiting in its trace.
#[test]
fn quit_leaves_from_the_consent_prompt_the_first_screen_and_a_gate() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    assert!(matches!(s.turn(COPY), TurnOutcome::Proposal { .. }));
    assert!(matches!(s.consent("/quit"), TurnOutcome::Quit));
    assert!(s.pending_proposal().is_none(), "the proposal is dropped");
    assert!(!dir.path().join(COPY_DEST).exists(), "nothing written");
    // The first screen, asked in context on a first launch: `/exit` chooses
    // nothing and the waiting line is never sent anywhere.
    let first = tree();
    let factory: ReasonerFactory = Box::new(|_| Box::new(NoReasoner));
    let mut u =
        SessionRuntime::open_unchosen(first.path(), IntelligenceCensus::empty(), None, factory);
    assert!(matches!(u.turn(SMALL_TALK), TurnOutcome::Ask(_)));
    assert!(matches!(u.choose("/exit"), TurnOutcome::Quit));
    assert!(!u.pending_choice(), "no choice made");
    std::fs::write(dir.path().join("draft.md"), "the draft\n").expect("draft");
    std::fs::write(dir.path().join("gate.nika"), GATE).expect("gate");
    assert!(matches!(
        s.turn("run gate.nika"),
        TurnOutcome::RunRequested { .. }
    ));
    let store = dir.path().join(".nika").join("traces");
    std::fs::create_dir_all(&store).expect("store");
    let trace = store.join("paused.ndjson");
    std::fs::write(&trace, PAUSED).expect("trace");
    assert!(matches!(
        s.observe_run(4, Some(&trace)),
        TurnOutcome::GateAsk { .. }
    ));
    assert!(matches!(s.answer_gate("/quit"), TurnOutcome::Quit));
    assert!(
        s.waiting_gate().is_some(),
        "the gate still waits in its trace"
    );
}

/// A bare consent word with nothing pending is refused as a wrong state:
/// it is neither work to build nor a question, and no model reads it.
#[test]
fn a_bare_yes_or_no_with_nothing_pending_is_refused_never_compiled() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    for word in ["yes", "oui", "ok", "no", "non"] {
        let TurnOutcome::Refusal(why) = s.turn(word) else {
            panic!("`{word}` with nothing pending is refused");
        };
        assert_eq!(why.class, RefusalClass::WrongState, "{why}");
        assert!(
            why.text.contains("nothing waits for a yes or a no"),
            "{why}"
        );
    }
    assert!(!dir.path().join(COPY_DEST).exists());
}
