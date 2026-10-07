// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native Run evidence: live frames, kept results, task and child inspection.

use super::*;

/// Two keyless tasks a second apart: the child's frames arrive over time.
const SLOW: &str = r#"nika: slow
permits:
  tools: ["nika:wait"]
tasks:
  first:
    invoke: { tool: "nika:wait", args: { duration: "1s" } }
  second:
    with: { x: "${{ tasks.first.output }}" }
    invoke: { tool: "nika:wait", args: { duration: "1s" } }
"#;

/// 13 · A run asked in the workspace is followed from its own frames: the
/// leg names the execution its first frame carries, binds the graph to the
/// bytes the run names (their sha256), and shows each task as the child
/// reports it while the run has not settled; the settlement and the
/// stream's wholeness come last, apart from the proof. The runtime reports a
/// task's start with its end, so a task shows done or not yet, never a
/// guessed « running ».
#[test]
fn a_run_is_followed_from_its_frames_before_it_settles() {
    let rig = Rig::new("live");
    std::fs::write(rig.path("slow.nika"), SLOW).expect("slow");
    let mut term = rig.spawn("13-live", 120, 36);
    wait_workspace(&mut term);
    term.send("run slow.nika\r");
    term.wait_until("the leg bound to the bytes it runs", |s| {
        s.contains("graph · the bytes") && s.contains("it was asked over")
    });
    term.wait_until("the first task done, the run not settled", |s| {
        s.contains("✔ first") && s.contains("○ second") && !s.contains("settled · succeeded")
    });
    term.wait_until("the settlement", |s| s.contains("settled · succeeded"));
    let shown = term.text();
    for said in [
        "events and the settlement, whole",
        "evidence · unsealed, as the run declared it",
        "✔ second",
    ] {
        assert!(shown.contains(said), "{said}\n{}", term.dump());
    }
    assert!(!shown.contains("graph not bound"), "{}", term.dump());
    term.leave();
}

/// A read and a write with an output: what the run left is found from its own frames.
const COPY: &str = r#"nika: copy
permits:
  fs: { read: ["./notes/brief.md"], write: ["./out/copy.md"] }
  tools: ["nika:read", "nika:write"]
tasks:
  read_source:
    invoke: { tool: "nika:read", args: { path: "./notes/brief.md" } }
  write_output:
    with: { text: "${{ tasks.read_source.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/copy.md", content: "${{ with.text }}" } }
outputs:
  written: ${{ tasks.write_output.output }}
"#;

/// The run's label the object region shows (`run <12 hex>`), when it shows one.
fn run_label(screen: &str) -> Option<String> {
    screen.lines().find_map(|line| {
        line.match_indices("run ").find_map(|(at, _)| {
            let hex = line.get(at + 4..at + 16)?;
            hex.chars()
                .all(|c| c.is_ascii_hexdigit())
                .then(|| format!("run {hex}"))
        })
    })
}

/// Visit the reopened conversation's history, then return to its latest row.
fn visit_reopened_history(term: &mut Term, label: &str) {
    // History may span several cards: prove both markers are reachable,
    // without requiring them to occupy the same page or stay pinned forever.
    let (mut saw_history, mut saw_run) = (false, false);
    for page in 0..=8 {
        saw_history |= term.screen.contains("earlier in this conversation");
        saw_run |= term.screen.contains("(run)");
        assert!(
            term.text().contains(label),
            "scrolling history keeps the same run\n{}",
            term.dump()
        );
        term.shot(&format!("reopened conversation page {page}"));
        if saw_history && saw_run {
            break;
        }
        if page < 8 {
            term.keys(PAGE_UP);
        }
    }
    assert!(
        saw_history && saw_run,
        "the history marker and its run must be accessible within eight pages\n{}",
        term.dump()
    );
    term.keys(END);
}

