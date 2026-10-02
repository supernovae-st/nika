# Nika TUI: reception criteria

Reception judges one exact candidate commit of the terminal renderer (`crates/nika-tui`) and of its door in
`nika`, without modifying it. This page maps the seven journeys of the TUI mandate, plus a first-contact journey
on the real binary, to checks that decide: PTY proofs, tmux cases and manual steps, each with a pass criterion and
a status.

Written on 29 September 2026 against base `513ca8465`. The measurements quoted come from that base, on one macOS
machine (tmux 3.6b) under a load average of about 27, with development-profile binaries. They are evidence at that
revision, not a standing performance claim.

## How a check runs

| Kind | Where | Command |
|---|---|---|
| PTY proof | `crates/nika-tui/tests/qa_*.rs` | `cargo test -p nika-tui --locked` |
| Defect or gap check | the same files, `#[ignore = "defect: …"]` or `#[ignore = "gap: …"]` | `cargo test -p nika-tui --locked -- --ignored` |
| Latency probe | `tests/qa_latency.rs` | `cargo test -p nika-tui --test qa_latency --locked -- --ignored --nocapture` |
| Real binary | `tests/qa_journey0.rs` | `NIKA_TUI_QA_NIKA=<candidate nika> cargo test -p nika-tui --test qa_journey0 --locked -- --ignored` |
| tmux | `docs/qa/tui/tmux_matrix.py` | `python3 docs/qa/tui/tmux_matrix.py --binary <nika-tui-proto> --out <dir>` |
| Manual | this page | the exact steps given with the check |

A defect or gap check fails on the base by construction. A candidate that fixes the defect makes its check pass
under `--ignored`; the reception records this and a follow-up commit removes the `ignore`. A check
is never deleted to go green.

Status tags: **now** decidable on the base; **pending:X** waits for the named component's implementation;
**contract:X** waits for the named typed contract; **binary** needs the candidate's own `nika`.

Severity of a finding: **P0** a law is broken (a decision taken without a fresh human act, a terminal left
broken, input lost, a secret shown); **P1** a journey cannot be completed, or a false fact is shown; **P2**
degraded but honest; **P3** cosmetic.

## The harness

`tests/qa_support/mod.rs` starts a binary on a real PTY whose size is set before the binary runs (`stty`, then
`exec`, so the first paint knows it), keeps every byte written and feeds a screen model. Every wait is bounded
(20 s) and dumps the screen, the history tail and the stream tail when it expires.

