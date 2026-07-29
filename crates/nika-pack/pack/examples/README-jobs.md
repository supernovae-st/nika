# Showcase · industry workflows, simplest → epic

> Complete, spec-correct workflows (the count lives in [`../manifest.yaml`](../manifest.yaml)) that answer one question per
> industry · **« what would MY Monday look like with this? »** Every
> file passes the same conformance gate as the foundation examples
> (`python conformance/runner.py all`) — schema + DAG cross-refs +
> stdlib surface. Together with the foundation set they exercise
> **every stdlib builtin** (verified by the conformance + coverage sweep).

The [foundation examples](../README.md) teach the *language*
construct-by-construct. The showcase teaches the *life* — real jobs,
tiered by complexity so a newcomer climbs from a 4-task starter to a
multi-agent swarm without ever leaving validated ground.

---

## The ladder · 4 tiers

```
T1 STARTER     ≤4 tasks · one chain or one diamond · your first wow
T2 CHAIN       4-6 tasks · typed outputs · human gates · data builtins
T3 FAN-OUT     runtime collections · for_each · retry · jq zips · swarms
T4 EPIC        multi-stage pipelines · agents under budget · self-reporting runs
```

## The workflows

| File | Industry | The wow | Key constructs |
|---|---|---|---|
| `t1-standup-digest` | engineering | the standup note writes itself from real commits | parallel start · `nika:date` · exec→infer |
| `t1-meeting-actions` | every office | transcript → tracker-ready typed action items | `infer.schema:` · input `default:` vs literal permit |
| `t1-price-watch` | e-commerce / personal | a price alert with **zero** model calls | `output:` jq · CEL `when:` · `egress:` **and** `net.http` |
| `t1-social-repurpose` | marketing / creators | one post → thread + LinkedIn + newsletter, in parallel | diamond DAG · `with:` aliasing |
| `t1-og-images` | marketing / content | brief in → OG PNG + provenance manifest out, one task | `nika:image_generate` · dir-writing permit |
| `t1-image-fx-batch` | creators / media | a folder of photos → deterministic art, byte-identical forever | `nika:glob` · jq-derived paths · `nika:image_fx` ops chain |
| `t2-release-notes` | engineering / devrel | git log → typed notes → a CHANGELOG copy edited in place | `exec` `recover:` · `nika:edit` · gated notify |
| `t2-seo-content-brief` | SEO / content | a brief that beats the competitor's best page | chained fetch modes · `recover:` · CEL indexing |
| `t2-invoice-chaser` | finance / freelance | overdue reminders drafted · NOTHING sent without a yes | `nika:convert` · `nika:prompt` gate · `size()` |
| `t2-support-triage` | customer support | the overnight queue triaged before coffee | schema-over-list · jq post-filter · `nika:uuid` |
| `t2-contract-guard` | legal / compliance | the contract **never leaves the machine** (local model) | `ollama/…` · `nika:validate` + `nika:assert` |
| `t2-etl-quarantine` | data engineering | bad batches degrade to quarantine · the pipeline lives | `on_error: recover:` · `nika:validate` · jq group_by |
| `t2-model-bench` | engineering / model selection | the same question, three local models, one MEASURED table | per-task `infer.model:` · `duration_ms` as data · jq fan-in |
| `t2-release-radar` | devops / dependencies | only the NEW ships reach you — **the one deliberate red**, see below | `mode: feed` · state-file diff · RFC 6902 |
| `t2-csv-chart-report` | data → picture | paste the spreadsheet, get the slide — offline, deterministic | `nika:convert` · jq group_by · `nika:chart` |
| `t2-transcript-shownotes` | podcasts / meetings | raw transcript → typed show-notes, ONE bounded infer | `infer.schema:` strict · typed→markdown |
| `t2-bookmark-triage` | personal / research | the bookmark pile triaged — dead links survive the batch | `mode: metadata` · resilient `for_each` · recover |
| `t3-competitor-radar` | strategy / PMM | everything they shipped last week, one brief | `for_each` · `max_parallel` · retry · fan-in |
| `t3-localization-factory` | product / i18n | the whole docs tree translated, voice intact | chained fan-outs · jq `transpose` zip |
| `t3-config-drift-sentinel` | SRE / platform | only UNSANCTIONED prod drift wakes anyone | RFC 7396 merge + RFC 6902 diff · blake3 |
| `t3-pr-review-fanout` | engineering | one read-only review agent **per changed file** | `for_each`+`agent:` swarm · default-deny tools |
| `t3-resume-screener` | HR / recruiting | one local-model rubric per candidate · PII stays home | `ollama/…` · `for_each` · schema enums · jq sort_by |
| `t4-deep-research-brief` | research / VC | plan → budgeted agent → thinking synthesis | plan-then-execute · budgets · `thinking:` |
| `t4-incident-war-room` | SRE / on-call | the postmortem drafts itself — after recovery is PROVEN | `nika:wait` settle · assert · `on_finally:` |
| `t4-ceo-monday-brief` | founders / execs | the brief that reports its own LLM bill | 3-branch gather · `nika:inspect` cost |
| `t4-release-train` | devops / release | gates → human GO → hold until the window → ship · verify | `nika:wait until:` · `nika:date diff` · `nika:prompt` |