/// 14 · What a run left, then the same run after a reopen: its outputs as
/// the settlement carried them, the file it reported writing as read now
/// (edited after the run, read again: other bytes, never called the run's),
/// and its Proof bound to its execution, source and receipt. Closed and
/// reopened, the conversation is repainted as history and the same run is
/// offered as evidence whose Proof is read again; nothing replays.
#[test]
fn a_run_result_its_file_and_proof_are_found_again_after_a_reopen() {
    let rig = Rig::new("result");
    std::fs::create_dir_all(rig.path("notes")).expect("notes");
    std::fs::write(rig.path("notes/brief.md"), "# Brief\nline two\n").expect("brief");
    std::fs::write(rig.path("copy.nika"), COPY).expect("copy");
    let mut term = rig.spawn("14-result", 120, 40);
    wait_workspace(&mut term);
    term.send("run copy.nika\r");
    term.wait_until("the settlement", |s| s.contains("settled · succeeded"));
    let label = run_label(&term.text()).expect("the run names its execution");
    term.keys(F6);
    term.keys(RIGHT);
    term.wait_until("the outputs the settlement carried", |s| {
        s.contains("[outputs]") && s.contains("written")
    });
    term.keys(RIGHT);
    term.wait_until("the file read now", |s| {
        s.contains("[files]") && s.contains("reported written by") && s.contains("# Brief")
    });
    term.keys(RIGHT);
    term.wait_until("the proof bound to the run", |s| {
        s.contains("[proof]") && s.contains("this run's journal") && s.contains("receipt match")
    });
    std::fs::write(rig.path("out/copy.md"), "# Edited after the run\n").expect("edit");
    term.keys(LEFT);
    term.keys("r");
    term.wait_until("today's bytes, read again", |s| {
        s.contains("[files]") && s.contains("# Edited after the run")
    });
    assert!(!term.text().contains("unchanged"), "{}", term.dump());
    term.leave();
    let (kept, files) = (journal_bytes(&rig), rig.tree());
    let mut again = rig.spawn("14-reopen", 120, 40);
    wait_workspace(&mut again);
    visit_reopened_history(&mut again, &label);
    // One line of a wrapped notice: a needle never spans a wrap.
    again.wait_text("last run, observed in an earlier");
    again.wait_until("the same run, as evidence", |s| {
        s.contains(&label) && s.contains("earlier session")
    });
    again.keys(F6);
    // Before any Proof: the run's tasks, one task, its outputs and its file,
    // from the journal it left (captured once, the same bytes the Proof reads).
    again.wait_until("its tasks, from its journal", |s| {
        s.contains("› ✔ read_source") && s.contains("write_output")
    });
    let witness = captured(&again.text()).expect("the captured journal's witness");
    again.keys("\r");
    again.wait_until("one task, as its journal recorded it", |s| {
        let preview = preview_text(s);
        preview.contains("task read_source") && preview.contains("as its journal recorded it")
    });
    again.keys(BACKSPACE);
    again.keys(RIGHT);
    again.wait_until("the outputs its journal recorded", |s| {
        s.contains("[outputs]") && s.contains("written") && s.contains("./out/copy.md")
    });
    again.keys(RIGHT);
    again.wait_until("the file it wrote, read now", |s| {
        s.contains("[files]")
            && s.contains("reported written by")
            && s.contains("# Edited after the run")
    });
    again.keys(RIGHT);
    again.wait_until("its proof read again, bound to it", |s| {
        s.contains("[proof]") && s.contains("this run's journal")
    });
    assert_eq!(
        captured(&again.text()),
        Some(witness),
        "the Proof reads the same bytes"
    );
    assert!(again.text().contains(&label), "{}", again.dump());
    // Narrowed, folded below the minimum and widened again: the same run.
    again.resize(80, 24);
    again.wait_until("the proof at 80x24", |s| {
        s.contains("[proof]") && s.contains("verdict")
    });
    again.resize(50, 14);
    again.wait_until("the focus view below the minimum", |s| {
        !s.contains("[proof]") && s.contains("nika ›")
    });
    again.resize(120, 40);
    again.wait_until("the proof back at 120x40", |s| {
        s.contains("[proof]") && s.contains("receipt")
    });
    // Consulting it ran, wrote and resumed nothing: the same journals and
    // the same files.
    assert_eq!(journal_bytes(&rig), kept, "no journal written or changed");
    assert_eq!(rig.tree(), files, "no file written or changed");
    again.leave();
}

