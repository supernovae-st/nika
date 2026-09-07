# Intent → Proof · 19.2-second storyboard

Audience: a junior developer who has never seen Nika. The film must answer four
questions in one silent viewing: What do I give it? What does it create? What
runs? What do I get?

## Story beats

| Time | Beat | What the viewer sees | Question answered |
|---:|---|---|---|
| 0.0–3.2s | Intent writes itself | “Turn this meeting into clear action items.” Words appear character by character; `meeting`, `clear`, and `action items` receive distinct semantic color. | What do I give Nika? |
| 3.2–5.3s | Capabilities connect | Apps, MCP, APIs, and built-ins connect to the request as four explicit inputs—not a logo cloud. | What can it use? |
| 5.3–8.5s | Intent becomes source | The sentence resolves into `meeting-actions.nika.yaml`. Beside the real syntax, plain-language cards say: read the meeting, ask one model, write the result. | What does Nika create? |
| 8.5–11.2s | Check before run | The file compiles into the canonical three-node DAG from `nika inspect`; a receipt exposes three steps, two allowed tools, and one model. | Can I inspect it first? |
| 11.2–15.3s | Run the graph | A single signal travels Read → Extract → Save. Each node has exactly one state: waiting, running, done. | What actually runs? |
| 15.3–18.6s | Result + proof | The real action items appear beside a verified run receipt and the real shortened trace hash. | What do I get, and can I trust the record? |
| 18.6–19.2s | Clean loop | The UI fades into GitHub’s exact dark background before restarting. | — |

## Comprehension rules

- One example, one camera, one left-to-right causal story.
- No fake terminal, no “try” detour, no command parade.
- Product words precede implementation words; YAML is translated beside itself.
- Color has a stable job: violet = intent/model, blue = structure/check,
  mint = completed/verifiable.
- Every transition preserves meaning: sentence → file → graph → output/receipt.
- The output occupies more area than the syntax. Value is the climax.
- Animation uses only opacity and transforms except SVG path drawing; the export
  remains smooth and deterministic.

## Truth sources

- Workflow: `scripts/media/fixtures/meeting-actions.nika.yaml`
- Canonical graph: `nika inspect` format 3 — `transcript → extract → save`
- Validation: `nika check` — 3 tasks, 2 allowed tools, 1 model
- Output: `media/raw/action-items.json`
- Trace: `media/raw/run-meeting.txt` — 15 events, complete chain,
  `66549215b46bd4d6b99d2413a6a65f90722c135a6b73edf4b134272049e18d08`

## Brand lock

- Canvas: GitHub dark `#0f1216` sampled from the supplied screenshot.
- Typeface: bundled Geist Sans + Geist Mono; no network font dependency.
- Mark: official Nika butterfly geometry and `#9fd0ff` fill from the public
  Nika brand asset; present in the persistent masthead and workflow file.
- Surfaces: GitHub/Linear restraint, 1 px borders, tight radii, no glassmorphism.
- Motion: GSAP timeline with SplitText, DrawSVG, and MotionPath; 30 fps video,
  20 fps README GIF at 1200 px, 1600×900 source.