## Clone and run

Every T1 and T2 file runs from the repo root with **no arguments and no
setup**. Run them exactly as their `Run ·` header line says — paths resolve
from your working directory, not from the file's, so the repo root is the
contract:

```bash
nika check examples/showcase/t2-csv-chart-report.nika.yaml --native-strict   # 0 findings, 0 hints
nika run   examples/showcase/t2-csv-chart-report.nika.yaml                   # artifacts land in out/
rm -rf out                                                                    # back to clean
```

- **Fixtures are committed.** `fixtures/` holds the sample data these files
  read — a sales CSV, a dirty orders batch, a meeting transcript, two PNGs, a
  contract, a ticket queue, a changelog. A header with **no `Needs ·` line
  needs nothing**: clone, run, watch it work. A `Needs ·` line names the one
  effect that is genuinely external (a git repo, a network host, ollama seats).
- **Every write lands under `out/`.** Nothing in this corpus writes into your
  source tree, so `rm -rf out` always restores a clean checkout. Reads point
  at `fixtures/`; repoint the `const:` at your own data and move the matching
  `permits.fs.read` literal with it — `permits:` cannot interpolate
  (`NIKA-AUTH-007`), and that is the point: a boundary you can read is a
  boundary a reviewer can check.
- **Offline by default.** Any `infer:` file rehearses with `--model mock/echo`
  — deterministic, zero keys. Files that reach the network carry an
  `on_error: recover:` sample so the run stays green with no network at all;
  a recovery stands in for the RAW response, so the `output:` jq bindings run
  over it unchanged.
- **One deliberate red · `t2-release-radar`.** `nika check` exits 2 there with
  `NIKA-SEC-009`, and the run refuses to start with it. The finding
  over-approximates (the "egress" it names is a local state-file write); the
  decision to leave it red rather than bolt on a `nika:prompt` to silence it
  is recorded in the engine repo at
  `docs/plans/2026-07-28-verdict-coverage.md` (§DECIDED · SEC-009). A red gate
  reporting something true is the honest state, and the file says so in its
  own header.

## Conventions (same gate as the foundation set)

- `# SPDX-License-Identifier: Apache-2.0` header + schema hint line
- `ollama/qwen3.5:4b` is the showcase model: every file leads local,
  with one deliberate exception class — strict-schema showcases pick a
  NON-thinking model (`ollama/llama3.2:3b`): a thinking model can burn
  the whole `max_tokens` in its think block before the JSON (engine#428).
  zero key, recorded-as-run. Cloud providers appear only as per-task
  overrides or swap hints, never as the envelope default. Where the
  data is sensitive the local model is the point: sovereignty is a
  feature, show it
- the offline story, honestly: `nika check` needs zero network on every
  file, and any `infer:` showcase dry-runs with `--model mock/echo`
  (deterministic, zero model) — see **Clone and run** above. The `agent:`
  showcases (`t3-pr-review-fanout` · `t4-deep-research-brief`) are the
  exception — mock echoes text and never *calls* a tool, so the ReAct loop
  does zero rounds under it: exercising the agent needs a real tool-calling
  model (the pinned local `qwen3.5` qualifies)
- every file is a conformance input · `python conformance/runner.py all`
  MUST stay green · one verb per task · snake_case ids · every
  `${{ tasks.X }}` reference at the `with:` boundary (the binding IS the edge)
- these files are the **single source** for the YAML shown in the
  public docs (`nika-docs` examples pages) and the website use-cases
  explorer — projected, never hand-copied
  (`scripts/showcase-projector.py` · `--check` is the drift gate)

🦋 *The showcase pack · 4 tiers · every stdlib builtin exercised across the example corpus · manifest = the contract.*