/// Every journal under the project's traces, with its bytes.
fn journal_bytes(rig: &Rig) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out: Vec<_> = (std::fs::read_dir(rig.path(".nika/traces")).expect("traces"))
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.is_file())
        .map(|path| {
            let bytes = std::fs::read(&path).expect("journal");
            (path, bytes)
        })
        .collect();
    out.sort();
    out
}

/// The twelve hex digits of the captured journal a screen names
/// (`captured bytes <hex>`), when it names one.
fn captured(screen: &str) -> Option<String> {
    screen.lines().find_map(|line| {
        let at = line.find("captured bytes ")? + "captured bytes ".len();
        let hex = line.get(at..at + 12)?;
        hex.chars()
            .all(|c| c.is_ascii_hexdigit())
            .then(|| hex.to_owned())
    })
}

/// A valid chained journal just under 8 MiB (of another execution: never
/// bound to the run), written over the run's own journal at `path`.
fn heavy_journal(path: &std::path::Path) {
    use sha2::{Digest as _, Sha256};
    let hex = |bytes: &[u8]| -> String {
        Sha256::digest(bytes)
            .iter()
            .fold(String::new(), |mut out, b| {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{b:02x}"));
                out
            })
    };
    let mut chain = hex(b"nika-trace-v1");
    let mut out = String::new();
    let mut n = 0_u64;
    while out.len() < 8 * 1024 * 1024 - 4096 {
        let kind = if n == 0 {
            "workflow_started"
        } else {
            "task_completed"
        };
        let line = format!(
            r#"{{"chain":"{chain}","correlation":null,"execution":{{"uuid":"01a0ef11-0212-70de-a8b3-99de9427fccc"}},"fields":[{{"key":"task","value":"t{n}"}},{{"key":"note","value":"{}"}}],"id":{{"uuid":"01a0ef11-03a1-73d9-a2bc-{n:012x}"}},"kind":"{kind}","run":null,"timestamp":{n}}}"#,
            "x".repeat(300)
        );
        chain = hex(line.as_bytes());
        out.push_str(&line);
        out.push('\n');
        n += 1;
    }
    std::fs::write(path, out).expect("heavy journal");
}

/// 15 · Reading what a run left never freezes the shell: while the proof of
/// a journal near the 8 MiB cap is verified on a worker, the busy row says
/// what is being read and words typed land in the composer at once; the
/// verdict follows, the words stay in the draft, nothing is sent.
#[test]
fn the_shell_stays_live_while_a_run_proof_is_verified() {
    let rig = Rig::new("acquire");
    std::fs::create_dir_all(rig.path("notes")).expect("notes");
    std::fs::write(rig.path("notes/brief.md"), "# Brief\n").expect("brief");
    std::fs::write(rig.path("copy.nika"), COPY).expect("copy");
    let mut term = rig.spawn("15-acquire", 120, 40);
    wait_workspace(&mut term);
    term.send("run copy.nika\r");
    term.wait_until("the settlement", |s| s.contains("settled · succeeded"));
    let traces = std::fs::read_dir(rig.path(".nika/traces")).expect("traces");
    let journal = (traces.filter_map(Result::ok))
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|e| e == "ndjson"))
        .expect("the run's journal");
    heavy_journal(&journal);
    term.send(&format!("{F6}{RIGHT}{RIGHT}{RIGHT}{SHIFT_F6}zz"));
    term.wait_until("words typed while the proof is read", |s| {
        s.contains("zz") && s.contains("reading what the run left") && !s.contains("verdict ·")
    });
    term.wait_until("the verdict, then", |s| {
        s.contains("verdict ·") && s.contains("zz")
    });
    // The words are the draft alone: never echoed into the conversation.
    assert_eq!(term.text().matches("zz").count(), 1, "{}", term.dump());
    term.leave();
}

/// Two tasks, the second reading a file that is not there: a run that fails
/// the same way every time, with nothing to configure and no model.
const PICK: &str = r#"nika: pick
permits:
  fs: { read: ["./notes/absent.md"] }
  tools: ["nika:read"]
