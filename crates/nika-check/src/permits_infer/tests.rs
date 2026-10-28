// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The capability-inference battery — a child module of [`super`], so it
//! runs under `--lib`. It lives in its own file because `permits_infer.rs`
//! plus the battery crosses the 1500-line cap (the `permits_fit` precedent).

use super::*;
use nika_schema::parser::{ParseMode, parse};
use nika_schema::source::FileId;
use proptest::prelude::*;

/// The round trip that keeps the tool's own advice honest: a boundary
/// this module WRITES must satisfy the lane that judges it. An
/// interpreter needs its script readable, so a block granting the
/// program and not the script would self-refuse the very workflow it
/// came from — the chart/tts sibling law, one boundary over.
///
/// MEASURED 2026-08-20 · without the read the leg exits 126, empty
/// stdout, and the run still renders it as a completed task.
#[test]
fn an_inferred_boundary_grants_the_script_its_interpreter_opens() {
    let wf = parse(
        "\
nika: t
tasks:
  leg:
    exec: { command: [\"bash\", \"scripts/deploy.sh\"] }
",
        FileId::new(0),
        ParseMode::Strict,
    )
    .expect("parses");
    let inferred = infer(&wf);
    let fs = inferred.permits.fs.as_ref().expect("an fs block");
    assert!(
        fs.read.iter().any(|r| r == "scripts/deploy.sh"),
        "the script must be granted: {:?}",
        fs.read
    );
    // and the boundary it wrote must survive the lane that judges it
    assert!(
        inferred.permits.jail_admits_read("scripts/deploy.sh"),
        "an inferred boundary must not self-refuse"
    );
}

/// A `cwd:` re-anchors the script; a COMPUTED one makes it unknowable,
/// and a guess there would write a grant the author never meant.
#[test]
fn the_inferred_script_read_follows_a_literal_cwd_and_stops_at_a_computed_one() {
    let literal = parse(
        "\
nika: t
tasks:
  leg:
    exec: { command: [\"bash\", \"inner.sh\"], cwd: \"sub\" }
",
        FileId::new(0),
        ParseMode::Strict,
    )
    .expect("parses");
    let fs = infer(&literal).permits.fs.expect("an fs block");
    assert!(fs.read.iter().any(|r| r == "sub/inner.sh"), "{:?}", fs.read);

    let computed = parse(
        "\
nika: t
const:
  where: \"sub\"
tasks:
  leg:
    exec: { command: [\"bash\", \"inner.sh\"], cwd: \"${{ const.where }}\" }
",
        FileId::new(0),
        ParseMode::Strict,
    )
    .expect("parses");
    let reads = infer(&computed)
        .permits
        .fs
        .map(|f| f.read)
        .unwrap_or_default();
    assert!(
        reads.is_empty(),
        "a computed cwd must pin nothing: {reads:?}"
    );
}

/// The silences · a non-interpreter positional is the program's own
/// business, and inline eval opens no file.
#[test]
fn the_inferred_script_read_claims_nothing_it_cannot_know() {
    for yaml in [
        "nika: t\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi.txt\"] }\n",
        "nika: t\ntasks:\n  t:\n    exec: { command: [\"python3\", \"-m\", \"unittest\"] }\n",
        "nika: t\ntasks:\n  t:\n    exec: { shell: \"bash leg.sh\" }\n",
    ] {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parses");
        let reads = infer(&wf).permits.fs.map(|f| f.read).unwrap_or_default();
        assert!(reads.is_empty(), "silence expected for {yaml}: {reads:?}");
    }
}

/// The inference covers chart + tts (they were invisible — the
/// boundary it wrote refused the very run it came from) and the
/// chart vega sibling rides along.
#[test]
fn chart_and_tts_infer_their_writes() {
    let wf = nika_schema::parser::parse(
        "\
nika: t
model: mock/echo
tasks:
  c:
    invoke:
      tool: \"nika:chart\"
      args:
        data: [{ x: \"a\", y: 1 }]
        chart: { type: bar, x: x, y: y }
        out: \"out/c.svg\"
        compile_to: vega_lite
  s:
    invoke:
      tool: \"nika:tts_generate\"
      args:
        text: \"hi\"
        output_dir: \"audio\"
",
        nika_schema::source::FileId::new(0),
        nika_schema::parser::ParseMode::Strict,
    )
    .expect("parse");
    let inferred = infer(&wf);
    let yaml = inferred.to_yaml();
    assert!(
        yaml.contains("out/c.svg"),
        "chart artifact inferred: {yaml}"
    );
    assert!(
        yaml.contains("out/c.vl.json"),
        "vega sibling inferred: {yaml}"
    );
    assert!(
        yaml.contains("audio/**"),
        "tts dir inferred recursive: {yaml}"
    );
}

/// The per-task projector: each family flattens deterministically,
/// dynamic exec widens, unpinnable effects project NOTHING, and a
/// bare infer task is empty (the wire's []).
#[test]
fn task_permits_attributes_each_family_deterministically() {
    let yaml = "\
nika: t
model: mock/echo
tasks:
  fetcher:
    invoke:
      tool: nika:fetch
      args:
        url: https://api.acme-widgets.org/items
  writer:
    invoke:
      tool: \"nika:write\"
      args:
        path: out/report.md
        content: hi
  lister:
    exec:
      command: [\"ls\", \"-la\"]
  sheller:
    exec:
      shell: \"echo hi && ls\"
  thinker:
    infer:
      prompt: p
  looper:
    agent:
      prompt: g
      tools: [\"nika:done\", \"nika:log\"]
      max_turns: 2
";
    let wf = nika_schema::parser::parse(
        yaml,
        nika_schema::source::FileId::new(0),
        nika_schema::parser::ParseMode::Strict,
    )
    .expect("fixture parses");
    let by_id = |id: &str| {
        let t = wf
            .tasks
            .iter()
            .find(|t| t.value.id.value == id)
            .expect("task exists");
        task_permits(&t.value)
    };

    assert_eq!(
        by_id("fetcher"),
        vec!["net.http: api.acme-widgets.org", "tool: nika:fetch"],
    );
    assert_eq!(
        by_id("writer"),
        vec!["fs.write: out/report.md", "tool: nika:write"],
    );
    assert_eq!(by_id("lister"), vec!["exec: ls"]);
    // Shell strings can never satisfy a program allowlist — widened.
    assert_eq!(by_id("sheller"), vec!["exec: true"]);
    // A bare infer has no pinnable effect — the wire's empty list.
    assert!(by_id("thinker").is_empty());
    // Agent tools are boundary vocabulary, BTree-ordered.
    assert_eq!(by_id("looper"), vec!["tool: nika:done", "tool: nika:log"]);
}

fn infer_of(yaml: &str) -> InferredPermits {
    infer(&parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse"))
}

proptest! {
    // The security-critical property: ANY literal `nika:read` path —
    // including quotes, backslashes, control chars marked-yaml admits
    // in a double-quoted scalar — renders to a `permits:` block that
    // PARSES and admits the workflow. A broken escape would either
    // fail to parse (the operator can't paste it) or silently shift
    // the boundary (the grant no longer matches the path).
    #[test]
    fn inferred_block_round_trips_for_arbitrary_literal_paths(
        path in "[a-z0-9 ./\"\\\\\t]{1,40}",
    ) {
        // build a read on this literal path via a JSON-escaped scalar
        let esc = path.replace('\\', "\\\\").replace('"', "\\\"").replace('\t', "\\t");
        let yaml = format!(
            "nika: w\ntasks:\n  t:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{esc}\" }} }}\n"
        );
        // only proceed if the SOURCE parses (some byte seqs are not
        // valid scalars — that's the parser's domain, not ours)
        let Ok(src_wf) = parse(&yaml, FileId::new(0), ParseMode::Strict) else {
            return Ok(());
        };
        // Escaping paths are never inferred (persona 7 · G-09 shovel);
        // the round-trip claim is for workspace-relative grants.
        if path_escapes_workspace(&path) {
            return Ok(());
        }
        let r = infer(&src_wf);
        // the rendered block must itself parse + admit the workflow
        let (head, tail) = yaml.split_once("tasks:").expect("has tasks");
        let spliced = format!("{head}{}tasks:{tail}", r.to_yaml());
        let wf = parse(&spliced, FileId::new(0), ParseMode::Strict)
            .expect("inferred block must parse");
        prop_assert!(wf.permits.is_some());
        prop_assert!(
            super::super::permits_fit::scan_escapes(&wf).is_empty(),
            "boundary must admit the path it was inferred from"
        );
    }
}

/// Splice the inferred block into the workflow and assert the re-check
/// reports ZERO capability escapes — the round-trip property.
fn assert_round_trips_clean(yaml: &str) {
    let r = infer_of(yaml);
    let (head, tail) = yaml
        .split_once("tasks:")
        .expect("test workflow has a tasks: block");
    let spliced = format!("{head}{}tasks:{tail}", r.to_yaml());
    let wf =
        parse(&spliced, FileId::new(0), ParseMode::Strict).expect("inferred permits block parses");
    assert!(wf.permits.is_some(), "the rendered block is a permits:");
    let escapes = super::super::permits_fit::scan_escapes(&wf);
    assert!(
        escapes.is_empty(),
        "inferred boundary must admit the workflow, got: {escapes:?}"
    );
}

#[test]
fn pure_compute_workflow_infers_empty_boundary() {
    // infer-only, no effects → exec:false, no fs/net/tools.
    let r = infer_of(
        "nika: w\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 10 }\n",
    );
    assert_eq!(r.permits.exec, Some(ExecPermit::No));
    assert!(r.permits.fs.is_none() && r.permits.net.is_none());
    assert!(r.permits.tools.is_none());
    assert!(r.notes.is_empty());
}

/// Spec 02 §Harness images · the inferred boundary names every file an `images.save` can
/// write (the first and its `-n` siblings), so it re-checks clean.
#[test]
fn an_images_save_infers_every_path_it_can_write() {
    let yaml = "nika: w\ntasks:\n  draw:\n    agent:\n      prompt: draw\n      images: { save: ./out/a.png }\n";
    let r = infer_of(yaml);
    let fs = r.permits.fs.as_ref().expect("fs");
    assert_eq!(
        fs.write,
        vec![
            "./out/a-2.png",
            "./out/a-3.png",
            "./out/a-4.png",
            "./out/a.png"
        ]
    );
    assert!(fs.read.is_empty());
    assert_round_trips_clean(yaml);
}

/// A rendered save cannot be pinned: the fs face is named partial and the note names the
/// task, never an empty write set presented as the answer (nor « pure compute »).
#[test]
fn a_rendered_images_save_marks_fs_partial() {
    let r = infer_of(
        "nika: w\ninputs:\n  out: { type: string, required: true }\ntasks:\n  draw:\n    agent:\n      prompt: draw\n      images: { save: \"${{ inputs.out }}\" }\n",
    );
    assert!(r.partial.fs, "{:?}", r.notes);
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("`draw`") && n.contains("dynamic path")),
        "{:?}",
        r.notes
    );
    assert!(r.permits.fs.is_none(), "{:?}", r.permits.fs);
}

/// Every literal `nika:image_compose` input is a read the inferred
/// boundary grants, and `out:` the write (the C-D1 gap — a read check
/// never modelled — is not repeated for the composition).
#[test]
fn image_compose_infers_each_input_as_a_read_and_out_as_a_write() {
    let yaml = "nika: w\ntasks:\n  s:\n    invoke: { tool: \"nika:image_compose\", args: { inputs: [\"./fx/a.png\", \"./fx/b.png\"], out: \"./out/sheet.png\" } }\n";
    let r = infer_of(yaml);
    let fs = r.permits.fs.as_ref().expect("fs");
    assert_eq!(fs.read, vec!["./fx/a.png", "./fx/b.png"]);
    assert_eq!(fs.write, vec!["./out/sheet.png"]);
    assert!(r.notes.is_empty(), "every path was literal: {:?}", r.notes);
    assert_round_trips_clean(yaml);
}

/// A templated list hides the read set: the fs face is named partial,
/// never inferred empty.
#[test]
fn a_templated_image_compose_list_marks_fs_partial() {
    let r = infer_of(
        "nika: w\nconst:\n  frames: [\"./fx/a.png\"]\ntasks:\n  s:\n    invoke: { tool: \"nika:image_compose\", args: { inputs: \"${{ const.frames }}\", out: \"./out/sheet.png\" } }\n",
    );
    assert!(r.partial.fs, "a hidden read set is a partial fs face");
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("`s`") && n.contains("dynamic path")),
        "{:?}",
        r.notes
    );
    assert_eq!(
        r.permits.fs.as_ref().expect("fs").write,
        vec!["./out/sheet.png"]
    );
}

/// Spec 02 §Vision inputs · a `vision:` file is a read: the inferred boundary grants every
/// literal or bare-const path (so the printed block re-checks clean) and nothing for a
/// URL, which the provider fetches.
#[test]
fn a_vision_file_infers_a_read_and_a_url_needs_no_grant() {
    let yaml = "nika: w\nconst:\n  legend: \"./diagrams/legend.png\"\ntasks:\n  look:\n    infer:\n      prompt: \"compare\"\n      vision:\n        - { source: file, path: \"./diagrams/flow.png\" }\n        - { source: file, path: \"${{ const.legend }}\" }\n        - { source: url, url: \"https://example.com/chart.png\" }\n";
    let r = infer_of(yaml);
    let fs = r.permits.fs.as_ref().expect("a vision file is an fs read");
    assert_eq!(
        fs.read,
        vec!["./diagrams/flow.png", "./diagrams/legend.png"]
    );
    assert!(fs.write.is_empty());
    assert!(r.permits.net.is_none(), "Nika never fetches a vision URL");
    assert!(r.permits.tools.is_none(), "the read rides no tool grant");
    assert!(!r.partial.any(), "{:?}", r.notes);
    assert_round_trips_clean(yaml);
}

/// A rendered vision path cannot be pinned: the fs face is named partial and the note names
/// the task, never an empty read set presented as the answer. A path that leaves the
/// workspace is noted, never pasted as a grant.
#[test]
fn a_dynamic_or_escaping_vision_path_marks_fs_partial() {
    let rendered = infer_of(
        "nika: w\ninputs:\n  pic: { type: string, required: true }\ntasks:\n  look:\n    infer:\n      prompt: \"describe\"\n      vision:\n        - { source: file, path: \"${{ inputs.pic }}\" }\n",
    );
    assert!(rendered.partial.fs, "{:?}", rendered.notes);
    assert!(
        rendered
            .notes
            .iter()
            .any(|n| n.contains("`look`") && n.contains("dynamic path")),
        "{:?}",
        rendered.notes
    );
    let escaping = infer_of(
        "nika: w\ntasks:\n  look:\n    infer:\n      prompt: \"describe\"\n      vision:\n        - { source: file, path: \"../shared/flow.png\" }\n",
    );
    assert!(escaping.partial.fs);
    assert!(
        escaping.permits.fs.is_none(),
        "an escaping path is never inferred: {:?}",
        escaping.permits.fs
    );
    assert!(
        escaping
            .notes
            .iter()
            .any(|n| n.contains("`../shared/flow.png`") && n.contains("escapes the workspace")),
        "{:?}",
        escaping.notes
    );
}

#[test]
fn literal_effects_infer_a_tight_boundary() {
    let r = infer_of(
        "nika: w\ntasks:\n  rd:\n    invoke: { tool: \"nika:read\", args: { path: \"./data/in.json\" } }\n  get:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.x.com/v1\" } }\n  build:\n    exec: { command: [\"cargo\", \"build\"] }\n",
    );
    assert_eq!(
        r.permits.exec,
        Some(ExecPermit::Programs(vec!["cargo".to_owned()]))
    );
    assert_eq!(
        r.permits.fs.as_ref().expect("fs").read,
        vec!["./data/in.json"]
    );
    assert_eq!(r.permits.net.as_ref().expect("net").http, vec!["api.x.com"]);
    let tools = r.permits.tools.as_ref().expect("tools");
    assert!(tools.contains(&"nika:read".to_owned()));
    assert!(tools.contains(&"nika:fetch".to_owned()));
    assert!(r.notes.is_empty(), "all effects were literal");
}

#[test]
fn dynamic_exec_widens_to_any_with_a_note() {
    // A dynamic SHELL string rides the shell-string arm (the form
    // decides before the head is even looked at).
    let r = infer_of(
        "nika: w\nconst: { c: \"git\" }\ntasks:\n  t:\n    exec: { shell: \"${{ const.c }} status\" }\n",
    );
    assert_eq!(r.permits.exec, Some(ExecPermit::Any), "dynamic → true");
    assert_eq!(r.notes.len(), 1);
    assert!(r.notes[0].contains("shell-string"));
}

#[test]
fn literal_shell_string_never_infers_a_self_refusing_allowlist() {
    // The trap this pins: `shell: "git log"` used to infer
    // `exec: ["git"]` — a boundary the runtime then REFUSES for the
    // very task it was inferred from (shell-string under a Programs
    // allowlist is rejected wholesale at dispatch). The sound
    // inference is `exec: true` + a rewrite-to-argv note.
    let r = infer_of("nika: w\ntasks:\n  t:\n    exec: { shell: \"git log\" }\n");
    assert_eq!(
        r.permits.exec,
        Some(ExecPermit::Any),
        "shell string → true, never a leading-token allowlist"
    );
    assert!(r.notes.iter().any(|n| n.contains("array form")));
}

#[test]
fn dynamic_argv_head_widens_to_any_not_a_garbage_literal() {
    // `["${{ inputs.bin }}"]` must NOT be inferred as a literal program
    // named `${{ inputs.bin }}` — the head is dynamic → exec: true + note.
    let r = infer_of(
        "nika: w\ninputs: { bin: { type: string, default: git } }\ntasks:\n  t:\n    exec: { command: [\"${{ inputs.bin }}\", \"status\"] }\n",
    );
    assert_eq!(r.permits.exec, Some(ExecPermit::Any));
    assert!(r.notes.iter().any(|n| n.contains("dynamic exec")));
}

#[test]
fn dynamic_fetch_url_notes_review() {
    let r = infer_of(
        "nika: w\nconst: { h: \"x.com\" }\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://${{ const.h }}/p\" } }\n",
    );
    // host couldn't be pinned → net stays unset, a note flags it
    assert!(r.permits.net.is_none());
    assert!(r.notes.iter().any(|n| n.contains("dynamic URL")));
}

#[test]
fn edit_grep_and_webhook_notify_are_classified() {
    let yaml = "nika: w\ntasks:\n  fix:\n    invoke: { tool: \"nika:edit\", args: { path: \"./README.md\", find: \"a\", replace: \"b\" } }\n  scan:\n    invoke: { tool: \"nika:grep\", args: { pattern: \"TODO\", path: \"./src\" } }\n  ping:\n    invoke: { tool: \"nika:notify\", args: { channel: \"webhook\", target: \"https://hooks.slack.com/x\", message: \"hi\" } }\n";
    let r = infer_of(yaml);
    let fs = r.permits.fs.as_ref().expect("fs");
    // edit reads then rewrites the same path
    assert_eq!(fs.read, vec!["./README.md", "./src/**"]);
    assert_eq!(fs.write, vec!["./README.md"]);
    // webhook notify is a net egress to the target host
    assert_eq!(
        r.permits.net.as_ref().expect("net").http,
        vec!["hooks.slack.com"]
    );
    assert_round_trips_clean(yaml);
}

#[test]
fn image_generate_infers_a_recursive_write_grant() {
    // Assets + the manifest land INSIDE output_dir — the inference
    // grants `<dir>/**` on fs.write (the grep-recursive analog, write
    // side), and the provider egress is deliberately NOT a net grant
    // (the image plane rides engine transport, like `infer:`).
    let yaml = "nika: w\ntasks:\n  og:\n    invoke: { tool: \"nika:image_generate\", args: { prompt: \"hero\", output_dir: \"./assets/og\" } }\n";
    let r = infer_of(yaml);
    let fs = r.permits.fs.as_ref().expect("fs");
    assert!(fs.read.is_empty(), "generation reads nothing");
    assert_eq!(fs.write, vec!["./assets/og/**"]);
    assert!(
        r.permits.net.is_none(),
        "provider egress ≠ permits.net.http"
    );
    assert!(
        r.permits
            .tools
            .as_ref()
            .expect("tools")
            .contains(&"nika:image_generate".to_owned())
    );
    assert!(r.notes.is_empty(), "literal dir → no review note");
    assert_round_trips_clean(yaml);
}

/// A `const:`-backed dir RESOLVES · a runtime-supplied one does not.
///
/// This test used to call the const case "dynamic" and assert that
/// inference could not pin it. That premise was wrong on the same
/// point the FIT scan was wrong on: `--var` satisfies `inputs:` and
/// nothing else (measured — on a file whose `p` is a const, `nika run
/// --var p=X` answers « this workflow declares no inputs »), so a
/// const cannot move between check and run and is therefore statically
/// known.
///
/// The consequence of the old reading was a tool contradicting itself:
/// `check` called a const-backed path a literal path and judged it,
/// while `--infer-permits` called the same path too dynamic to pin and
/// omitted it — so the block it printed never round-tripped clean on
/// any file routing a path through `const:`, which is every file the
/// templates teach.
///
/// THE AUTHORITY IS THE BOUNDARY, and this pins it in both directions.
#[test]
fn a_const_dir_resolves_and_a_runtime_one_notes_review() {
    let konst = "nika: w\nconst: { dir: \"./assets\" }\ntasks:\n  og:\n    invoke: { tool: \"nika:image_generate\", args: { prompt: \"hero\", output_dir: \"${{ const.dir }}\" } }\n";
    let r = infer_of(konst);
    assert!(
        r.permits.fs.is_some(),
        "a const-backed dir is statically known: {:?}",
        r.permits.fs
    );
    assert_round_trips_clean(konst);

    // `inputs:` is genuinely dynamic even WITH a default — the run can
    // supply another value, and a boundary drafted against a value the
    // run may replace is exactly the claim this tool must not make.
    let runtime = "nika: w\ninputs:\n  dir:\n    type: string\n    default: \"./assets\"\ntasks:\n  og:\n    invoke: { tool: \"nika:image_generate\", args: { prompt: \"hero\", output_dir: \"${{ inputs.dir }}\" } }\n";
    let r = infer_of(runtime);
    assert!(r.permits.fs.is_none(), "a runtime dir cannot be pinned");
    assert!(r.notes.iter().any(|n| n.contains("dynamic path")));
}

#[test]
fn non_webhook_notify_is_not_a_net_effect() {
    let r = infer_of(
        "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:notify\", args: { channel: \"email\", target: \"ops@x.com\", message: \"hi\" } }\n",
    );
    assert!(
        r.permits.net.is_none(),
        "email rides an engine transport, not a workflow host grant"
    );
}

#[test]
fn rendered_yaml_round_trips_through_the_parser() {
    // The inferred block must itself parse AND admit the workflow.
    assert_round_trips_clean(
        "nika: w\ntasks:\n  rd:\n    invoke: { tool: \"nika:read\", args: { path: \"./data/x\" } }\n  build:\n    exec: { command: [\"cargo\", \"test\"] }\n",
    );
}

#[test]
fn exec_false_and_exec_true_round_trip() {
    // ExecPermit::No — a pure-invoke workflow renders `exec: false`.
    assert_round_trips_clean(
        "nika: w\ntasks:\n  rd:\n    invoke: { tool: \"nika:read\", args: { path: \"./x\" } }\n",
    );
    // ExecPermit::Any — a dynamic command renders `exec: true`.
    assert_round_trips_clean(
        "nika: w\nconst: { c: \"git\" }\ntasks:\n  t:\n    exec: { shell: \"${{ const.c }} status\" }\n",
    );
}

#[test]
fn agent_tool_globs_round_trip() {
    let yaml = "nika: w\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:fetch\", \"mcp:browser/*\"]\n";
    let r = infer_of(yaml);
    let tools = r.permits.tools.as_ref().expect("tools");
    assert!(tools.contains(&"mcp:browser/*".to_owned()));
    assert_eq!(r.permits.exec, Some(ExecPermit::No), "no exec task");
    assert_round_trips_clean(yaml);
}

#[test]
fn quotes_and_backslashes_in_paths_render_valid_yaml() {
    // An unescaped `"` would render a structurally broken block — the
    // review's PROVEN parse failure.
    let yaml = "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:read\", args: { path: \"data/he said \\\"hi\\\".json\" } }\n";
    assert_round_trips_clean(yaml);
    let r = infer_of(yaml);
    assert!(r.to_yaml().contains(r#"\"hi\""#), "quote is escaped");
}

#[test]
fn empty_permits_renders_the_explicit_empty_mapping() {
    // A bare `permits:` is YAML null and the parser rejects it.
    let rendered = render_yaml(&Permits::new());
    assert_eq!(rendered, "permits: {}\n");
    let full = format!(
        "nika: w\n{rendered}tasks:\n  t:\n    infer: {{ prompt: \"hi\", max_tokens: 5 }}\n"
    );
    assert!(
        parse(&full, FileId::new(0), ParseMode::Strict).is_ok(),
        "the empty mapping form parses"
    );
}

#[test]
fn a_documentation_host_is_granted_and_the_block_re_checks_clean() {
    // The first draft of this change withheld RFC 2606/6761 hosts from
    // the grants; the E-diff soundness property refused it at once
    // (`Fetch("api.example.com")` — the PERMITS rung still read the
    // host as outside `permits.net.http`). The boundary is uniform: the
    // runtime allowlist judges the host before the transport declines
    // to dial it, so the inference grants it, marks nothing partial,
    // and the round trip stays clean. The never-dialed fact rides the
    // `documentation-host` hint of the very same check (wave 3 ·
    // persona 04).
    const TASKS: &str = "tasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.com/x\" } }\n";
    let r = infer_of(&format!("nika: w\n{TASKS}"));
    let net = r
        .permits
        .net
        .as_ref()
        .expect("a documentation host is granted like any boundary member");
    assert_eq!(net.http, vec!["example.com".to_owned()]);
    assert!(!r.partial.net, "the block is complete: {:?}", r.partial);
    assert!(r.notes.is_empty(), "{:?}", r.notes);

    let round_trip = format!("nika: w\n{}{TASKS}", r.to_yaml());
    let report = crate::check(
        &parse(&round_trip, FileId::new(0), ParseMode::Strict).expect("round trip parses"),
    );
    assert!(
        report.capability_escapes.iter().all(|e| e.floor),
        "the inferred block re-checks clean: {:?}",
        report.capability_escapes
    );
    assert!(
        report
            .hints
            .iter()
            .any(|h| h.kind == "documentation-host" && h.task == "a"),
        "the hint names the host the run will not dial: {:?}",
        report.hints
    );
}

#[test]
fn floor_blocked_host_is_never_inferred_into_the_grants() {
    // A loopback fetch must NOT synthesize `net.http: ["127.0.0.1"]`
    // — even though the exact literal WOULD declassify the floor
    // (#395): the explicit act must stay the author's, so the
    // inference keeps its hands off and the note TEACHES the opt-in.
    // Public hosts still infer.
    let r = infer_of(
        "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://127.0.0.1:9/x\" } }\n  b:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.acme-widgets.com/x\" } }\n",
    );
    let net = r.permits.net.as_ref().expect("public host infers net");
    assert_eq!(net.http, vec!["api.acme-widgets.com".to_owned()]);
    assert_eq!(r.notes.len(), 1, "{:?}", r.notes);
    assert!(r.notes[0].contains("SSRF floor"), "{}", r.notes[0]);
    assert!(r.notes[0].contains("`127.0.0.1`"), "{}", r.notes[0]);
    assert!(
        r.notes[0].contains("explicit act"),
        "the loopback note teaches the opt-in: {}",
        r.notes[0]
    );
}

#[test]
fn an_escaping_host_path_is_never_inferred_into_the_grants() {
    // Persona 7 · 2026-08-22: `--infer-permits` printed
    // `fs.read: ["/etc/passwd"]`, the author pasted it, check+run
    // greened. Absolute / home-relative paths stay a note.
    let r = infer_of(
        "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:read\", args: { path: \"/etc/passwd\" } }\n  b:\n    invoke: { tool: \"nika:read\", args: { path: \"./notes.md\" } }\n",
    );
    let fs = r.permits.fs.as_ref().expect("workspace path infers fs");
    assert_eq!(fs.read, vec!["./notes.md".to_owned()]);
    assert!(
        !fs.read.iter().any(|p| p.contains("passwd")),
        "host file must not be in the paste block: {:?}",
        fs.read
    );
    assert!(r.partial.fs, "the escaping face is incomplete");
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("/etc/passwd") && n.contains("never the printed repair")),
        "{:?}",
        r.notes
    );
    let home = infer_of(
        "nika: w\ntasks:\n  t:\n    invoke: { tool: \"nika:read\", args: { path: \"~/.ssh/id_rsa\" } }\n",
    );
    assert!(
        home.permits.fs.as_ref().is_none_or(|f| f.read.is_empty()),
        "home-relative secrets are not inferred: {:?}",
        home.permits.fs
    );
    assert!(home.partial.fs);
}

#[test]
fn non_loopback_floor_note_never_teaches_the_opt_in() {
    // The never-list keeps the pre-#395 wording: no entry can admit a
    // metadata/RFC1918 target, so the note must NOT hint one.
    let r = infer_of(
        "nika: w\ntasks:\n  a:\n    invoke: { tool: \"nika:fetch\", args: { url: \"http://169.254.169.254/latest/meta-data/\" } }\n",
    );
    assert_eq!(r.notes.len(), 1, "{:?}", r.notes);
    assert!(
        r.notes[0].contains("no `permits.net.http` entry can admit it"),
        "{}",
        r.notes[0]
    );
    assert!(
        !r.notes[0].contains("explicit act"),
        "never-list targets get no opt-in hint: {}",
        r.notes[0]
    );
}

/// nika#1597 · an argv's literal URL reaches its host exactly like an
/// invoke's, and the exec net arm of the fit lane judges it. MEASURED on
/// 0.124.0: the block printed for `curl https://example.com/` granted
/// `exec: ["curl"]` alone, and `nika check` refused that very block
/// with `NIKA-SEC-004 · net`.
#[test]
fn an_exec_argv_literal_url_infers_its_host() {
    let yaml = "nika: w\ntasks:\n  get:\n    exec: { command: [\"curl\", \"-s\", \"https://example.com/\"] }\n";
    let r = infer_of(yaml);
    let net = r.permits.net.as_ref().expect("a net block");
    assert_eq!(net.http, vec!["example.com"]);
    assert!(!r.partial.net, "{:?}", r.notes);
    assert_round_trips_clean(yaml);
}

/// A bare const-backed URL is static to the fit lane, so it is static
/// here too (the `const:` asymmetry this collector already closed for
/// paths).
#[test]
fn a_const_backed_exec_url_infers_its_host() {
    let yaml = "nika: w\nconst:\n  site: \"https://api.example.com/v1\"\ntasks:\n  get:\n    exec: { command: [\"curl\", \"${{ const.site }}\"] }\n";
    let r = infer_of(yaml);
    let net = r.permits.net.as_ref().expect("a net block");
    assert_eq!(net.http, vec!["api.example.com"]);
    assert_round_trips_clean(yaml);
}

/// The shell line is scanned for standalone URL tokens by the fit lane
/// once its form is admissible (`exec: true`, the widening this module
/// writes for it), so the host rides the same block.
#[test]
fn a_shell_line_literal_url_infers_its_host() {
    let yaml = "nika: w\ntasks:\n  get:\n    exec: { shell: \"curl -s https://example.com/ | head -1\" }\n";
    let r = infer_of(yaml);
    assert_eq!(r.permits.exec, Some(ExecPermit::Any));
    let net = r.permits.net.as_ref().expect("a net block");
    assert_eq!(net.http, vec!["example.com"]);
    assert_round_trips_clean(yaml);
}

/// nika#1597 · the reported case: an HTTP client handed its URL by the
/// caller. No static read names that host, and the run's egress proxy
/// admits only `permits.net.http`, so the face is partial and the note
/// names the task and the program — never a complete-looking block the
/// run then refuses.
#[test]
fn a_computed_url_handed_to_an_http_client_marks_net_partial() {
    let r = infer_of(
        "nika: w\ninputs:\n  url: { type: string, required: true }\ntasks:\n  get:\n    exec: { command: [\"curl\", \"-s\", \"${{ inputs.url }}\"] }\n",
    );
    assert!(r.partial.net, "{:?}", r.notes);
    assert!(r.permits.net.is_none(), "{:?}", r.permits.net);
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("`get`") && n.contains("`curl`") && n.contains("permits.net.http")),
        "{:?}",
        r.notes
    );
}

/// A URL the argv already pins is the host; a computed option beside
/// it (an output path, a header) is not a second, unknown host.
#[test]
fn a_computed_option_beside_a_literal_url_pins_the_host_only() {
    let yaml = "nika: w\ninputs:\n  out: { type: string, required: true }\ntasks:\n  get:\n    exec: { command: [\"curl\", \"-o\", \"${{ inputs.out }}\", \"https://example.com/\"] }\n";
    let r = infer_of(yaml);
    let net = r.permits.net.as_ref().expect("a net block");
    assert_eq!(net.http, vec!["example.com"]);
    assert!(!r.partial.net, "{:?}", r.notes);
}

/// A computed argument to a program that is not an HTTP client says
/// nothing about hosts: the net face stays complete (and empty).
#[test]
fn a_computed_argument_to_another_program_leaves_net_complete() {
    let r = infer_of(
        "nika: w\ninputs:\n  msg: { type: string, required: true }\ntasks:\n  save:\n    exec: { command: [\"git\", \"commit\", \"-m\", \"${{ inputs.msg }}\"] }\n",
    );
    assert!(!r.partial.net, "{:?}", r.notes);
    assert!(r.permits.net.is_none(), "{:?}", r.permits.net);
}

/// A URL whose AUTHORITY is computed cannot be pinned by any program:
/// the scheme says a host is reached, the template says which. WHATWG
/// admits `$`, `{` and `}` in a domain, so the spelling without spaces
/// parses with the template text as its host; that text is never granted.
#[test]
fn an_exec_url_with_a_computed_authority_marks_net_partial() {
    for url in [
        "https://${{ inputs.host }}/r.git",
        "https://${{inputs.host}}/r.git",
    ] {
        let yaml = format!(
            "nika: w\ninputs:\n  host: {{ type: string, required: true }}\ntasks:\n  pull:\n    exec: {{ command: [\"git\", \"clone\", \"{url}\"] }}\n"
        );
        let r = infer_of(&yaml);
        assert!(r.partial.net, "{url}: {:?}", r.notes);
        assert!(r.permits.net.is_none(), "{url}: {:?}", r.permits.net);
        assert!(
            r.notes.iter().any(|n| n.contains("`pull`")),
            "{url}: {:?}",
            r.notes
        );
        assert_round_trips_clean(&yaml);
    }
}

/// A template ahead of the delimiter that closes the authority can
/// still move the host (a value `x@evil.example/` in the password): the
/// parsed host is granted, since the fit lane judges it, and the face
/// stays partial.
#[test]
fn a_template_in_the_userinfo_keeps_the_net_face_partial() {
    let yaml = "nika: w\ninputs:\n  pw: { type: string, required: true }\ntasks:\n  get:\n    exec: { command: [\"curl\", \"https://u:${{ inputs.pw }}@example.com/\"] }\n";
    let r = infer_of(yaml);
    let net = r.permits.net.as_ref().expect("a net block");
    assert_eq!(net.http, vec!["example.com"]);
    assert!(r.partial.net, "{:?}", r.notes);
    assert_round_trips_clean(yaml);
}

/// The SSRF floor refuses a private target whatever `permits:` says, so
/// an exec URL there is noted, never written into the grants (the same
/// law as an invoke's).
#[test]
fn a_floor_blocked_exec_host_is_noted_never_inferred() {
    let r = infer_of(
        "nika: w\ntasks:\n  get:\n    exec: { command: [\"curl\", \"https://10.0.0.8/x\"] }\n",
    );
    assert!(r.permits.net.is_none(), "{:?}", r.permits.net);
    assert!(r.partial.net);
    assert!(
        r.notes
            .iter()
            .any(|n| n.contains("10.0.0.8") && n.contains("SSRF floor")),
        "{:?}",
        r.notes
    );
}
