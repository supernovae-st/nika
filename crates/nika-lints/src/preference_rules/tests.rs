use super::*;
use nika_schema::expression::scan_templates;
use nika_schema::raw::{RawExecAction, RawInferAction, RawInvokeAction};
use nika_schema::source::{Span, Spanned};
use nika_schema::{FileId, ParseMode, parse};
use serde_json::{Value, json};

// Fixtures speak W2 « the flow » — `depends_on:` is a dead form
// (NIKA-SCHEMA parse error): a bare dep is `after: {x: success}` ·
// a data read is a `with:` binding consumed as `${{ with.<name> }}` ·
// `when:` never references `tasks.*` (NIKA-VAR-021). The yaml scenarios
// mirror the spec conformance corpus (`conformance/tests/lints/`).
//
// (`one-obvious-way/001` is RETIRED — its discouraged form is illegal
// now — so the /001 tests and the status-expression helpers they
// grounded are gone with it.)

// ───────────────────────── rule-level helpers ─────────────────────────

/// Parse a workflow fixture + run the full preference rule set.
fn lints_of(yaml: &str) -> Vec<Lint> {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    one_obvious_way(&wf)
}

/// Only the lints for one rule id (keeps assertions immune to other rules).
fn lints_for(yaml: &str, rule: &str) -> Vec<Lint> {
    lints_of(yaml)
        .into_iter()
        .filter(|l| l.rule == rule)
        .collect()
}

/// The `${{ … }}` islands of a raw string — what `literal_parts_use_shell`
/// consumes alongside the raw `command`.
fn islands_of(s: &str) -> Vec<TemplateIsland> {
    scan_templates(s).expect("templates scan")
}

