# Jobs · industry workflows, simplest → epic

> Complete, spec-correct workflows (the count lives in [`manifest.yaml`](manifest.yaml)) that answer one question per
> industry · **« what would MY Monday look like with this? »** Every
> file passes the same conformance gate as the foundation examples
> (`python conformance/runner.py all`) — schema + DAG cross-refs +
> stdlib surface. Together with the foundation set they exercise
> **every stdlib builtin** (verified by the conformance + coverage sweep).

The [foundation examples](README.md) teach the *language*
construct-by-construct. The jobs teach the *life* — real work, ordered
simplest → epic so a newcomer climbs from a 4-task starter to a
multi-agent swarm without ever leaving validated ground.

---

## The workflows

| File | Industry | The wow | Key constructs |
|---|---|---|---|
| [`standup-digest`](standup-digest.nika.yaml) | engineering | the standup note writes itself from real commits | parallel start · `nika:date` · exec→infer |
| [`meeting-actions`](meeting-actions.nika.yaml) | every office | transcript → tracker-ready typed action items | `infer.schema:` · input `default:` vs literal permit |
| [`price-watch`](price-watch.nika.yaml) | e-commerce / personal | a price alert with **zero** model calls | `extract:` jq · CEL `when:` · `egress:` **and** `net.http` |
| [`social-repurpose`](social-repurpose.nika.yaml) | marketing / creators | one post → thread + LinkedIn + newsletter, in parallel | diamond DAG · `with:` aliasing |
| [`og-images`](og-images.nika.yaml) | marketing / content | brief in → OG PNG + provenance manifest out, one task | `nika:image_generate` · dir-writing permit |
| [`image-fx-batch`](image-fx-batch.nika.yaml) | creators / media | a folder of photos → deterministic art, byte-identical forever | `nika:glob` · jq-derived paths · `nika:image_fx` ops chain |
| [`release-notes`](release-notes.nika.yaml) | engineering / devrel | git log → typed notes → a CHANGELOG copy edited in place | `exec` `recover:` · `nika:edit` · gated notify |
| [`seo-content-brief`](seo-content-brief.nika.yaml) | SEO / content | a brief that beats the competitor's best page | chained fetch modes · `recover:` · CEL indexing |
| [`invoice-chaser`](invoice-chaser.nika.yaml) | finance / freelance | overdue reminders drafted · NOTHING sent without a yes | `nika:convert` · `nika:prompt` gate · `size()` |
| [`support-triage`](support-triage.nika.yaml) | customer support | the overnight queue triaged before coffee | schema-over-list · jq post-filter · `nika:uuid` |
| [`contract-guard`](contract-guard.nika.yaml) | legal / compliance | the contract **never leaves the machine** (local model) | `ollama/…` · `nika:validate` + `nika:assert` |
| [`etl-quarantine`](etl-quarantine.nika.yaml) | data engineering | bad batches degrade to quarantine · the pipeline lives | `on_error: recover:` · `nika:validate` · jq group_by |
| [`model-bench`](model-bench.nika.yaml) | engineering / model selection | the same question, three local models, one MEASURED table | per-task `infer.model:` · `duration_ms` as data · jq fan-in |
| [`release-radar`](release-radar.nika.yaml) | devops / dependencies | only the NEW ships reach you — human-gated since it crosses the trifecta | `mode: feed` · state-file diff · RFC 6902 |
| [`csv-chart-report`](csv-chart-report.nika.yaml) | data → picture | paste the spreadsheet, get the slide — offline, deterministic | `nika:convert` · jq group_by · `nika:chart` |
| [`transcript-shownotes`](transcript-shownotes.nika.yaml) | podcasts / meetings | raw transcript → typed show-notes, ONE bounded infer | `infer.schema:` strict · typed→markdown |
| [`bookmark-triage`](bookmark-triage.nika.yaml) | personal / research | the bookmark pile triaged — dead links survive the batch | `mode: metadata` · resilient `for_each` · recover |
| [`competitor-radar`](competitor-radar.nika.yaml) | strategy / PMM | everything they shipped last week, one brief | `for_each` · `for_each.max_parallel` · retry · fan-in |
| [`localization-factory`](localization-factory.nika.yaml) | product / i18n | the whole docs tree translated, voice intact | chained fan-outs · jq `transpose` zip |
| [`config-drift-sentinel`](config-drift-sentinel.nika.yaml) | SRE / platform | only UNSANCTIONED prod drift wakes anyone | RFC 7396 merge + RFC 6902 diff · blake3 |
| [`pr-review-fanout`](pr-review-fanout.nika.yaml) | engineering | one read-only review agent **per changed file** | `for_each`+`agent:` swarm · default-deny tools |
| [`resume-screener`](resume-screener.nika.yaml) | HR / recruiting | one local-model rubric per candidate · PII stays home | `ollama/…` · `for_each` · schema enums · jq sort_by |
| [`deep-research-brief`](deep-research-brief.nika.yaml) | research / VC | plan → budgeted agent → thinking synthesis | plan-then-execute · budgets · `thinking:` |
| [`incident-war-room`](incident-war-room.nika.yaml) | SRE / on-call | the postmortem drafts itself — after recovery is PROVEN | `nika:wait` settle · assert · `after: {…: terminal}` always-pattern |
| [`ceo-monday-brief`](ceo-monday-brief.nika.yaml) | founders / execs | the Monday brief assembles itself — and the human decision sits at the ROOT of the lethal trifecta | 3-branch gather · root `nika:prompt` gate · capped synthesis |
| [`release-train`](release-train.nika.yaml) | devops / release | gates → human GO → hold until the window → ship · verify | `nika:wait until:` · `nika:date diff` · `nika:prompt` |

## Clone and run

Every T1 and T2 file runs from the repo root with **no arguments and no
setup**. Run them exactly as their `Run ·` header line says — paths resolve
from your working directory, not from the file's, so the repo root is the
contract:

```bash
nika check examples/csv-chart-report.nika.yaml --native-strict   # 0 findings, 0 hints
nika run   examples/csv-chart-report.nika.yaml                   # artifacts land in out/
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
  a recovery stands in for the RAW response, so the `extract:` jq bindings run
  over it unchanged.
- **Every job checks green — including `release-radar`.** It crosses the
  lethal trifecta (private read + feed ingress + state-file write), so it
  carries the canonical human gate (`after: {approve: success}` at the head
  of the flow · NEP-0002 v2.2) instead of shipping red. The always-red
  SEC-009 witness the conformance lane still needs lives beside the other
  conformance inputs at
  `conformance/envelope/trifecta-realized-flow-ungated.nika.yaml` — an
  attack shape stays an attack shape; a TEACHING page checks green.

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
  showcases (`pr-review-fanout` · `deep-research-brief`) are the
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

🦋 *The jobs pack · ordered simplest → epic · every stdlib builtin exercised across the example corpus · manifest = the contract.*