`tests/qa_support/vt.rs` is that screen model: cursor addressing, erase, scroll regions with `SU`/`SD`, `SGR`, DEC
private modes with the alternate screen, OSC titles; wide glyphs on two cells, combining marks on the cell before;
a history fed by scroll regions that start at the top row. It answers the device-attributes and cursor-position
queries from its own state, as a VT220 without a keyboard protocol would. Its limits: no per-cell attributes
(colour and weight are counted per stream), no reflow on resize (xterm keeps rows and cuts columns), and `ED 2`
does not save the screen into the history (tmux's `scroll-on-clear` does).

`tests/qa_support/child.rs` runs the real shell (`nika_tui::app::run`) over a conversation the proof controls,
inside the test executable re-invoked on a PTY, as nika-cli's `tests/tui_run_cost.rs` does: a first turn that
stays busy until the proof releases it and ends on a gate or a free prompt, an opening of N transcript lines, a
flood of busy labels.

`nika-tui-proto` walks `Script::demo`: intent, question (`reply ›`), proposal (`apply? ›`), saved (`nika ›`), run,
gate (`answer ›`), result.

## Laws every candidate keeps

| Law | Proofs | At `513ca8465` |
|---|---|---|
| The terminal is restored on every exit path | `tests/pty_restore.rs` (normal close inline and focus, two `Ctrl+C`, panic inline, `SIGTERM` focus, paste and switch, pipe); `qa_lifecycle`: `sigterm_restores_the_inline_terminal_and_leaves_with_143`, `a_panic_in_focus_leaves_the_alternate_screen_before_the_message`, `two_control_c_in_focus_leave_with_130_and_restore`, `a_sigint_signal_arms_then_leaves_like_two_control_c`, `sighup_restores_and_leaves_with_129` | pass, except `SIGHUP` (defect) |
| A paste is data | `qa_input::a_multi_line_paste_stays_data_at_every_waiting_state` | pass |
| Typeahead never decides | `qa_input::typeahead_never_answers_a_question_painted_after_it`, `typeahead_never_consents_to_a_proposal_painted_after_it`, `typeahead_never_answers_a_gate_painted_after_it`; `qa_busy::typeahead_during_a_busy_turn_never_answers_the_gate_it_ends_on`; `qa_journey0::typeahead_never_picks_an_intelligence_on_a_screen_painted_after_it`. Controls: `typeahead_into_a_free_prompt_is_kept_as_an_unsent_draft`, `keys_typed_during_a_busy_turn_are_kept_for_after_it`, `a_fresh_answer_after_the_gate_is_painted_is_heard`, `a_gate_that_asks_for_fresh_input_drops_the_typeahead` | **fail, P0** (all four decision states); controls pass |
| Plain, pipe and `TERM=dumb` stay untouched | `pty_restore::a_pipe_is_refused_with_exit_2_and_no_escape_sequence`, `qa_lifecycle::term_dumb_on_a_real_terminal_is_refused_without_an_escape_sequence`; nika-cli `tui_pty.rs`: `term_dumb_opens_the_plain_session_without_escape_sequences`, `tui_on_a_pipe_keeps_the_concierge_and_writes_no_escape_sequence` | pass |
| Meaning survives `NO_COLOR` and ASCII | `qa_layout::without_colour_no_hue_is_painted_and_the_marks_remain`, `with_colour_the_gate_wears_the_warning_slot`, `a_forced_colour_under_no_color_is_the_colour_the_terminal_sees` (defect); ASCII: J6.5 | pass; forced colour fails (P3); ASCII not reachable from the proto |
| At 80x24 the composer comes first | `qa_layout::*_journey_fits_*` (every size) | pass; B1 (defect, J6.1) |
| Motion is finite, cancellable, reduced-motion equivalent, silent when idle | `qa_lifecycle::an_idle_inline_prompt_writes_nothing_for_five_seconds`, `an_idle_focus_screen_after_a_turn_writes_nothing_for_five_seconds`, `every_effect_ends_in_silence`, `a_focus_report_costs_at_most_one_frame_then_silence` | pass; the motion effects join `every_effect_ends_in_silence` |
| No invented progress, cost or seal | J3.7 honesty assertions | pending:rendering, pending:run-display |

The typeahead law, as amended on 29 September: after a turn that ends on a decision (proposal, gate, question,
choice), the keys typed before it was painted go back to the draft; Enter, submit and history recall typed
before it are dropped; one dim notice says so. The two spending questions keep their discard. The proofs above
assert the first two parts; the notice wording belongs to the input-handling implementation.

## J0 · First contact on the real binary

| ID | Check | Kind | Pass criterion | Status |
|---|---|---|---|---|
| J0.1 | Bare `nika` at 80x24 in an empty temporary project: temporary `HOME`, environment cleared (no key), keychain off, run keys absent, `PATH=/usr/bin:/bin`, every `NIKA_<ID>_BASE_URL` pointed at a counting loopback listener. Intent, the intelligence choice, `4` (No AI), then `Ctrl+C` twice | `qa_journey0::first_contact_at_80x24_asks_the_intelligence_and_calls_no_provider` | the choice names its four options; after `4` a question (`reply ›`) or the free prompt returns; exit 130, terminal restored; **zero** listener connections; nothing addressed past 80x24 | binary: pass at the base (0 connections; with No AI the intent is refused with its fix, `✖ no conversational intelligence … /intelligence to choose a path`) |
| J0.2 | The intent and a `4` in one write, before the choice is painted | `qa_journey0::typeahead_never_picks_an_intelligence_on_a_screen_painted_after_it` | the choice still waits, `4` in the draft | **binary: fails at the base (P0)**: the typed `4` answers the screen and the choice is kept (`intelligence: no conversational AI · … · kept`) before the human saw it |
| J0.3 | tmux capture of J0.1: open, choice, question, exit | manual, same environment as J0.1 (with and without the base-URL listener), `tmux new-session -x 80 -y 24` then `capture-pane -p` after each step | the place line names what is absent (`no git`, `no nika.yaml`) instead of inventing it; the choice's header stays readable; nothing painted under the shell prompt after the exit (B4) | binary: at the base the first screen says only `Nika · project`; at 80x24 the choice's header has scrolled away (B1's blank rows); the live area stays painted after the exit (B4); the choice lists five local engines as available with none configured (the census is presence-only; Session text, shown verbatim) |
| J0.4 | One real proposal at 80x24 | manual: with a local model, or record why no offline path produces a proposal | the proposal's top is still readable when `apply? ›` shows (the real review is 25 to 40 rows) | binary |
| J0.5 | A `mock/echo` run to a `nika:prompt` gate, the answer, the result, `/proof` | manual in tmux on a checked workflow | the gate names its effect; only a fresh answer resumes; the result card's cost is known, partial or unknown, never `$0` for unknown; `/proof` verbatim | binary, pending:run-display, pending:rendering |

## J1 · A → B → A during a stream

One conversation per process exists at the base, and every key typed during a turn waits for the turn to end
(J2.6), so this journey is not reachable yet.

| ID | Check | Kind | Pass criterion | Status |
|---|---|---|---|---|
| J1.1 | While A's turn is busy, switch to B, type a draft, switch back to A | PTY on the child host with two conversations | A's reply lands in A only; B's draft is intact; nothing is sent to the wrong thread; each switch is drawn within one frame | contract:conversation-switching (ProjectSessions), pending:input |
| J1.2 | A's gate arrives while B is shown | same | shown as waiting « elsewhere » with A's name; nothing typed in B answers it | contract:conversation-switching, pending:input |
| J1.3 | Switch presentation (`Ctrl+T`) during a busy turn | `qa_busy` gap family (J2.6) | the switch happens at once | now: fails (gap) |

## J2 · Input during activity

| ID | Check | Kind | Pass criterion | Status |
|---|---|---|---|---|
| J2.1 | A three-line draft (`Alt+Enter`, `Ctrl+J`) | `qa_input::a_multi_line_draft_is_sent_whole` | one turn; the echo keeps three lines | now: pass |
| J2.2 | A bracketed paste of `yes`, `run it`, `/quit` at free, question, proposal, saved and gate | `qa_input::a_multi_line_paste_stays_data_at_every_waiting_state` | the lines land in the composer; nothing acts within 500 ms; the draft can be erased | now: pass |
| J2.3 | A decision typed before its question, proposal or gate is painted | the three `qa_input::typeahead_never_*` proofs | the decision still waits; the typed words are back in the draft; their Enter is dropped | **now: fail (P0)** |
| J2.4 | The same during a genuinely busy turn | `qa_busy::typeahead_during_a_busy_turn_never_answers_the_gate_it_ends_on` | same | **now: fail (P0)** |
| J2.5 | Typeahead into a free prompt | `typeahead_into_a_free_prompt_is_kept_as_an_unsent_draft`, `keys_typed_during_a_busy_turn_are_kept_for_after_it` | kept as an unsent draft, never lost | now: pass |
| J2.6 | Typing shows during a turn | `qa_busy::keys_typed_during_a_busy_turn_show_at_once` | a key typed while the spinner turns shows within 300 ms | now: fails (gap: `run_turn` defers every key) |
| J2.7 | Scrolling during a turn | `qa_busy::scrolling_during_a_busy_turn_moves_the_view_at_once` | `PgUp` moves the focus transcript within 300 ms | now: fails (gap) |
| J2.8 | A resize storm while typing (from 110x36 through six sizes, one every six characters) | `qa_input::a_resize_storm_while_typing_loses_no_character_inline`, `…_in_focus` | all 36 characters land in order; the line sent is whole; no absolute move past the settled screen | now: pass |
| J2.9 | A resize during a turn | `qa_busy::a_resize_during_a_busy_turn_redraws_at_the_new_size_inline`, `…_in_focus` | the busy screen is laid out again at 80x24 while the turn runs | now: pass |
| J2.10 | Wide and combining glyphs | `qa_input::wide_and_combining_glyphs_go_through_whole` | CJK, an emoji and a combining accent are drawn and sent whole | now: pass |
| J2.11 | A word wider than the composer | `qa_input::a_word_wider_than_the_composer_stays_visible` | a 108-character word wraps and stays visible, the cursor with it | **now: fails (P1)**: the composer uses `WrapMode::Word`, whose words wider than the viewport are not split; `WordOrGlyph` falls back to graphemes |
| J2.12 | History recall | `composer.rs` unit tests | `Up`/`Down` recall only at the buffer's edges, the draft kept | now: pass (unit) |

## J3 · Preparing B while A runs; inspecting parent, child, fanout, retry, agent

Everything here needs the typed run feed (`src/runs/` frames) and a live composer during a turn
(J2.6).

| ID | Check | Kind | Pass criterion | Status |
|---|---|---|---|---|
| J3.1 | The digest-notes run folded from typed frames (read, draft, gate, write) | run-projection unit test | per task: verb, transitions, attempt, duration, cost known/partial/unknown; no string parsed | pending:run-display |
| J3.2 | Draft the next intent while a run is live, then answer its gate | PTY on the real binary, a `mock/echo` workflow with a `nika:prompt` gate, tapped runner | the draft is taken during the run; only a line typed after the gate is painted answers it | binary, pending:run-display, J2.6 |
| J3.3 | Parent and child (`invoke: { workflow: … }`) | same | the child's tasks under the parent; costs aggregated then filtered, never counted twice; the child's trace reachable | pending:run-display, contract:run-frames |
| J3.4 | Fanout (`for_each` over 10 000 items) | same | one aggregate row and a bounded window of items; errors filterable; never 10 000 cards | pending:run-display |
| J3.5 | Retry (`retry: { max_attempts: 3 }` on a failing task) | same | « attempt N of M » and each attempt's error code; no ETA | pending:run-display |
| J3.6 | Agent (`agent:` with `max_turns`) | same | activity only on observed events; after 2 s without one, a still marker and « waiting on <seat> · Ns since last word » | pending:run-display, pending:motion |
| J3.7 | Honesty assertions | TestBackend and PTY | no « sealed » without a `run_sealed` frame; no `$0` for an unknown cost; no four ✓ at proposal time; no verb hue on a static row; no butterfly in the scrollback; no hint naming a key that does nothing | pending:rendering, pending:run-display, pending:motion |

## J4 · Resume, disconnect, lease, expired gate, uncertain effect, missing or changed file, masked secret, revoked context

| ID | Check | Kind | Pass criterion | Status |
|---|---|---|---|---|
| J4.1 | `SIGHUP` (the terminal hung up, a killed tmux pane, an SSH drop) | `qa_lifecycle::sighup_restores_and_leaves_with_129` | exit 129 through the same restore path as `SIGTERM`, so the session ends cleanly | **now: fails (P2)**: the process dies of the signal; `events.rs` watches only `SIGTERM` and `SIGINT` |
| J4.2 | Resume a paused gate after closing | manual on the real binary: run a `mock/echo` workflow to its `nika:prompt` gate, `Ctrl+C` twice, reopen `nika` in the same project | the gate waits again with its question and `answer ›`; the answer resumes the same trace | binary |
| J4.3 | A session held elsewhere | manual: two `nika` in the same project, two tmux panes | the second names the holder and offers to wait or read; never a second writer | binary, contract:nika-session |
| J4.4 | An expired gate (its trace no longer carries the pause) | manual | named expired, never answerable | contract:nika-session (typed `pending_gate`) |
| J4.5 | An uncertain effect | nika-cli `tui_run_cost::uncertain_dispatch_blocks_a_new_run_without_automatic_retry` (session side), then the result card | « uncertain », never « done »; no automatic retry | pending:rendering, pending:run-display |
| J4.6 | A produced file missing or changed after the run | file-view unit tests (present, missing, changed, unknown kept distinct); manual: delete `./digest.md`, then `/proof` | missing is not empty is not unknown; « changed » only from a typed identity verdict | pending:file-view, contract:nika-session |
| J4.7 | A masked secret | a workflow with `secrets:` referenced by a task, through proposal, check and result | the secret shows by its name only; a protected value is never drawn in clear | pending:file-view, binary |
| J4.8 | A revoked context source | a source removed between two turns | shown unavailable, never reused | contract:conversation-switching |

## J5 · ARM and target limits, honest

| ID | Check | Kind | Pass criterion | Status |
|---|---|---|---|---|
| J5.1 | Readiness to arm | the arm output verbatim inside a frame until a typed readiness view exists | a missing input, an unsupported policy, an absent host, a time zone, a missed slot are each named; nothing reads « armed » without the typed fact | contract:nika-session, nika-arm |
| J5.2 | A run refused by its target limits | nika-cli `tui_pty::a_refused_run_names_its_reason_inside_the_viewport` | the refusal names its reason inside the viewport | binary (existing proof) |

## J6 · Terminal matrix

| ID | Check | Kind | Pass criterion | Status |
|---|---|---|---|---|
| J6.1 | 80x24, 100x32, 120x40, 160x48, inline and focus | `qa_layout::{inline,focus}_journey_fits_{80x24,100x32,120x40,160x48}` | every prompt on screen; the gate and its hint whole; the prompt row in the bottom half; the focus rule spans the width; nothing addressed past the edge; no bell for a short turn; terminal restored | now: pass |
| J6.1b | The live area's height | `qa_layout::inline_hint_sits_right_under_the_composer` (B1), `a_human_line_is_echoed_with_one_marker` (B2), `focus_keeps_the_lines_of_one_run_together` (B3), `leaving_inline_clears_the_live_area` (B4) | the hint right under the composer; one marker per echo; one run story without blank rows; nothing of the live area left after an exit | now: fail (pending:rendering) |
| J6.2 | tmux panes of 79x23 and 80x23 | `qa_layout::*_journey_fits_a_79x23_pane`, `*_an_80x23_pane`; tmux `split-79x23-*`, `split-80x23-*` (real vertical splits of a 23-row window) | as J6.1 | now: pass; `RUN` / `READY` breaks across rows at 79 and 80 columns (known, D3, pending:rendering) |
| J6.3 | Resize and copy | J2.8, J2.9; tmux `resize-120-80-{inline,focus}-joined` | after 120x40 → 80x24, `capture-pane -pJ` keeps `9f3c1a`, `./notes/lundi.md` and `./digest.md` whole; no `│` copied | now: pass |
| J6.3b | A waiting decision survives a narrowing | `qa_layout::a_width_shrink_keeps_the_waiting_proposal_on_screen`; tmux `resize-120-80-inline-joined` (its `proposal-80x24` capture) | after 120 → 80 columns the proposal is still on screen above `apply? ›` | **now: fails (P1)**: ratatui clears the whole screen on a horizontal shrink and moves the viewport to row 0; only `apply? ›` and its hint stay, the proposal survives only where the terminal saves a cleared screen (tmux `scroll-on-clear`) |
| J6.4 | Unicode and CJK | J2.10, J2.11; manual: macOS Japanese input in Terminal and iTerm2 | the candidate window sits at the caret | now (PTY); manual |
| J6.5 | ASCII | the proto's `--ascii` and `nika --ascii`: every byte of every frame ≤ `0x7F` except the Session's own words | pending:input, pending:run-display |
| J6.6 | `NO_COLOR` | the law above | now |
| J6.7 | Reduced motion | the proto's `--reduced-motion`, `NIKA_REDUCED_MOTION` | the busy marker stays still; the facts stay (« since HH:MM:SS », the leave hint, never « 0s »); no bell | pending:input, pending:motion |
| J6.8 | Esc under tmux | tmux `esc-then-x-*`, `esc-startup-window-*`, `split-arrow-*` (a real client through a nested tmux, typing 7 s after attach) | measurement for D7, below | now: measured |
| J6.9 | SSH | manual: `ssh -t localhost`, run the proto, resize the window, type `~.` | the resize is followed; `~.` exits through the `SIGHUP` path (J4.1) | manual |
| J6.10 | Plain | the law above | now |
| J6.11 | `TERM` variants | tmux `term-tmux-256color`, `term-screen-256color` | the journey is unchanged | now: pass |
| J6.12 | Focus reports | `qa_lifecycle::a_focus_report_costs_at_most_one_frame_then_silence` | at most 64 bytes per report, then silence | now: pass |

**Esc under tmux (D7), measured at the base on tmux 3.6b** (defaults: `escape-time` 10, `extended-keys` off). In
focus, Esc then `x` typed 50 ms later: at `escape-time` 10 the full screen is left and `x` lands in the inline
composer; at `escape-time` 500 tmux merges them into Alt+x, the full screen is not left and the `x` is swallowed
(the composer inserts no Alt+character); `extended-keys` on or off changes nothing. A 700 ms gap at 500 behaves
like 10. An Up arrow whose bytes arrive 50 ms apart (a slow link splits `ESC` from `[A`): at `escape-time` 10,
**the full screen is left and `[A` leaks into the draft**; at 500 it parses as Up. During the first seconds after a
client attaches, while tmux waits for its terminal to answer its queries, tmux holds a lone Esc for at least
500 ms whatever `escape-time` says (616 ms measured at 10). So with tmux's default, a split arrow mis-parses into
Esc and leaves the full screen: the measured condition for stopping the Esc ladder at the composer and keeping
`Ctrl+T` as the only presentation switch (input handling; `tests/pty_restore.rs` then gets its explicit patch).

## J7 · Large runs

| ID | Check | Kind | Pass criterion | Status |
|---|---|---|---|---|
| J7.1 | 10 000 transcript blocks in focus: echo latency | probe `focus 120x40 · 10000 transcript lines` | the idle thresholds below | **now: fails (P1)**: p50 353 ms, p95 899 ms, p99 1 380 ms, 220 ms of CPU per keystroke: the focus transcript wraps every block on every frame |
| J7.2 | 10 000 blocks committed inline at open | probe `inline 80x24 · 10000 transcript lines` | first prompt ≤ 2 s; the echo unaffected afterwards | now, measured: 1 371 ms at load 27, 17 163 ms at load 39 (848 KiB written either way); echo p95 7.8 ms; decide on a release build |
| J7.3 | 100 000 busy labels in one turn | probe `busy row flooded` | the turn ends when its work ends; bounded bytes | now: 50 ms, 3 KiB (427 ms, 2 KiB at load 39) |
| J7.4 | 10 000 fanout items as typed frames | run projection | the fold stays under one frame per batch; an aggregate view | pending:run-display |
| J7.5 | 100 000 typed events | run projection | resident memory bounded; input p99 during the burst within the thresholds | pending:run-display |
| J7.6 | Select an error during a burst | run projection with the transcript renderer | the selection holds while chunks arrive | pending:run-display |

## Performance thresholds (proposed)

The app's contribution is measured from the PTY write of a keystroke to its glyph in the bytes the renderer writes
back (`qa_latency`). It adds to the keyboard's and the terminal's own latency: median typing latency measured for
terminal emulators ranges from about 2 ms (xterm) to 28 ms (gnome-terminal) (Typometer runs published with the
Zutty terminal, 2021), and indirect input stops being
perceived as immediate somewhere around 50 to 75 ms end to end (Deber, Jota, Forlines, Wigdor, CHI 2015, « How
Much Faster is Fast Enough? »). One 60 Hz frame is 16.7 ms. The thresholds keep the renderer within one frame at
p95 and two at p99, so a good terminal stays fast with Nika in it.

Two probe runs at the base, both on development builds: run A under a load average of about 27, run B about 39.
CPU time per keystroke agrees between them; wall-clock latency does not (the machine was saturated by other builds),
so the wall-clock figures below are upper bounds from run A unless run B is named.

| Metric | Proposed threshold (release build, idle machine) | Base (development build) | Rationale |
|---|---|---|---|
| Echo latency, idle | p50 ≤ 5 ms · p95 ≤ 16 ms · p99 ≤ 33 ms | inline 80x24: 1.05 / 6.55 / 13.28 ms (max 23.25) · focus 120x40: 2.20 / 7.86 / 13.00 ms; run B: 1.64 / 16.68 / 48.46 and 6.01 / 100.77 / 142.14 ms | within one frame at p95, two at p99 |
| Echo latency during an animation (spinner, reveal, sweep) | p95 ≤ 16 ms · p99 ≤ 50 ms | not measurable: a key typed during a turn is not drawn until the turn ends (shown 41 ms after the release, 2 042 ms after the key) | an effect frame may delay one keystroke by one frame, never more |
| Echo latency with 10 000 transcript blocks | the idle thresholds | focus 120x40: 352.83 / 899.10 / 1 379.59 ms (run B: 1 594 / 3 411 / 5 214 ms) | history must not tax typing (a virtualised transcript; the transcript rendering target is draw ≤ 2 ms p95) |
| Bytes per keystroke echo | ≤ 64 B | 40 B | one cell, one cursor move, one style reset |
| CPU per keystroke | ≤ 2 ms | 0.65-0.70 ms inline, 1.70-2.00 ms focus 120x40, 220-233 ms focus with 10 000 blocks | one redraw at 60 Hz must leave room for the terminal |
| Busy spinner | ≤ 40 B per row per 100 ms step | 41 B per step, inline and focus (826 B over 2 s, run B) | the activity budget below; the seconds counter adds its bytes once a second |
| Idle output | 0 bytes over 5 s, after 1.5 s for finite effects to end | 0 bytes (inline prompt; focus after a turn; after every base effect) | zero redraw ticks when idle |
| Focus report | ≤ 64 B, then silence | pass | a window switch starts nothing |
| Bytes per effect | reveal ≤ 4 KB · sweep ≤ 2 KB · attention and glow ≤ 512 B · activity ≤ 40 B per row per step | no effect at the base | the design panel's proposal; bytes matter over SSH |
| Resident memory with 10 000 transcript blocks | ≤ 32 MiB peak | resident 19.3 MiB in focus in run A, 6.2 MiB in run B (macOS compresses idle pages under pressure: measure the peak, e.g. `/usr/bin/time -l`) · 4.9 MiB inline · the idle proto 4.2 MiB | one list of blocks, no per-frame copies |
| First prompt | ≤ 250 ms with an answering terminal | 285 ms focus, 755 ms inline (run B: 3 340 and 1 224 ms); 10 000 blocks committed inline: 1 371 ms (run B: 17 163 ms) | the opening is where people judge speed |
| A 100 000-label busy flood | the turn ends within one frame of its work | 50 ms, 3 KiB (run B: 427 ms, 2 KiB) | a burst never queues frames behind the work |

Measure on a release build and a quiet machine before ratifying; record the load average, the profile and the
terminal with every number.

## Reception procedure

1. Check the candidate out read-only in a separate detached worktree, leaving the implementation checkout untouched.
2. Build the nika-tui tests and `nika-tui-proto`; build `nika` when J0 applies.
3. `cargo test -p nika-tui --locked`: every regular proof passes.
4. `cargo test -p nika-tui --locked -- --ignored`: list which defect and gap checks now pass, and which still fail.
5. The latency probe; compare with the thresholds.
6. The tmux matrix on the candidate's proto, and J0 on its `nika`.
7. Report findings ranked P0 to P3: the failing check, the observed screen, the affected component.

## Known at `513ca8465`

| Finding | Severity | Check | Component |
|---|---|---|---|
| Typeahead answers a question, consents to a proposal, answers a gate painted after it; on the real binary it picks and keeps an intelligence on a choice screen painted after it | P0 | `typeahead_never_*`, `typeahead_during_a_busy_turn_never_answers_the_gate_it_ends_on`, `qa_journey0::typeahead_never_picks_an_intelligence_on_a_screen_painted_after_it` | `app.rs` input handling |
| A word wider than the composer is clipped; the rest is typed blind | P1 | `a_word_wider_than_the_composer_stays_visible` | `composer.rs` sizing |
| Narrowing the terminal while a proposal waits wipes the proposal off the screen; `apply? ›` still asks | P1 | `a_width_shrink_keeps_the_waiting_proposal_on_screen` | terminal rendering and input handling (inline viewport) |
| Focus with 10 000 transcript blocks: 0.35 to 1.4 s per keystroke | P1 | probe J7.1 | transcript rendering |
| Keys, scroll and `Ctrl+T` during a turn wait for the turn to end | P1 for J1 to J3 | `keys_typed_during_a_busy_turn_show_at_once`, `scrolling_during_a_busy_turn_moves_the_view_at_once` | `app.rs` input handling, contract:conversation-switching |
| With tmux defaults, a split arrow mis-parses into Esc: full screen left, `[A` in the draft | P2 | tmux `split-arrow-50ms-escape-time-10` | input handling |
| `SIGHUP` kills the renderer without restoring the terminal | P2 | `sighup_restores_and_leaves_with_129` | input and signal handling |
| B1 to B4: stretched live area, doubled echo marker, double-spaced run lines, live area left painted after exit | P2 | `qa_layout` B1 to B4 | terminal rendering |
| The intelligence choice lists five local engines as available on a machine where none is configured or running | P2 | J0.3 capture | nika-session (the census is presence-only; the TUI shows its text verbatim) |
| Under `NO_COLOR`, a forced colour paints no hue (crossterm reads `NO_COLOR` itself) and the weight substitutes are gone | P3 | `a_forced_colour_under_no_color_is_the_colour_the_terminal_sees` | terminal rendering and input handling (one colour owner) |
| The inline viewport is not cleared when created: a partial line under the cursor stays inside it | P3 | observed through the child host; the handoff path creates a fresh viewport the same way | input handling |
| A resize the renderer does not observe is never repainted: a `Resize` event repaints only when the size read then differs from the last known one, so a round trip (a quick zoom, a storm back to its start) leaves the screen as a non-reflowing terminal cut it | P3 | observed under load with the screen model (xterm policy); tmux showed the screen intact after a round trip | `app.rs`: repaint in full on every `Resize` |