// ─────────────────────────────────────────────────────────────────────
// rule_008_interpolated_string_command       (body→() · the ||→&& swap)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn rule_008_fires_on_interpolated_string_command() {
    // body→() — replacing the whole fn body with `()` emits no lint.
    let yaml = "\
nika: interp
tasks:
  produce:
    exec: { command: [\"./gen.sh\"] }
  consume:
    with:
      data: \"${{ tasks.produce.output }}\"
    exec: { shell: \"process ${{ with.data }}\" }
";
    let eight = lints_for(yaml, "one-obvious-way/008");
    assert_eq!(eight.len(), 1, "exactly one /008 must fire");
    assert_eq!(eight[0].task_id, "consume");
}

#[test]
fn rule_008_silent_on_a_plain_literal_command() {
    // `islands.is_empty() || literal_parts_use_shell(..)` decides
    // « skip ». For `"cargo build"`: islands EMPTY (true) · shell-meta NONE
    // (false). The correct `||` ⇒ true ⇒ skip ⇒ no lint. The `&&` mutant ⇒
    // `true && false` ⇒ false ⇒ FALL THROUGH ⇒ a spurious /008 on a command
    // that has no interpolation at all. Asserting zero kills the swap.
    let yaml = "\
nika: plain
tasks:
  build:
    exec: { command: [\"cargo\", \"build\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/008").is_empty(),
        "a non-interpolated command must never be flagged"
    );
}

#[test]
fn rule_008_silent_on_a_genuine_pipeline() {
    // The OTHER half of the `||`: islands NON-empty but the literal
    // parts carry a `|` ⇒ `literal_parts_use_shell` true ⇒ skip. (Also pins
    // the `literal_parts_use_shell` true-branch reachability.)
    let yaml = "\
nika: pipe
tasks:
  produce:
    exec: { command: [\"./gen.sh\"] }
  consume:
    with:
      data: \"${{ tasks.produce.output }}\"
    exec: { shell: \"cat ${{ with.data }} | wc -l\" }
";
    assert!(
        lints_for(yaml, "one-obvious-way/008").is_empty(),
        "a genuine shell pipeline keeps the string form"
    );
}

// ─────────────────────────────────────────────────────────────────────
// rule_009_stream_binding                                     (body→())
// ─────────────────────────────────────────────────────────────────────

#[test]
fn rule_009_fires_on_a_bare_iterator_binding() {
    // body→() — replacing the fn body with `()` emits no lint.
    let yaml = "\
nika: stream
tasks:
  fetch:
    invoke: { tool: \"nika:read\", args: { path: \"u.json\" } }
    extract:
      emails: \".users[]\"
";
    let nine = lints_for(yaml, "one-obvious-way/009");
    assert_eq!(nine.len(), 1, "exactly one /009 must fire");
    assert_eq!(nine[0].task_id, "fetch");
    assert!(nine[0].message.contains("emails"), "{}", nine[0].message);
}

// ─────────────────────────────────────────────────────────────────────
// ends_in_bare_iterator                          (→true · →false mutants)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn ends_in_bare_iterator_true_on_a_stream() {
    // `-> false` mutant: a genuine bare iterator must return TRUE.
    assert!(ends_in_bare_iterator(".users[]"));
    assert!(ends_in_bare_iterator(".[]"));
    assert!(ends_in_bare_iterator("(.a)[]"));
    assert!(ends_in_bare_iterator(".users[] ")); // trailing ws trimmed
}

#[test]
fn ends_in_bare_iterator_false_on_a_literal_or_scalar() {
    // `-> true` mutant: a non-iterator must return FALSE.
    assert!(!ends_in_bare_iterator(".a // []")); // empty-array literal default
    assert!(!ends_in_bare_iterator(".users")); // no trailing `[]`
    assert!(!ends_in_bare_iterator(".users[0]")); // indexed take
}

// ─────────────────────────────────────────────────────────────────────
// literal_parts_use_shell                                (→true mutant)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn literal_parts_use_shell_false_without_metacharacters() {
    // `-> true` mutant: an interpolated command whose literal parts
    // carry NO shell metacharacter must return FALSE (so /008 fires).
    let cmd = "process ${{ with.v }} now";
    assert!(!literal_parts_use_shell(cmd, &islands_of(cmd)));
}

#[test]
fn literal_parts_use_shell_true_with_a_pipe() {
    // The complementary TRUE case (a real pipeline) — also distinguishes
    // the fn from a constant-false would-be mutant.
    let cmd = "cat ${{ with.v }} | wc -l";
    assert!(literal_parts_use_shell(cmd, &islands_of(cmd)));
}

// ─────────────────────────────────────────────────────────────────────
// rule_002_skip_for_dependents
//   (body→() · the skip-guard `!` · the acknowledgement arms)
//   mirror: conformance/tests/lints/002-*
// ─────────────────────────────────────────────────────────────────────

#[test]
fn rule_002_fires_for_an_unguarded_dependent() {
    // body→() and the on_error-skip match guard both silence the
    // expected single /002. `b` reads `a`'s VALUE through a `with:`
    // binding and acknowledges the possible skip neither way (no
    // `after: {a: success}` tightening · no `when:` over the binding).
    let yaml = "\
nika: f002
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
    on_error: { skip: true }
  b:
    with:
      data: \"${{ tasks.a.output }}\"
    exec: { command: [\"./b.sh\", \"${{ with.data }}\"] }
";
    let two = lints_for(yaml, "one-obvious-way/002");
    assert_eq!(
        two.len(),
        1,
        "an unguarded value-reading dependent of a skip task fires /002"
    );
    assert_eq!(two[0].task_id, "a");
    assert!(two[0].message.contains('b'), "{}", two[0].message);
}

#[test]
fn rule_002_silent_when_the_dependent_tightens_the_gate() {
    // Acknowledgement arm 1 — `after: {a: success}` cancels the
    // dependent on skip, so the changed contract is decided. The
    // tightened-check mutant (dropping the Control(Succeeded) match)
    // turns this into a spurious /002.
    let yaml = "\
nika: f002ok
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
    on_error: { skip: true }
  b:
    with:
      data: \"${{ tasks.a.output }}\"
    after: { a: success }
    exec: { command: [\"./b.sh\", \"${{ with.data }}\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/002").is_empty(),
        "a dependent that tightens the gate acknowledges the skip — no /002"
    );
}

#[test]
fn rule_002_silent_when_the_when_reads_the_binding() {
    // Acknowledgement arm 2 — the dependent's `when:` references the
    // binding bound to the skippable producer (`${{ with.data != null }}`
    // — the canonical null test · spec 03 §gate algebra). Exercises
    // `when_expr`'s Some path + the `NamespaceRef::With` match; dropping
    // either turns this into a spurious /002.
    let yaml = "\
nika: f002when
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
    on_error: { skip: true }
  b:
    with:
      data: \"${{ tasks.a.output }}\"
    when: \"${{ with.data != null }}\"
    exec: { command: [\"./b.sh\", \"${{ with.data }}\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/002").is_empty(),
        "a `when:` over the binding acknowledges the skip — no /002"
    );
}

#[test]
fn rule_002_silent_without_dependents() {
    // A skip producer with NO value-reading dependent changes nobody's
    // contract — silent (the unguarded set is empty).
    let yaml = "\
nika: f002leaf
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
    on_error: { skip: true }
";
    assert!(
        lints_for(yaml, "one-obvious-way/002").is_empty(),
        "a leaf skip task has no dependent to mislead — no /002"
    );
}

// ─────────────────────────────────────────────────────────────────────
// rule_003_004_failure_guarded_tasks       (the fingerprint ==→!= swap)
//   mirror: conformance/tests/lints/003-* · 004-*
// ─────────────────────────────────────────────────────────────────────

#[test]
fn rule_003_fires_on_a_structural_duplicate() {
    // `==`→`!=` on the fingerprint compare: /003 fires when the
    // `after: {build: failure}` task's action fingerprint EQUALS the
    // guarded producer's. The mutant inverts it: identical actions ⇒ no
    // /003 (and the exec body is not a value-producer ⇒ no /004 either).
    let yaml = "\
nika: dup
tasks:
  build:
    exec: { command: [\"./build.sh\"] }
  rebuild:
    after: { build: failure }
    exec: { command: [\"./build.sh\"] }
";
    let three = lints_for(yaml, "one-obvious-way/003");
    assert_eq!(three.len(), 1, "a failure-path structural copy fires /003");
    assert_eq!(three[0].task_id, "rebuild");
}

#[test]
fn rule_003_fires_on_an_invoke_duplicate() {
    // The Invoke fingerprint arm end-to-end (mirror of the spec fixture
    // `003-fires-on-failure-guarded-duplicate`) — same tool, same args.
    let yaml = "\
nika: f003
tasks:
  a:
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://example.com/data\" }
  a_retry:
    after: { a: failure }
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://example.com/data\" }
";
    let three = lints_for(yaml, "one-obvious-way/003");
    assert_eq!(
        three.len(),
        1,
        "an identical failure-path invoke fires /003"
    );
    assert_eq!(three[0].task_id, "a_retry");
}

#[test]
fn rule_003_silent_when_bodies_differ() {
    // Real failure-path WORK (a notification · a different tool) is the
    // legitimate use of `after: {a: failure}` — neither /003 (bodies
    // differ) nor /004 (not a mere value) may fire.
    let yaml = "\
nika: f003diff
tasks:
  a:
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://example.com/data\" }
  report:
    after: { a: failure }
    invoke:
      tool: \"nika:notify\"
      args: { channel: webhook, target: \"https://hooks.example.com\", message: \"a failed\" }
";
    assert!(
        lints_for(yaml, "one-obvious-way/003").is_empty(),
        "differing bodies are not a retry re-implementation"
    );
    assert!(
        lints_for(yaml, "one-obvious-way/004").is_empty(),
        "a notify call is real failure work, not a mere value"
    );
}

#[test]
fn rule_004_fires_on_an_echo_fallback_task() {
    // A template-free argv `echo` behind `after: {a: failure}` is a mere
    // fallback VALUE — the route belongs in `a`'s `on_error: recover:`.
    let yaml = "\
nika: f004echo
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
  fallback:
    after: { a: failure }
    exec: { command: [\"echo\", \"default-value\"] }
";
    let four = lints_for(yaml, "one-obvious-way/004");
    assert_eq!(four.len(), 1, "the echo fallback task fires /004");
    assert_eq!(four[0].task_id, "fallback");
}

#[test]
fn rule_004_fires_on_a_literal_jq_fallback_task() {
    // The other conservative « mere value » shape — `nika:jq` with
    // template-free args behind the failure gate.
    let yaml = "\
nika: f004
tasks:
  a:
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://example.com/data\" }
  fallback:
    after: { a: failure }
    invoke:
      tool: \"nika:jq\"
      args: { input: { count: 0 }, expression: \".\" }
";
    let four = lints_for(yaml, "one-obvious-way/004");
    assert_eq!(four.len(), 1, "the literal jq fallback task fires /004");
    assert_eq!(four[0].task_id, "fallback");
    assert!(
        lints_for(yaml, "one-obvious-way/003").is_empty(),
        "differing bodies — no /003 alongside"
    );
}

#[test]
fn rule_004_silent_on_real_failure_work() {
    // A mirror fetch on failure is real work: not a fingerprint
    // duplicate (args differ) and not a value-producer — both rules
    // stay silent.
    let yaml = "\
nika: f004real
tasks:
  a:
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://primary.example.com\" }
  mirror:
    after: { a: failure }
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://mirror.example.com\" }
";
    assert!(
        lints_for(yaml, "one-obvious-way/004").is_empty(),
        "a mirror fetch is real failure work — no /004"
    );
    assert!(
        lints_for(yaml, "one-obvious-way/003").is_empty(),
        "differing args ⇒ differing fingerprints — no /003"
    );
}

#[test]
fn rule_004_silent_without_a_failure_gate() {
    // `if fired_003 || checked.is_empty()` → the `&&` mutant falls
    // through to `checked[0]` on an EMPTY vec ⇒ panic (a killed
    // mutant). A success-gated value-producer collects no failure
    // check ⇒ no /004 (and no /003).
    let yaml = "\
nika: succgate
tasks:
  a:
    exec: { command: [\"echo\", \"hi\"] }
  b:
    after: { a: success }
    invoke: { tool: \"nika:jq\", args: { filter: \".x\" } }
";
    assert!(
        lints_for(yaml, "one-obvious-way/004").is_empty(),
        "a success gate collects no failure check ⇒ no /004"
    );
    assert!(
        lints_for(yaml, "one-obvious-way/003").is_empty(),
        "no failure-guarded duplicate either"
    );
}

// ─────────────────────────────────────────────────────────────────────
// rule_005_cleanup_via_terminal_task
//   (body→() · the n<3 guard · the n-1 count · the others-set compare)
//   mirror: conformance/tests/lints/005-*
// ─────────────────────────────────────────────────────────────────────

#[test]
fn rule_005_fires_on_terminal_after_on_everything() {
    // body→(), the `n < 3` early-return swaps, the `!= n-1` count
    // mutants and the `terminal_targets != others` compare all silence
    // the expected single /005: a 3-task workflow whose cleanup holds
    // `after: {…: terminal}` on BOTH others fires exactly once.
    let yaml = "\
nika: f005
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
  b:
    exec: { command: [\"./b.sh\"] }
  cleanup:
    after: { a: terminal, b: terminal }
    exec: { command: [\"./cleanup.sh\"] }
";
    let five = lints_for(yaml, "one-obvious-way/005");
    assert_eq!(five.len(), 1, "the terminal cleanup fires /005");
    assert_eq!(five[0].task_id, "cleanup");
}

#[test]
fn rule_005_silent_on_a_two_task_workflow() {
    // `n < 3` → `>`: with the mutant, `2 > 3` is false ⇒ the guard does
    // NOT return ⇒ a 2-task « terminal-after-the-other » task would
    // wrongly fire /005. The correct `<` returns early (a real cleanup
    // needs ≥ 3 tasks). Asserting zero kills the swap.
    let yaml = "\
nika: f005two
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
  b:
    after: { a: terminal }
    exec: { command: [\"./b.sh\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/005").is_empty(),
        "fewer than 3 tasks is not a cleanup-of-everything"
    );
}

#[test]
fn rule_005_silent_when_terminal_after_not_on_everything() {
    // Negative control · a terminal `after:` on only ONE of two other
    // tasks ⇒ `terminal_targets != others` ⇒ no /005. Pins the count and
    // set-equality mutants from the other direction.
    let yaml = "\
nika: f005partial
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
  b:
    exec: { command: [\"./b.sh\"] }
  c:
    after: { a: terminal }
    exec: { command: [\"./c.sh\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/005").is_empty(),
        "a strict subset of terminal targets is not a cleanup-of-everything"
    );
}

#[test]
fn rule_005_silent_on_a_plain_join_task() {
    // A join that reads BOTH producers' values through `with:` bindings
    // (zero `after:` entries) is the canonical fan-in — no /005.
    let yaml = "\
nika: f005join
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
  b:
    exec: { command: [\"./b.sh\"] }
  summarize:
    with:
      left: \"${{ tasks.a.output }}\"
      right: \"${{ tasks.b.output }}\"
    exec: { command: [\"./summarize.sh\", \"${{ with.left }}\", \"${{ with.right }}\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/005").is_empty(),
        "a value-edge join is not a cleanup smuggled into the graph"
    );
}

// ─────────────────────────────────────────────────────────────────────
// rule_010_non_tightening_after
//   (body→() · the Terminal predicate match · the Value role match)
//   mirror: conformance/tests/lints/010-*
// ─────────────────────────────────────────────────────────────────────

#[test]
fn rule_010_fires_on_a_non_tightening_after() {
    // body→() emits no lint; dropping the Terminal match or the
    // Value-role match silences it too: a value edge to `a` PLUS
    // `after: {a: terminal}` composes to the value edge's own
    // {success, skipped} — the entry restates, never tightens.
    let yaml = "\
nika: f008
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
  b:
    with:
      data: \"${{ tasks.a.output }}\"
    after: { a: terminal }
    exec: { command: [\"./b.sh\", \"${{ with.data }}\"] }
";
    let ten = lints_for(yaml, "one-obvious-way/010");
    assert_eq!(ten.len(), 1, "the non-tightening after entry fires /010");
    assert_eq!(ten[0].task_id, "b");
    assert!(ten[0].message.contains("terminal"), "{}", ten[0].message);
    assert!(
        ten[0].suggestion.contains("success"),
        "the /010 suggestion names the respelling"
    );
}

#[test]
fn rule_010_fires_on_a_named_binding_edge() {
    // A NAMED `output:` binding reference (`tasks.a.summary`) is a value
    // edge too (`role_of_field`'s `_ => Value` arm) — the terminal
    // restatement beside it fires the same /010.
    let yaml = "\
nika: f010named
tasks:
  a:
    invoke: { tool: \"nika:read\", args: { path: \"u.json\" } }
    extract:
      summary: \".s\"
  b:
    with:
      s: \"${{ tasks.a.summary }}\"
    after: { a: terminal }
    exec: { command: [\"./b.sh\", \"${{ with.s }}\"] }
";
    let ten = lints_for(yaml, "one-obvious-way/010");
    assert_eq!(ten.len(), 1, "a named-binding value edge fires /010 too");
    assert_eq!(ten[0].task_id, "b");
}

#[test]
fn rule_010_silent_on_a_tightening_after() {
    // `after: {a: success}` beside the value edge TIGHTENS the gate
    // (skip no longer admits) — the canonical acknowledgement, never
    // flagged. Kills a would-be any-predicate mutant of the Terminal
    // match.
    let yaml = "\
nika: f008ok
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
  b:
    with:
      data: \"${{ tasks.a.output }}\"
    after: { a: success }
    exec: { command: [\"./b.sh\", \"${{ with.data }}\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/010").is_empty(),
        "a tightening after entry is the one way — no /010"
    );
}

#[test]
fn rule_010_silent_without_a_value_edge() {
    // A pure control dependent (`after: {a: terminal}` · no `with:`)
    // restates nothing — the terminal predicate IS the declared intent.
    let yaml = "\
nika: f008noedge
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
  b:
    after: { a: terminal }
    exec: { command: [\"./b.sh\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/010").is_empty(),
        "no value edge ⇒ nothing restated ⇒ no /010"
    );
}

#[test]
fn rule_010_silent_on_an_observation_edge() {
    // A `.status` reference is a terminal-OBSERVATION edge, not a value
    // edge — every settled state admits it, so `after: {a: terminal}`
    // beside it restates nothing the rule owns. Pins the
    // `EdgeKind::Value` role discrimination inside the rule.
    let yaml = "\
nika: f010obs
tasks:
  a:
    exec: { command: [\"./a.sh\"] }
  b:
    with:
      seen: \"${{ tasks.a.status }}\"
    after: { a: terminal }
    exec: { command: [\"./b.sh\", \"${{ with.seen }}\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/010").is_empty(),
        "an observation edge is not a value edge — no /010"
    );
}

// ─────────────────────────────────────────────────────────────────────
// rule_006_per_element_timing    (body→() · the head-check ! · ||→&&)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn rule_006_fires_on_a_timeout_wrapper_in_for_each() {
    // body→(), deleting the `!` on the head check (a `timeout`-headed
    // command wrongly `continue`s), and the `||`→`&&` swap on the
    // timeout/gtimeout pair all silence the expected single /006.
    let yaml = "\
nika: f006
tasks:
  shards:
    for_each: { items: [1, 2, 3] }
    exec: { command: [\"timeout\", \"30\", \"./process.sh\"] }
";
    let six = lints_for(yaml, "one-obvious-way/006");
    assert_eq!(six.len(), 1, "the per-element timeout wrapper fires /006");
    assert_eq!(six[0].task_id, "shards");
}

#[test]
fn rule_006_fires_on_a_gtimeout_wrapper() {
    // The `gtimeout` half of the head pair: with the `&&` mutant the
    // gtimeout line also fails to fire.
    let yaml = "\
nika: f006g
tasks:
  shards:
    for_each: { items: [1, 2, 3] }
    exec: { command: [\"gtimeout\", \"30\", \"./process.sh\"] }
";
    let six = lints_for(yaml, "one-obvious-way/006");
    assert_eq!(six.len(), 1, "the gtimeout wrapper fires /006");
    assert_eq!(six[0].task_id, "shards");
}

#[test]
fn rule_006_silent_on_a_plain_for_each_command() {
    // Negative control · a `for_each` body with no timeout wrapper ⇒ no
    // /006 (pins the head-token precision · low false-positive).
    let yaml = "\
nika: f006ok
tasks:
  shards:
    for_each: { items: [1, 2, 3] }
    timeout: 30s
    exec: { command: [\"./process.sh\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/006").is_empty(),
        "a plain for_each command is not a per-element timing trick"
    );
}

// ─────────────────────────────────────────────────────────────────────
// rule_007_manual_sharding
//   (body→() · the single-successor arm of `next` · the len<3 guard)
//   mirror: conformance/tests/lints/007-*
// ─────────────────────────────────────────────────────────────────────

#[test]
fn rule_007_fires_on_three_sequential_exec_shards() {
    // body→(), deleting the `Some(&[j])` arm of `next` (no chain edges ⇒
    // every chain is length 1 ⇒ never fires), and the `chain.len() < 3`
    // swaps: three single-producer exec shards (`after: {…: success}`
    // links) differing in one token fire exactly one /007 on the head.
    let yaml = "\
nika: f007
tasks:
  shard1:
    exec: { command: [\"./process.sh\", \"part1\"] }
  shard2:
    after: { shard1: success }
    exec: { command: [\"./process.sh\", \"part2\"] }
  shard3:
    after: { shard2: success }
    exec: { command: [\"./process.sh\", \"part3\"] }
";
    let seven = lints_for(yaml, "one-obvious-way/007");
    assert_eq!(seven.len(), 1, "the 3-shard chain fires /007 on the head");
    assert_eq!(seven[0].task_id, "shard1");
}

#[test]
fn rule_007_fires_on_three_sequential_invoke_shards() {
    // Drives the Invoke branch of `is_shard_chain` through the rule —
    // same tool, args differing in one leaf path — end-to-end (in
    // addition to the direct helper tests below).
    let yaml = "\
nika: f007invoke
tasks:
  page1:
    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.com/page/1\" } }
  page2:
    after: { page1: success }
    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.com/page/2\" } }
  page3:
    after: { page2: success }
    invoke: { tool: \"nika:fetch\", args: { url: \"https://example.com/page/3\" } }
";
    let seven = lints_for(yaml, "one-obvious-way/007");
    assert_eq!(seven.len(), 1, "the 3-invoke-shard chain fires /007");
    assert_eq!(seven[0].task_id, "page1");
}

#[test]
fn rule_007_silent_on_a_two_task_chain() {
    // `chain.len() < 3` → `>`: with the mutant `2 > 3` is false ⇒ a
    // length-2 shard chain wrongly proceeds to `is_shard_chain` (true
    // for two single-slot shards) ⇒ a spurious /007. The correct `<`
    // skips chains under 3. Asserting zero kills the swap.
    let yaml = "\
nika: f007two
tasks:
  shard1:
    exec: { command: [\"./process.sh\", \"part1\"] }
  shard2:
    after: { shard1: success }
    exec: { command: [\"./process.sh\", \"part2\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/007").is_empty(),
        "a 2-task chain is below the ≥3 shard threshold"
    );
}

#[test]
fn rule_007_silent_on_a_genuine_pipeline() {
    // Negative control · 3 chained tasks that are NOT the same operation
    // (different programs / token counts) ⇒ `is_shard_chain` false ⇒ no
    // /007.
    let yaml = "\
nika: f007pipe
tasks:
  fetch:
    exec: { command: [\"./fetch.sh\"] }
  parse:
    after: { fetch: success }
    exec: { command: [\"./parse.sh\", \"--strict\", \"input.json\"] }
  report:
    after: { parse: success }
    exec: { command: [\"./report.sh\"] }
";
    assert!(
        lints_for(yaml, "one-obvious-way/007").is_empty(),
        "a genuine pipeline is not manual sharding"
    );
}

// ───────────────────────── direct-construction helpers ─────────────────────────
//
// `is_value_producer` / `is_shard_chain` / `leaf_paths` / `differing_leaf_paths`
// / `action_fingerprint` take `&RawAction` / `&[&RawTask]` / `&Value`, not a
// parsed string — so we build the AST nodes directly (same crate · the
// `#[non_exhaustive]` ratchet only binds external crates).

/// A zero-span `Spanned<String>`.
fn sp(s: &str) -> Spanned<String> {
    Spanned::new(s.to_owned(), Span::default())
}

/// An `exec:` action with a shell-string command.
fn exec_cmd(cmd: &str) -> RawAction {
    RawAction::Exec(RawExecAction::new(sp(cmd)))
}

/// An `invoke:` action — `tool` + optional template-args JSON.
fn invoke_tool(tool: &str, args: Option<Value>) -> RawAction {
    let mut a = RawInvokeAction::new(sp(tool));
    a.args = args.map(|v| Spanned::new(v, Span::default()));
    RawAction::Invoke(a)
}

/// A bare `infer:` action (never a value-producer, never a shard).
fn infer_prompt(prompt: &str) -> RawAction {
    RawAction::Infer(Box::new(RawInferAction::new(sp(prompt))))
}

/// A task carrying an action — id is irrelevant to the helpers under test.
fn task_with(id: &str, action: RawAction) -> RawTask {
    RawTask::new(sp(id), action)
}

// ─────────────────────────────────────────────────────────────────────
// action_fingerprint                          (body → Default::default())
// ─────────────────────────────────────────────────────────────────────

#[test]
fn action_fingerprint_is_a_real_structural_print() {
    // body → `Default::default()` (⇒ `Value::Null`): the print must
    // NOT be null, must carry the verb, and must DISTINGUISH two different
    // actions (the whole point — rule 003/007 compare prints for equality).
    let fp_exec = action_fingerprint(&exec_cmd("./build.sh"));
    assert_ne!(fp_exec, Value::Null);
    assert_eq!(fp_exec.get("verb").and_then(Value::as_str), Some("exec"));

    let fp_invoke = action_fingerprint(&invoke_tool("nika:read", None));
    assert_ne!(fp_exec, fp_invoke, "different verbs ⇒ different prints");

    // identical exec bodies ⇒ identical prints (the equality rule 003 uses).
    assert_eq!(fp_exec, action_fingerprint(&exec_cmd("./build.sh")));
    // different commands ⇒ different prints.
    assert_ne!(fp_exec, action_fingerprint(&exec_cmd("./other.sh")));
}

// ─────────────────────────────────────────────────────────────────────
// is_value_producer
//   (→false/→true · the jq-tool guard · the `!contains("${{")` closures
//    · the Exec arm deletion · the echo-head &&→|| swap)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn value_producer_jq_without_templates_is_one() {
    // →false, the jq guard →false and the `==`→`!=` tool compare: a
    // `nika:jq` with template-free args IS a value-producer. All three
    // mutants make this return false.
    assert!(is_value_producer(&invoke_tool(
        "nika:jq",
        Some(json!({ "filter": ".name" }))
    )));
    // and with NO args at all (is_none_or short-circuits true).
    assert!(is_value_producer(&invoke_tool("nika:jq", None)));
}

#[test]
fn value_producer_jq_with_templates_is_not_one() {
    // Deleting the `!` in `!serialized.contains("${{")`: jq args carrying
    // `${{ … }}` ⇒ contains ⇒ `!` ⇒ false (NOT a value-producer).
    // Deleting the `!` inverts it ⇒ wrongly a value-producer.
    assert!(!is_value_producer(&invoke_tool(
        "nika:jq",
        Some(json!({ "filter": "${{ tasks.a.output }}" }))
    )));
}

#[test]
fn value_producer_non_jq_invoke_is_not_one() {
    // Forcing the `tool == "nika:jq"` guard true makes ANY invoke take
    // the jq arm. A `nika:fetch` with no args would then return true
    // (is_none_or). The correct guard rejects it ⇒ false.
    assert!(!is_value_producer(&invoke_tool("nika:fetch", None)));
}

#[test]
fn value_producer_echo_exec_is_one() {
    // Deleting the `Exec` arm (⇒ `_ => false`): a template-free `echo`
    // command IS a value-producer. Deleting the arm returns false.
    assert!(is_value_producer(&exec_cmd("echo hello")));
    // leading whitespace is trimmed before the `echo ` check.
    assert!(is_value_producer(&exec_cmd("   echo hi")));
}

#[test]
fn value_producer_non_echo_exec_is_not_one() {
    // `&&` → `||` in `starts_with("echo ") && !contains("${{")`.
    // For `"ls -la"`: starts_with=false · `!contains`=true. Correct `&&`
    // ⇒ false. The `||` mutant ⇒ true ⇒ wrongly a value-producer.
    assert!(!is_value_producer(&exec_cmd("ls -la")));
}

#[test]
fn value_producer_echo_with_template_is_not_one() {
    // Deleting the second `!`: for `echo ${{ … }}`: starts_with=true ·
    // contains=true ⇒ `!`=false ⇒ true && false = false (NOT a value-
    // producer · the value is interpolated work). Deleting the `!` ⇒
    // true && true = true ⇒ wrongly a value-producer.
    assert!(!is_value_producer(&exec_cmd("echo ${{ tasks.a.output }}")));
}

#[test]
fn value_producer_argv_and_infer_are_not_value_producers() {
    // The catch-all `_ => false` path: an `infer` action is never a mere
    // value. The argv law flipped with spec#78 fixture 004: `["echo", …]`
    // with template-free elements IS a value producer now — and an
    // interpolated element or a non-echo program still is not.
    assert!(!is_value_producer(&infer_prompt("summarize this")));
    let echo = RawAction::Exec(RawExecAction::with_command(
        nika_schema::raw::action::RawCommand::Argv(vec![sp("echo"), sp("hi")]),
    ));
    assert!(is_value_producer(&echo), "template-free argv echo = value");
    let interp = RawAction::Exec(RawExecAction::with_command(
        nika_schema::raw::action::RawCommand::Argv(vec![sp("echo"), sp("${{ tasks.a.output }}")]),
    ));
    assert!(!is_value_producer(&interp), "interpolation = real work");
    let prog = RawAction::Exec(RawExecAction::with_command(
        nika_schema::raw::action::RawCommand::Argv(vec![sp("./gen.sh")]),
    ));
    assert!(!is_value_producer(&prog), "non-echo argv = real work");
}

// ─────────────────────────────────────────────────────────────────────
// is_shard_chain
//   exec branch (→false/→true · Exec-arm deletion · the len guards ·
//                the varying-position insert)
//   invoke branch (Invoke-arm deletion · the tool guard · varying==1)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn is_shard_chain_exec_three_token_single_varying_slot() {
    // →false, deleting the Exec arm (⇒ `_=>false`), the mixed-verb
    // `tokens.len() != actions.len()` guard swap, the differing-length
    // `t.len() != len` guard swap, and `varying.len() == 1` → `!=`: a
    // genuine exec shard chain (3 tokens · ONLY the last varies) returns
    // true. Three tokens (not two) is required for the insert swap below.
    let tasks = [
        task_with("s1", exec_cmd("./p a x")),
        task_with("s2", exec_cmd("./p a y")),
        task_with("s3", exec_cmd("./p a z")),
    ];
    let refs: Vec<&RawTask> = tasks.iter().collect();
    assert!(is_shard_chain(&refs, &[0, 1, 2]));
}

#[test]
fn is_shard_chain_exec_inserts_varying_not_equal_positions() {
    // `if a != b` ⇒ `==`: the loop records VARYING token positions.
    // With three tokens where pos 0+1 are constant (`./p`, `a`) and pos 2
    // varies, the correct set is {2} (len 1 ⇒ true). The `==` mutant records
    // the EQUAL positions {0, 1} (len 2 ⇒ false). Two equal positions are
    // needed so the mutant's set size ≠ 1 (a 2-token chain would give the
    // equal-set size 1 too and mask the swap).
    let tasks = [
        task_with("s1", exec_cmd("./p a x")),
        task_with("s2", exec_cmd("./p a y")),
        task_with("s3", exec_cmd("./p a z")),
    ];
    let refs: Vec<&RawTask> = tasks.iter().collect();
    assert!(is_shard_chain(&refs, &[0, 1, 2]));

    // negative control · TWO token positions vary ⇒ not a single-slot shard.
    let multi = [
        task_with("s1", exec_cmd("./p a x")),
        task_with("s2", exec_cmd("./p b y")),
        task_with("s3", exec_cmd("./p c z")),
    ];
    let multi_refs: Vec<&RawTask> = multi.iter().collect();
    assert!(!is_shard_chain(&multi_refs, &[0, 1, 2]));
}

#[test]
fn is_shard_chain_rejects_a_genuine_pipeline() {
    // →true: different programs / differing token counts is NOT a
    // shard chain. A forced-true body wrongly accepts it.
    let tasks = [
        task_with("fetch", exec_cmd("./fetch.sh")),
        task_with("parse", exec_cmd("./parse.sh --strict input.json")),
        task_with("report", exec_cmd("./report.sh")),
    ];
    let refs: Vec<&RawTask> = tasks.iter().collect();
    assert!(!is_shard_chain(&refs, &[0, 1, 2]));
}

#[test]
fn is_shard_chain_invoke_same_tool_single_varying_leaf() {
    // Deleting the Invoke arm (⇒ `_=>false`), the `inv.tool != first.tool`
    // mixed-tool guard swap, and `varying.len() == 1` → `!=`: same tool,
    // args differing in exactly ONE leaf path (`/url`) ⇒ true.
    let tasks = [
        task_with(
            "p1",
            invoke_tool("nika:fetch", Some(json!({ "url": "https://x/1" }))),
        ),
        task_with(
            "p2",
            invoke_tool("nika:fetch", Some(json!({ "url": "https://x/2" }))),
        ),
        task_with(
            "p3",
            invoke_tool("nika:fetch", Some(json!({ "url": "https://x/3" }))),
        ),
    ];
    let refs: Vec<&RawTask> = tasks.iter().collect();
    assert!(is_shard_chain(&refs, &[0, 1, 2]));
}

#[test]
fn is_shard_chain_invoke_rejects_mixed_tools() {
    // The `!=` → `==` tool-guard complement: DIFFERENT tools across the
    // chain ⇒ the guard must return false. With `==` the same-tool guard
    // inverts and a mixed-tool chain would slip through — pin the correct
    // false here.
    let tasks = [
        task_with(
            "p1",
            invoke_tool("nika:fetch", Some(json!({ "url": "https://x/1" }))),
        ),
        task_with(
            "p2",
            invoke_tool("nika:read", Some(json!({ "url": "https://x/2" }))),
        ),
        task_with(
            "p3",
            invoke_tool("nika:fetch", Some(json!({ "url": "https://x/3" }))),
        ),
    ];
    let refs: Vec<&RawTask> = tasks.iter().collect();
    assert!(!is_shard_chain(&refs, &[0, 1, 2]));
}

// ─────────────────────────────────────────────────────────────────────
// leaf_paths            (body→() · Object-arm deletion · Array-arm deletion)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn leaf_paths_recurses_into_objects() {
    // body → `()` (⇒ nothing inserted) AND deleting the `Object` arm
    // (⇒ the whole object is inserted at the prefix instead of recursing).
    // Correct: an object yields one key per field (`/a`, `/b`), NOT a single
    // `""` → {whole object}.
    let mut out = BTreeMap::new();
    leaf_paths(&json!({ "a": 1, "b": 2 }), "", &mut out);
    let keys: Vec<&String> = out.keys().collect();
    assert_eq!(keys, vec!["/a", "/b"]);
    assert_eq!(out.get("/a"), Some(&json!(1)));
    assert_eq!(out.get("/b"), Some(&json!(2)));
}

#[test]
fn leaf_paths_recurses_into_arrays() {
    // Deleting the `Array` arm (⇒ the whole array inserted at the
    // prefix). Correct: an array yields one key per index (`/0`, `/1`).
    let mut out = BTreeMap::new();
    leaf_paths(&json!([10, 20]), "", &mut out);
    let keys: Vec<&String> = out.keys().collect();
    assert_eq!(keys, vec!["/0", "/1"]);
    assert_eq!(out.get("/0"), Some(&json!(10)));
}

// ─────────────────────────────────────────────────────────────────────
// differing_leaf_paths
//   (body → empty/`[""]`/`["xyzzy"]` constants · the `!=`→`==` filter)
// ─────────────────────────────────────────────────────────────────────

#[test]
fn differing_leaf_paths_reports_the_changed_path_only() {
    // The constant-body mutants (`BTreeSet::new()` / `[""]` / `["xyzzy"]`):
    // two objects sharing `/x` and differing at `/u` ⇒ the differing set is
    // EXACTLY {`/u`}. The empty-set mutant gives {} · the `[""]` mutant gives
    // {""} · the `["xyzzy"]` mutant gives {"xyzzy"} — all ≠ {"/u"}.
    // The `ma.get != mb.get` → `==` filter swap would keep the EQUAL path
    // `/x` instead ⇒ {"/x"}.
    let a = json!({ "x": 1, "u": "p1" });
    let b = json!({ "x": 1, "u": "p2" });
    let diff = differing_leaf_paths(&a, &b);
    let expected: BTreeSet<String> = ["/u".to_string()].into_iter().collect();
    assert_eq!(diff, expected);
}

#[test]
fn differing_leaf_paths_empty_when_identical() {
    // Complement of the filter swap — identical values share every path, so
    // the correct `!=` filter keeps NONE. (The `==` mutant would keep them
    // all.)
    let a = json!({ "x": 1, "u": "p1" });
    assert!(differing_leaf_paths(&a, &a).is_empty());
}
