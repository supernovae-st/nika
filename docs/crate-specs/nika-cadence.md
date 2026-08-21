# Crate spec — `nika-cadence`

| | |
|---|---|
| Status | **CANDIDATE** — Gate 1 (this document) authored 2026-08-11. Crafted shim-standalone (50 tests today, 45 at authoring · clippy 0 `-D warnings` · rustfmt clean) · committed with the temporary `[workspace]` shim (`92a0f8497`), then the four pre-freeze corrections of plan §2unvicies (the bitset's ONE encoding · `Slot` declares the DST shift · the field count is the type · the error span). The two items this row used to name (the allowlist row, the shim removal) are BOTH DONE; the row described work already shipped. Remaining before admission, measured 2026-08-13: the Gate 11 P1 below, and Gate 5 at 88 percent against a 90 floor. **W1, measured 2026-08-19**: 79 tests green (`cargo test -p nika-cadence --lib`) · `Cadence::prev_before` (the mirror, 366-day bound) · the `due` planner (`due` · `earliest_next` · `DueKind` · `ON_TIME_WINDOW`) — the pure half the `fire`/`serve` edges read. The L4 `emit` adapter and resident `serve` consumer are now landed (see §3). Gate 5 re-run this wave; the floor holds ≥90. |
| Layer | L0 — pure, zero I/O, zero async |
| Design | The arming-registry grammar (the `arm:` block of `nika.yaml`, D-2026-08-10-N3) + the pure next-slot calculator + the W7 typed firing and ledger machines. Hand-counted 5-field cron (zero cron library — the count is validated BEFORE field semantics, scar #6) · IANA zones resolved from the EMBEDDED tzdb only (`jiff-tzdb`, never the host's zoneinfo) · two cadence forms (cron + readable `lundi 9h07`), display normalizing to the readable one. The machines own no I/O and read no clock: callers inject events, policy, `now`, and borrowed journal text; the L4 adapter alone owns files, locks, fsync, and rotation. |
| LOC budget | ≤5,000 src prod (W7 measured 4,623 after the complete pure ledger/snapshot seam) · ≤15,000 hard cap |
| File cap | ≤1,500 LOC each (W7 max 1,493 in `ledger.rs`; `firing.rs` 1,372) |
| Function cap | ≤100 lines each (max ~60) |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 (workspace-inherited at admission) |
| Publish | `false` — foundation crate, never on crates.io |
| Dependencies | `serde` · `serde_json` (W7 ledger wire fold) · `serde_yaml_bw` (the panic-free YAML plane) · `thiserror` · `jiff` · `jiff-tzdb` (the embedded IANA tzdb) · `sha2` (W7 domain-separated `SlotId` + `ArmGeneration`) — dev: `proptest` |
| NIKA codes | **none owed** — `CadenceErrorKind::spec_code()` emits the grammar's OWN slugs (`cadence.*`), never a `NIKA-*` registry code; every refusal is rendered as a taught fix at the L4 verb boundary (`exit 2`, the FILE plane). The `check-error-one-voice.sh` allowlist row is ALREADY in place (class `spec-plane`, the `CelErrorKind` precedent — corrected 2026-08-13 at Gate 11; this row said `wrapped-intermediate`/`ExprError`, which the real TSV and the canonical audit table both contradict). |

---

## 1. Purpose

`nika-cadence` is the pure arming domain: registry grammar, slot calculator,
firing state machine, and ledger codec/replay fold. It answers when a beat
fires and what durable evidence means without touching a file. Two L4
consumers read this registry (`nika arm` today ·
`nika serve` at ②), so the shared logic lives at L0 — never in a CLI
crate (the layering precedent: `nika-check`'s Cargo.toml · "THREE L0
consumers make any higher layer an upward-dep violation").

The four locks (D-2026-08-11-N1→N4 · one law at four moments: THE FILE
PROPOSES, THE MACHINE DISPOSES):

- **N1 · DST** — a slot inside a spring gap fires at the FIRST VALID
  instant (02:00 absent ⇒ 03:00 — never jiff's roll-forward, never a
  silent skip) · a slot in an autumn fold fires ONCE, at its first
  occurrence. Implemented as gap-detection (`zoned.datetime() != civil`)
  + minute-stepping, bounded at 26 h.
- **N2 · no resume** — a beat starts from ZERO; the pure fold describes
  evidence but never resumes or executes a run.
- **N3 · identity** — `par:` DECLARES the human and proves nothing; the
  machine's key authorizes. A merge arms nothing (`arm --write` is L4).
- **N4 · absence** — removing a line does NOT disarm; that gesture is
  `arm --disarm`, an L4 act this crate knows nothing about.

The grammar laws (validated at parse, every refusal named and teaching
its fix): `manqué:` and `plafond:` REQUIRED, no default (choosing for
the operator is choosing who pays) · the zone lives INSIDE the cadence
expression (`TZ=Europe/Paris 0 9 * * 1` — only `on-webhook` goes
zoneless) · `dom`+`dow` restricted together refused (the Vixie OR trap)
· weekday origin NAMED (`0` and `7` are dimanche) · safe defaults in
the DEFAULTS (law ⑥): `où: local` · `chevauchement: sauter` ·
`après_saut: prochain-créneau` · `actif: false` requires `raison:` +
`jusqu_au:` · round 1 refuses by name: `signature:` · `budget:` ·
`traces:` · `registry:` · every unknown key (`deny_unknown_fields`).

It does **not** own: filesystem access (the L4 edge reads the bytes,
this layer reads the text — a workflow's EXISTENCE is judged there,
its SHAPE here) · clocks (trap ①: the kernel `Clock` trait has no civil
surface, so the calculator takes a `jiff::Zoned` and tests ride literal
instants) · sleeping (trap ②: `VirtualClock::sleep` does not advance
time — the caller sleeps, never the calculator) · host zone resolution
(trap ③: `TimeZone::get` is forbidden — `jiff_tzdb::get` +
`TimeZone::tzif` only).

## 2. Determinism contract

- same `(cadence, instant)` ⇒ same next slot, on every host — the tzdb
  is baked at build, re-vendored each release, zero system read
- no `HashMap` anywhere (the workspace lint guards a future signature's
  determinism) · no `Vec` in public returns (FCI-014 — accessors hand
  out slices or iterators) · `#[non_exhaustive]` on public-field
  structs (FCI-016) · wire tag frozen at `nika: v1` (FCI-003) <!-- stale-ok: the PROJECT file (nika.yaml) · the engine still freezes v1 here while spec 01 says nika: <name> · engine work owed -->
- the day walk happens in the BEAT's zone — a Monday slot is Monday in
  Paris, whatever zone `from` rides · horizon 3,000 days forward (a
  century year is leap only when divisible by 400: from 2096-03-01 the
  next 29 February is 2,920 days out — the 1,500-day horizon died on
  that, §6.2) · 366 days backward (`prev_before`: the recent past a
  planner asks about, never an archaeology)

## 3. Public surface

`parse_registry(&str) -> Result<ArmRegistry, CadenceError>` ·
`validate(&ArmRegistry) -> impl Iterator<Item = CadenceError>` (an
empty walk IS the green verdict) · `Cadence::parse(&str)` (the count
is the TYPE — `parse_cron_fields(&[&str; 5])`, scar #6 is the
compiler's) · `Cadence::next_after(&Zoned) -> Option<Slot>` where
`Slot { at, civil, shift }` DECLARES the DST displacement
(`Shift::{Exact, AdvancedFirstValid, FoldedFirst}` — N1 at the type
level; a merged slot says so) · `Cadence::prev_before(&Zoned) ->
Option<Slot>` (W1 — the mirror: strictly-after there, at-or-before
here, the two bound the half-open interval `(prev, next]` a beat is
due in; 366-day walk; the gap's advanced slot is never RETURNED —
`next_after` carries it) · `Cadence::describe() -> String` (the
readable form wins, full fields print `*` — the bitset's one
encoding) · `next::next_slots(&Cadence, &Zoned, usize) -> impl
Iterator<Item = Slot>` · `ArmRegistry::{SCHEMA, beats, beat_count}` ·
`Beat::{locus, overlap, after_skip, is_active}` · `CronSpec` field
accessors handing out `Field<LO, HI>` (the bitset: `contains` ·
`iter` — double-ended since W1, the backwards walk rides `.rev()` —
`single` · `is_full` · 8 bytes · `Copy` · zero alloc) ·
`CadenceError{kind, detail, remedy, on, span}` (the span paints the
faulty byte, the `CelError` precedent) + `CadenceErrorKind::
spec_code()`.

**W1 — the pure planner (`due` module), the half both firing edges
read** (`fire` at W2 · `serve` at W5): `due(&ArmRegistry, &Zoned, &dyn
Fn(usize) -> Option<Zoned>) -> Result<impl Iterator<Item = Due>,
CadenceError>` — active · local beats whose previous slot falls in
`(last_fired, now]`, the firing state arriving as a CALLBACK (N2: the
crate computes slots, never carries run state) · `earliest_next(
&ArmRegistry, &Zoned) -> Result<Option<(usize, Slot)>, CadenceError>`
— what the edge sleeps until · `Due { index, beat, slot, kind }` ·
`DueKind::{OnTime, Missed { slots }}` (the silence counted whole over
the FIRE set — the gap's advanced fire included — saturated at
`MISSED_SLOTS_CAP` = 10,000) · `ON_TIME_WINDOW` = 5 minutes (a
`SignedDuration`: absolute time, and `Span`'s builders are not `const`
in jiff 0.2). `emit` is landed at the L4 adapter: cadence supplies the pure
schedule and label inputs while `nika-cli` renders launchd/systemd units.

**W7 — the pure firing machine (`firing` module)**:
`SlotId::derive(workflow, cadence, slot)` freezes the existing
`nika/arm-slot@1` identity; `ArmGeneration::compute(beat,
workflow_bytes)` freezes `nika/arm-gen@1` over the beat's declared
canonical fields and the exact workflow-byte hash; `FencingToken`
prevents naked sequence integers crossing the boundary.
`FiringEvent` and `FiringState` carry the closed lifecycle vocabulary;
`transition` is the table, `fold` applies fencing pairing, and `decide`
returns typed ordered effects under an injected `FiringPolicy` and
`Timestamp`. Every public enum/struct is forward-compatible; the three
identities validate their wire form before construction.

**W7 — the pure ledger (`ledger` module)**: `DecisionKind`, `Claim`, typed
`Receipt`, `HistoryEntry`, `Unsettled`, and `LastRecord` are the wire
vocabulary. A `Receipt` copies slot identity and generation from its claim,
carries the exact fencing token, and derives terminal kind from exit (`0`
fired, `4` paused, every other accepted code failed). Modern bare, mismatched,
duplicate, future, or contradictory receipts make the chain invalid; only an
explicitly marked legacy bare receipt remains readable;
`ledger_line` and `verify_line` freeze the `nika/arm-event@1` hash chain;
`scan_chain` returns the verified prefix; `replay` folds borrowed journals into
the byte-stable projection, watermark, and lifecycle; `fold_replay` applies the
open deadline boundary; `unsettled` reconciles only a matching later fencing
receipt. `nika-arm` is the filesystem adapter and owns every effect.
This seam is the ADR-114 amendment: no dependency cycle and no second judge.
That adapter holds a kernel advisory lease on a stable path (PID/epoch bytes are
diagnostic, never authority) and advances a local `head.json` seq/hash anchor
after each fsynced append. The anchor distinguishes a clean suffix deletion
from a legitimate append→anchor crash: only an older hash that still matches
the verified prefix may advance; missing or ahead/mismatched evidence refuses.
The `rotated` genesis also commits the ordered W2 archive bundle by canonical
name and exact-byte SHA-256. The L4 adapter validates that commitment before
every fold or write, so archive alteration, reordering, insertion, and deletion
all fail closed.

## 4. Tests

79 today (W1, 2026-08-19 — 50 before the wave; the planner and the
mirror added the rest), 45 at authoring (the four pre-freeze corrections added five that
were described in prose here without being recounted): parse (both forms ·
TZ-less refused · 6 fields refused
by the TYPE · out-of-range · Vixie OR judged on the sets (`1-31` dom is
every day) · unknown zone · 7-is-dimanche · spans pin the faulty token)
· calculator (strictly-after · month/year crossing · slept-3-days ·
29 Feb 2028 inside the horizon · **DST traversal**: the 2026-03-29 gap
fires 03:00 CEST with `Shift::AdvancedFirstValid`, the 2026-10-25 fold
fires once with `Shift::FoldedFirst`, and the gap-day merge
`0,30 2,3 * * *` is visible in the returned slots — 4 civil slots, 2
fires, the absorbing fire declared) · **the mirror (W1)**: at-or-before
· the slot at the instant itself · gap walked backwards (the advanced
slot is never returned) · fold walked backwards (the first occurrence,
then the EVE — never the second) · the 366-day bound (a 29 February
three years back is `None`) · month/year crossing backwards · **the
planner (W1)**: on-time due once · missed with the silence counted ·
idle/cloud never due · already-fired never re-due · never-fired invents
no backlog (N2) · the 5-minute window to the second · the 10,000 cap ·
the gap fire due like any other (the fire-set read) · a law-breaking
cadence refuses the whole plan · the two-encodings regression
(`describe("* * * * *")` stays 5 fields, not 244 chars) · validate
(every law, every refusal teaching its fix) · the embedded tzdb proven
twice (behavioral Paris offsets + a static source guard: no non-comment
line may name the host-preferring resolvers) · proptest (parse never
panics · law pass never panics · a daily slot is strictly later and
within a day · **the inverse law (W1)**: `prev_before(next_after(t) +
1s) == next_after(t)` over the corpus, the gap slot's exception
declared). **W7 firing tests**: known-vector `SlotId`; stable generation
across computations, changed generation on one workflow byte, positional
label excluded, canonical hash assembled independently; every lifecycle
transition and terminal; foreign fencing; durable decision ordering;
typed skip reasons; and proptest over arbitrary event sequences against
an independently encoded transition table. Mutation floor: run
`check-mutation-floor.sh` at admission and again for every new semantic
module.

**W7 ledger tests** additionally pin canonical-line verification, one-byte
tamper refusal, verified-prefix truncation, claim/receipt lifecycle folding,
orphan deadline equality vs strictly-after ambiguity, legacy replay,
byte-stable projection round-trip, and tallies. The CLI adapter suite keeps the
filesystem E2E matrix (delete/rebuild, tamper, reorder, truncation, migration,
idempotence, and audible refusal).

## 5. Non-goals / guards

No `Clock` dependency (the civil surface is a `Zoned`, the clock lives
at the L4 edge) · no sleep in the calculator (a sleeping loop spins
forever under `clock: virtual`) · no `TimeZone::get` (hermeticity) ·
no NIKA-* codes (no range allocated) · no resume/catch-up state (N2 —
the dispatcher's concern, L4) · no disarm gesture (N4) · no signature
verification (②'s, refused by name in round 1).

### Tranché à la revue d'avant-gel (2026-08-12, opérateur)

- **`describe()` n'est jamais re-parsé** — la forme lisible affichée
  (`lundi 9h07 · Europe/Paris`) est pour les yeux ; la branche cron
  seule est round-trip. La grammaire ne s'étend PAS au suffixe `· tz`.
- **les créneaux absorbés ne sont pas un élément du flux** — `Shift`
  reste `#[non_exhaustive]` : un `Absorbed` déclaré peut rejoindre sans
  casser, le jour où un consommateur en a vraiment besoin.
- **`ArmedRegistry` (champs déballés, légalité structurelle) se tranche
  à V3⑪**, avec le premier consommateur réel (`sign cadence`) en main —
  la duplication « re-parse + None-impossible » sera prouvée ou
  imaginaire à ce moment-là, pas avant.

### W5 — `nika serve` : les trois questions du brouillon, tranchées (2026-08-19)

Le tireur résident (`crates/nika-cli/src/verbs/serve.rs`) a clos les trois
questions que le brouillon laissait ouvertes — les réponses vivent ici,
pas dans le code :

- **`max_retries` au niveau job → NON.** Un beat repart de zéro (N2) :
  chaque tir est un run NEUF, un run en pause (exit 4) est PARQUÉ avec sa
  trace, jamais repris. L'ordonnanceur n'a donc rien à quoi un retry
  s'accrocherait — le retry vit DANS le workflow (`retry:` sur la tâche,
  plein-jitter et tout), jamais dans le beat.
- **l'enum d'état de job → c'est `history.ndjson`.** L'état n'est pas un
  type à inventer : c'est le journal du sidecar (`.nika/arm/<label>/`) —
  devenu le ledger versionné `nika/arm-event@1` (W5-bis · chaîne sha256
  vérifiée à chaque append, queue invalide coupée, journal W2 roté sans
  effacement, chaque append fsync'd). Le vocabulaire `kind` des DÉCISIONS
  est fermé — `fired` · `skipped` · `paused` · `failed` · `disarmed` (le
  dernier est history-only : il ne porte pas de slot, donc `record` ne
  l'écrit jamais dans `last.json`, et `last` le relit comme illisible —
  la direction sûre) — et deux lignes STRUCTURELLES le complètent :
  `claimed` (le claim durable, fsync'd AVANT le run · son receipt le
  settle par fencing) et `rotated` (la preuve d'archive d'un journal
  d'avant le ledger).
- **`serve_tokens` (qui déclenche à distance) → hors v0.** Aucun port,
  aucune entrée : Gate 1 (diamond-discipline §5, résolu 2026-08-19) —
  `serve` ne lit QUE `nika.yaml` et son sidecar, jamais le réseau, et le
  test `serve_has_no_input_but_the_registry_and_its_state` le pinne. Le
  déclenchement à distance est le problème du cloud (`où: cloud`) : le
  cloud le portera, pas le résident.

---

## 6. Gate status (measured 2026-08-13, not declared)

| Gate | Verdict | Evidence |
|---|---|---|
| 1 SPEC | ✅ | this document |
| 2/3 TDD + IMPL | ✅ | 50 tests, all green (`cargo nextest -p nika-cadence`) |
| 4 CLIPPY | ✅ | `--all-targets -- -D warnings`, rc 0 |
| 5 MUTATION | ✅ **93 percent** (200/215 viable) | `scripts/ci/check-mutation-floor.sh nika-cadence 90`, real run, after the three killer tests below. Was 88 percent. |
| 8 DOCS | ✅ | `cargo doc --no-deps`, 0 warnings |
| 11 REVIEW SWARM | ✅ **three lenses, P1 and both P2 closed** | doctrine lens approved outright; the Rust lens found the P1; the spec-vs-code lens found two doc faults. All fixed the same session. |
| 12 ATOMIC | ✅ this admission | removal from the `wip` array in the workspace manifest |

Plus the shared gates, all green the same day: loc-limits · fn-length ·
unwrap · expect · crate-size · adr-coverage · credential-headers ·
layering · dead-code · error-one-voice.

### 6.1 The Gate 11 P1, and why it blocked (CLOSED 2026-08-13)

**A `(dom, months)` pair that no calendar can satisfy parses green, and the
beat then never fires, in silence.**

`TZ=Europe/Paris 0 9 31 4 *` (April 31, a banal typo) parses `Ok` and
`validate` returns zero faults: after the five fields, the ONLY structural
check is the Vixie `dom`+`dow` OR guard (`src/cron.rs`). `next_after`
(`src/next.rs`) then walks its horizon, `covers()` is true on no day, and it
returns `None` forever.

This crate's own contract is that every refusal is named and teaches its fix.
A silent `None` is the one outcome that contract forbids.

**Closed** · `CadenceErrorKind::DateImpossible` (`cadence.date-impossible`)
now refuses at parse, after the Vixie guard, when no month in the set admits
a day in the `dom` set. February takes **29**, not 28: a leap year makes the
29th real, so `29 2` is a beat that fires rarely rather than an impossible
one, and refusing it would have been the mirror of the fault being closed.
Judged on the SETS like the Vixie guard, so an unrestricted side needs no
special case. The refusal teaches the bound, not just the refusal.

### 6.2 Two P2s found alongside, both confirmed by reading

- **The 1500-day horizon is too short, and its comment is false.**
  `src/next.rs` reasons that "a 29 February is never further than 4 years".
  2100 is not a leap year, so from 2096-03-01 the next one is 2104-02-29,
  2920 days out. `0 9 29 2 *` returns `None` there. Raise to 3000.
- **One unresolvable slot kills the whole walk.** `resolve(civil, tz)?`
  propagates its `None` out of `next_after` rather than skipping that
  candidate, so a single irresolvable civil time silences the entire beat.
  Use an `else { continue }` binding.

### 6.3 One finding raised then withdrawn, recorded so it is not re-raised

`Field<LO, HI>` (`src/cron.rs`) carries no `#[non_exhaustive]`. Two lenses
disagreed; the tie was settled by reading. Its single field `bits: u64` is
private and there is no public constructor (`empty`/`full`/`set` are all
crate-private), so external code can neither literal-construct it nor match
it exhaustively. FCI-016 governs public FIELDS and does not bind here. No
change owed.

### 6.4 What two adversarial reviews found AFTER admission

Admission is not the end of the review; it is the point where the crate stops
being watched, which is why both passes were run against the shipped code
rather than the candidate.

**A refuter attacked the `DateImpossible` guard and could not break it.** The
guard is an existential OR over the full `months` × `dom` product, so it can
only reject a pair no calendar satisfies, never a satisfiable one. Blast
radius measured the same day and it is zero: the only cron in the monorepo
matching the refused shape is this crate's own fixture, and no `arm:` registry
exists on disk at all.

**It found the mine beside its target instead.** `field_value` returned a
matched NAMED alias without the bound check the numeric path applied. Harmless
while every table sits in range, and a hazard otherwise, because `Field::set`
shifts by `v - LO` and an alias below `LO` underflows a `u8` before the shift.
Closed, and the test guards the TABLES rather than the branch, since the branch
cannot be exercised without adding a bad alias.

**A Rust review then found the guard's default arm pointing the wrong way.**
`longest_day_of` answered `31`, the most permissive bound, to the one input
nobody had vetted. An out-of-range month would have made the satisfiability
check PASS, which is the silent `None` the guard exists to kill. Unreachable
today, and being unreachable is not being right: it answers `0` now, and no day
a `Field<1, 31>` holds is `<= 0`, so an impossible month contributes nothing.

The same review measured what this spec had only asserted. The cartesian
product costs 2 iterations on the common path and its 372 only on the error
path, once, at parse. The 3000-day horizon clears a worst REAL walk of 2921
days (the day after a 29 February, across a non-leap century) with 79 days to
spare, and the test pins 2920, so shrinking the horizon reddens.

**Property gained**, in the reviewer's words: the guard and the horizon
together make « it parses ⇒ it fires » total.

### 6.5 What Gate 5 said about the tests, concretely

The 23 survivors cluster, and one cluster matters more than the others:
three of them sit on `validate_beat`'s ceiling guard
(`!(p > 0.0 && p.is_finite())`, `src/parse.rs`). Replacing `&&` with `||`,
`>` with `>=`, or the whole guard with `false` all survive, which means no
test distinguishes a ceiling of zero, of infinity, or of NaN from a valid
one. `plafond:` is a REQUIRED field with no default precisely because it
decides who pays; its guard is the last one that should be untested. The
other named survivors sit on `Field::is_empty`, `parse_field`'s bound
comparison, `CadenceError::remedy` (never asserted), and a `*` in `resolve`.

### 6.6 W7 firing-ledger mutation proof

The final testimonial binds tested commit `32ebdf3a` and tree `bb7f629d` to
the committed pre-run receipt `24c89f07`. cargo-mutants 27.0.0 enumerated 486
unique mutants across `src/firing.rs` and `src/ledger.rs`. Three exact,
once-matched mutants are excluded with reachable-domain equivalence proofs:
the `unsettled` `>` to `>=` replacement at `ledger.rs:406`, plus the two
`LifecycleValidator::accept` `&&` to `||` replacements at lines 1108 and
1109. Their exact identities, proofs, source binding, and exclusion
cardinality live in the testimonial manifest and its pre-run receipt.

The remaining 483 mutants ran serially with the exact receipt-bound command:

```console
<CARGO_BIN> mutants -p nika-cadence \
  -f 'crates/nika-cadence/src/firing.rs' \
  -f 'crates/nika-cadence/src/ledger.rs' \
  -E '<EXACT_EQUIVALENT_406>' \
  -E '<EXACT_EQUIVALENT_1108>' \
  -E '<EXACT_EQUIVALENT_1109>' \
  -o '<OUTPUT>' -j 1 --baseline run \
  --timeout 300 --build-timeout 300 -- --lib
```

The complete run settled **442 caught, 41 unviable, 0 missed, and 0 timed
out**. The viable mutation score is therefore **442 / 442 = 100%**, above
the 90% floor. The privacy-sanitized `outcomes.json` has SHA-256
`3b8ffd15eb5ca7ef07b0807067c43c49ebb89d0b3bf840410ece240e4f2e8349`;
the [machine-verifiable manifest](../testimonials/arm-w7-ledger-salvage/manifest.json)
binds it to the raw artifact hash, full accounting, invocation, tools,
inputs, and clean tested tree.