tasks:
  greet:
    invoke: { tool: "nika:log", args: { message: hello } }
  look:
    after: { greet: success }
    invoke: { tool: "nika:read", args: { path: "./notes/absent.md" } }
"#;

const BACKSPACE: &str = "\x7f";

/// 16 · A task of the run is picked by its id and read in detail, then the
/// list comes back: the pick survives a resize and the focus view, the
/// detail says the failure the stream carried, and reading it runs nothing
/// again (one journal, the project's files untouched).
#[test]
fn a_task_of_the_run_is_picked_read_and_left() {
    let rig = Rig::new("pick");
    std::fs::write(rig.path("pick.nika"), PICK).expect("pick");
    let mut term = rig.spawn("16-pick", 120, 40);
    wait_workspace(&mut term);
    term.send("run pick.nika\r");
    term.wait_until("the settlement", |s| s.contains("settled · failed"));
    let tree = rig.tree();
    term.keys(F6);
    term.wait_until("the task list, the first task picked", |s| {
        s.contains("tasks · ↑↓ pick · Enter details") && s.contains("› ✔ greet")
    });
    term.keys(DOWN);
    term.wait_text("› ✖ look");
    term.resize(80, 24);
    term.wait_until("the same task at 80x24", |s| s.contains("› ✖ look"));
    term.keys("\r");
    term.wait_until("its detail at 80x24", |s| {
        s.contains("task look") && s.contains("Backspace") && s.contains("failed")
    });
    assert!(term.text().contains("why ·"), "{}", term.dump());
    term.resize(50, 14);
    term.wait_until("the focus view below the minimum", |s| {
        !s.contains("task look") && s.contains("nika ›")
    });
    term.resize(120, 40);
    term.wait_until("the detail back with the workspace", |s| {
        s.contains("task look") && s.contains("why ·")
    });
    term.keys(BACKSPACE);
    term.wait_until("the list again, the same task picked", |s| {
        s.contains("› ✖ look") && !s.contains("task look")
    });
    let journals = (std::fs::read_dir(rig.path(".nika/traces")).expect("traces"))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "ndjson"))
        .count();
    assert_eq!(journals, 1, "reading a task runs nothing again");
    assert_eq!(rig.tree(), tree, "the project's files are untouched");
    term.leave();
    let env = [("NO_COLOR", "1"), ("NIKA_REDUCED_MOTION", "1")];
    let mut ascii = rig.spawn_with("16-pick-ascii", &["--ascii"], 120, 40, &env);
    wait_workspace(&mut ascii);
    ascii.send("run pick.nika\r");
    ascii.wait_until("the settlement", |s| s.contains("settled - failed"));
    ascii.keys(F6);
    ascii.wait_until("the ASCII list", |s| {
        s.contains("tasks - Up/Down pick - Enter details") && s.contains("* ok greet")
    });
    ascii.keys(DOWN);
    ascii.wait_text("* X look");
    ascii.keys("\r");
    ascii.wait_until("the ASCII detail", |s| {
        s.contains("task look") && s.contains("why -")
    });
    // The object region's own rows (the conversation keeps the run story's
    // words as the Session wrote them).
    let object = right_preview(&ascii.screen);
    assert!(
        object.iter().any(|row| row.contains("task look")),
        "{}",
        ascii.dump()
    );
    assert_renderer_ascii(&object.join("\n"));
    let raw = String::from_utf8_lossy(&ascii.raw).into_owned();
    // The colour forms the renderer writes (indexed, true colour, basic):
    // `ESC[38;<col>H` on a 40-row screen is a cursor move, not a colour.
    for hue in [
        "\x1b[38;5;",
        "\x1b[38;2;",
        "\x1b[48;5;",
        "\x1b[48;2;",
        "\x1b[31m",
        "\x1b[32m",
        "\x1b[33m",
    ] {
        assert!(!raw.contains(hue), "a colour under NO_COLOR: {hue:?}");
    }
    ascii.leave();
}

