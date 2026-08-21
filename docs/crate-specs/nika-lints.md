# Crate spec — `nika-lints` (descended member)

| | |
|---|---|
| Status | **DESCENDED 2026-07-13** from `nika-schema` (W1 window) — NOT a fresh admission: a size-cap member split of an already-admitted unit per D-2026-07-09-N1 (one architectural unit · two workspace members). The W1 tranche tipped `nika-schema` past the 15k prod-LOC cap (main sat at 14963); the lint subsystem was the clean seam (zero in-workspace production consumers — only its own suites). |
| Layer | **L0** — pure, zero I/O, zero async; deps `nika-schema` (RawWorkflow · spans), `nika-check` (the shared native-first classifier), and `serde_json` (rule-table payload shapes). |
| Design | The spec-normative **advisory lint passes** — warnings, never errors (spec `03-dag.md` §One obvious way · « the discouraged forms are legal · just not canonical »). Three rule sets: `one_obvious_way` (the preference rules the spec marks « normative for linters »), `native_first` (verb-choice advisories · exec with a probable native path · all-segment classifier shared with the check ladder via `nika_check::native_first::classify_all`), `arg_injection` (argument-injection advisories for the exec array form · spec `02-verbs.md` §exec Security). |
| Name | `nika-lints` — honest plural: a home for lint PASSES, each pass a function `&RawWorkflow → Vec<Lint>`. Descent precedent: `nika-tmpl` (2026-07-10). |
| LOC | ~2566 LOC src (`scripts/crate-metrics.sh --loc nika-lints` · ±15% band per vector 6). |
| Deps | `nika-schema`, `nika-check`, `serde_json`. dev: `serde`. |
| Publish | `false` — foundation crate (ADR-022). |

## 1 · Why this crate exists

Two reasons, one mechanism:

1. **The cap held.** `≤15k prod LOC/crate` is a hard law; the W1 « the map »
   grammar tranche (+238) crossed it. The sanctioned move is a member split,
   not an exemption — the unit (the schema language surface) stays ONE
   architectural unit; the workspace gains one member.
2. **The seam was real.** The lint passes consume only the public parsed
   model (`raw` · `source` · `expression` · `types`) plus one shared
   classifier; nothing in the workspace invokes them in production paths
   (the check ladder's `schema_lints`/`hints` lanes are a different,
   ladder-native subsystem). A library API this separable does not belong
   inside the parser's LOC budget.

## 2 · Known residue (owned)

The passes are spec-normative and engine-implemented but **not wired to any
user surface** (`nika check` lanes and the LSP do not run them) — tracked in
the perfection ledger as an owned residue; the wiring decision (ladder lane
vs LSP-only vs both) belongs to a later wave, not W1.

## 3 · Rule admission and promotion

A lint is admitted only when all of these are true:

- its predicate is deterministic, dependency-free, and statically executable;
- the discouraged shape recurs often enough to justify permanent diagnosis;
- its advice does not hide semantics, authority, cost, or failure behavior;
- any suggested `nika:*` replacement already resolves in the real builtin
  catalog — an aspirational builtin never becomes a hint.

Each stable id owns an engine regression pair under
`tests/fixtures/{compiler,lint}/`: a positive that fires and a neighboring
negative that stays silent. The exact 16-id matrix covers
`one-obvious-way/001..010`, `native-first/001..005`, and
`arg-injection/001`. These are engine diagnostics fixtures, not a claim of
spec-conformance; the portable conformance corpus remains in `nika-spec`.

Severity follows the clippy discipline. A correctness-class predicate must
have zero known false positives. Suspicious shapes warn. Pedantic shapes may
accept deliberate false positives to retain useful sensitivity. Restriction
rules are never enabled as a block. A warning that becomes perfectly reliable
does not escalate into a harsher warning: it descends into the compiler as a
coded refusal, while its stable lint id remains retired. That is the
`one-obvious-way/001` → `NIKA-VAR-021` precedent.
