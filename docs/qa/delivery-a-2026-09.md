# Conversational delivery A: qualified local scope

Qualified on 25 September 2026 (Europe/Paris): source `4c728c98059ac4111f09b8bc3e30ada013fc026f`, `nika 0.120.3 (4c728c980)`, development-profile executable SHA-256 `a327faf1da06b1ea57fa12e23184006f2d321a024e6dee31222beea6c9172961`. The qualified executable remains pinned separately from later documentation-only commits.

This records a bounded macOS qualification of the conversational Session path. It does not announce all of V7, a general reliability rate, a package release, or the larger intent campaign.

## What was exercised

The installed executable was started with the bare `nika` command from a fresh login shell. Persistent provider configuration and an operator-installed technical knowledge snapshot (not distributed by the public installers) were used; the driver supplied no provider credentials or temporary authoring settings.

| Journey | Request and oracle |
| --- | --- |
| Filter and aggregate | French request over a CSV; preserve its header; write only paid rows and a separate numeric total of 20. |
| Source-grounded summary | Three bullets from synthetic project notes through real DeepSeek inference; inspect all stated facts against the source. Wording may vary. |
| Revise and resume | Change the CSV destination while preserving the confirmed-order filter and the independent total; close and reopen the pending draft, review it again, save and explicitly run; exact rows C101/C103, total 40, old destination absent. |
| Decimal paths variant | Different field names and nested paths; exact retained rows and numeric total 11.75. |
| JSON variant | Filter error objects without changing fields; count 2; an instruction-like note remains data and creates no extra file. |
| Bounded decision | Jev chooses lookup from search/lookup/none, followed by an identifier clarification and an exact ticket-42 result. This is compiler routing, not knowledge selection. |

Save was checked separately from Run: saving the proposed workflow did not create its output files. Source hashes, destinations and deterministic result contents were compared independently. Run trace verification checks recorded integrity and effects; it does not certify semantic fidelity.

## Composition and bounds

DeepSeek receives the complete request, relevant project observations and selected technical knowledge in native authoring. Revision retains the original request, base workflow, change and clarification answers. The knowledge receipt records what was actually presented; presentation alone is not proof of a quality improvement.

Session authoring has a 180-second deadline per API call, an initial 16,384-output-token limit, a maximum of 32,768, and at most three native repairs. Truncation can consume a bounded repair; transport uncertainty does not trigger blind replay. The selected Jev adapter allows at most three decisions per compile, each with a 20-second deadline and no retry. These bounds are distinct from monetary admission and the explicit Run ceiling.

All six journeys in the table passed on this executable. The revision changed only the two CSV destination occurrences. Close/reopen preserved its exact workflow and goal, with fresh consent and no new authoring call. A Run with a zero ceiling was followed by new native authoring; a later close/reopen admitted a conversation call without resetting history. Deterministic outputs were compared exactly; the summary's three bullets contained all source facts. All six run trace chains verified.

The installed Foundry retained 65 referenced files. Native filter authoring presented 13 references (15,783 bytes), and revision presented 13 (18,333 bytes). Jev made one HTTP-200 lookup decision from search/lookup/none, leading to the observed lookup workflow.

Local checks included 335 Session library tests (three explicit external tests ignored), 38 public monetary boundary/preparation/restart tests, Session Clippy, formatting, size limits, and SDK OneDoor and Recovery on the pinned executable. Independent read-only review found no blocker in the Run-scope change.

This installed attempt recorded 12 product API calls, 78,218 input and 18,301 output tokens (13,592 reasoning tokens are included in output). The known catalog subtotal was USD 0.027219672; one Jev call is unpriced. Retained qualification attempts, including earlier failures, repairs and diagnostics, recorded 63 calls, 452,514 input and 123,628 output tokens, a known subtotal of USD 0.187597788 and six unpriced calls. Development agents and the earlier orchestrator campaign are outside that accounting. The summary Run took 23.356 seconds; deterministic Runs took 26–49 milliseconds, excluding startup, review and preparation.

## Limits

These are finite synthetic examples on one installed candidate. They do not establish correctness for arbitrary requests, all platforms, every provider, or the complete Foundry. Text summaries received source-based review, not an exact-string oracle. Catalog cost estimates are not invoices; Jev billing and development-agent consumption remain separate from priced product-call subtotals.

The candidate is installed reversibly for the qualifying operator. No universal installer or full V7 release is claimed by this note.

## Release-profile follow-up: public authoring path

On 25 September, a separate local release-profile candidate was exercised without
Jev or a knowledge snapshot: source `1f7622ecd148a2516c9dd458d946f4d464da901c`,
`nika 0.121.0 (1f7622ecd)`, SHA-256
`7335f025d27b81b6cdc5a90e1353c72ccfc4355ae740e1306204334939ea8d2f`.
It was built from a clean tree for `aarch64-apple-darwin` with
`local-infer,access-harness`. This was a frozen local executable, not a downloaded
release archive or the final installation.

Three native terminal journeys passed in isolated HOME directories with
DeepSeek. The CSV filter produced exactly Aster/Cedar and total 20. The summary
produced three bullets containing only the synthetic source's facts, with one
recorded `deepseek/deepseek-flash` inference. The revision changed only the two
CSV destination occurrences, survived close/reopen and `/restore`, then produced
C101/C103 and total 40, with the obsolete destination absent. Saving created no
result files. The restored draft required a new Run ceiling; its explicit
zero-ceiling run passed. Source files remained unchanged.

| Run | Trace identifier | Verified chain head |
| --- | --- | --- |
| Filter | `2026-09-25T09-10-33Z-1e6b` | `c91dcc757fb1f3a0aa0ba609dc82d831e6ad548ba9c4282724cfa0ddc576c357` |
| Summary | `2026-09-25T09-13-08Z-e5d8` | `c01c43451a3bb1a2e362591b808e6db4935ce6f2ef3bb9ad1c8dfb9f031e61a4` |
| Revision | `2026-09-25T09-17-30Z-fb4a` | `9575479a7196095ed2575ec875f36e24eb58bd21219a597b657227082eb05b1f` |

All three chains independently verified; the runs were **unsealed**, as expected
in keyless isolated homes. Eight authoring calls had a catalog subtotal of
approximately USD 0.024537; the summary inference recorded USD 0.009652536,
below its USD 0.25 Run ceiling. These are estimates, not invoices or a global
spending cap. There was one factual clarification for the runtime model.

The same bytes passed `funnel-e2e.sh`, `trust-battery.sh`, all 244 native
authoring-gauntlet cases, and the project-schema check's 37 workflow cases plus
seven negative judge controls. These checks do not substitute for repository CI:
23 integration tests still failed at that source revision and were assigned
separate repairs. This follow-up establishes the three observed journeys only;
it does not qualify changed source, all providers, or a future public archive.

## Operation and downgrade notes

Review and save with `yes` before issuing a separate Run. A Run ceiling stated while a proposal still awaits consent is outside this qualified sequence. Direct Session API callers must answer a pending gate before requesting another Run.

New history records identify closed Run operations separately from Session monetary amendments. This reader accepts earlier histories conservatively; older executables cannot read the new Run operation and refuse that history. Installation rollback restores the executable and configuration, not a reverse history migration. Preserve these records and use this candidate to resume them; deleting records is not a way to reset spending uncertainty.