/// A parent whose one task calls a child workflow: both may log (a
/// composed run grants the child's tool in both files), nothing else.
const PARENT: &str = r#"nika: parent
permits:
  tools: ["nika:log"]
tasks:
  call:
    invoke: { workflow: "./child.nika" }
"#;

/// The child the parent calls.
const CHILD: &str = r#"nika: child
permits:
  tools: ["nika:log"]
tasks:
  greet:
    invoke: { tool: "nika:log", args: { message: hello } }
"#;

/// The `.ndjson` journals under the rig's `.nika/traces`.
fn journals(rig: &Rig) -> usize {
    (std::fs::read_dir(rig.path(".nika/traces")).expect("traces"))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "ndjson"))
        .count()
}

/// 17 · From a task of the run, the child run its settle frame named is
/// opened one level down: the host reads the journal that child really
/// wrote, says its identity is not recorded (the local child route binds
/// none), shows the verifier's verdict and the child's own task, and
/// Backspace returns to the parent's task, then its list, through a resize
/// and the focus view, running and writing nothing.
#[test]
fn a_child_run_is_opened_from_its_task_and_left_for_the_parent() {
    let rig = Rig::new("child");
    std::fs::write(rig.path("parent.nika"), PARENT).expect("parent");
    std::fs::write(rig.path("child.nika"), CHILD).expect("child");
    let mut term = rig.spawn("17-child", 120, 40);
    wait_workspace(&mut term);
    term.send("run parent.nika\r");
    term.wait_until("the parent settled", |s| s.contains("settled · succeeded"));
    let (tree, written) = (rig.tree(), journals(&rig));
    term.keys(F6);
    term.wait_text("› ✔ call");
    term.keys("\r");
    term.wait_until("the task names its child", |s| {
        s.contains("task call") && s.contains("child run · ./child.nika")
    });
    term.wait_text("Enter: open its journal");
    term.keys("\r");
    term.wait_until("the child's journal, read", |s| {
        s.contains("child ./child.nika") && s.contains("verdict ·") && s.contains("greet")
    });
    assert!(term.text().contains("not recorded"), "{}", term.dump());
    term.resize(80, 24);
    term.wait_until("the child at 80x24", |s| s.contains("child ./child.nika"));
    term.resize(50, 14);
    term.wait_until("the focus view below the minimum", |s| {
        !s.contains("child ./child.nika") && s.contains("nika ›")
    });
    term.resize(120, 40);
    term.wait_until("the child back with the workspace", |s| {
        s.contains("child ./child.nika") && s.contains("verdict ·")
    });
    term.keys(BACKSPACE);
    term.wait_until("the parent's task again", |s| {
        s.contains("task call") && !s.contains("child ./child.nika")
    });
    term.keys(BACKSPACE);
    term.wait_until("the parent's list again", |s| {
        s.contains("› ✔ call") && !s.contains("task call")
    });
    assert_eq!(journals(&rig), written, "opening the child runs nothing");
    assert_eq!(rig.tree(), tree, "the project's files are untouched");
    term.leave();
    let env = [("NO_COLOR", "1"), ("NIKA_REDUCED_MOTION", "1")];
    let mut ascii = rig.spawn_with("17-child-ascii", &["--ascii"], 120, 40, &env);
    wait_workspace(&mut ascii);
    ascii.send("run parent.nika\r");
    ascii.wait_until("the parent settled", |s| s.contains("settled - succeeded"));
    ascii.keys(F6);
    ascii.wait_text("* ok call");
    ascii.keys("\r");
    ascii.wait_text("Enter: open its journal");
    ascii.keys("\r");
    ascii.wait_until("the ASCII child", |s| {
        s.contains("child ./child.nika") && s.contains("verdict -")
    });
    let object = right_preview(&ascii.screen);
    assert_renderer_ascii(&object.join("\n"));
    let raw = String::from_utf8_lossy(&ascii.raw).into_owned();
    for hue in ["\x1b[38;5;", "\x1b[38;2;", "\x1b[48;5;", "\x1b[48;2;"] {
        assert!(!raw.contains(hue), "a colour under NO_COLOR: {hue:?}");
    }
    ascii.leave();
}
