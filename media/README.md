# Nika media

Official visual assets for the README, docs, website and social surfaces.

## Current README film

**From intent to proof:** [30-second MP4 with sound](videos/intent-to-proof.mp4),
[README GIF](gifs/intent-to-proof.optimized.gif) (a 12-second cut of five
beats; the whole film does not fit the GIF budget at a watchable frame rate),
[poster](posters/intent-to-proof.png) and
[contact sheet](storyboards/intent-to-proof.png).

One request travels the target architecture. A sentence becomes six
obligations, grounded in the observed world. A Foundry block is proposed, and
Rust proves the semantic plan. The one unknown, a currency, is asked rather
than guessed. The plan is then lowered deterministically into a `.nika`
program, checked, and closed against the intent. A human consents to the exact
revision; the runtime executes one effect and returns the result with proof.

This is an **architecture illustration with fixture data**. The program it
draws is `scripts/media/fixtures/invoice-payments.nika`, which passes `nika check`.
The check rows, task durations, and computed totals (€228.00) are captured from
the real binary. The payment itself is illustrative, and some components carry
their target-design names. The ledger of what is real, the 120 BPM timing
system, the reading-time audit, and the render steps are in
[the film README](../scripts/media/motion/intent-to-proof/README.md).

## Earlier films

**From intent to impact** ([60-second MP4](videos/intent-to-impact.mp4),
[GIF](gifs/intent-to-impact.optimized.gif), [poster](posters/intent-to-impact.png),
[contact sheet](storyboards/intent-to-impact.png)) was the README film before.
It is an illustrative product film with fictional data and no live
integrations: checkout research, a reviewable file, preflight, bounded parallel
work, approval and concrete results. Its source and timing tests are in
[its README](../scripts/media/motion/intent-to-impact/README.md).

The older `intent-dag-proof` film also remains an archived alternative.

## Feature clips

Short clips for READMEs, the docs and the site, each an MP4 and WebM, a
README GIF and a poster, rendered by the same engine as the README film
from captured output:

| Clip | What it shows |
|---|---|
| `nika-hero` | the audit, then a real local model run and the items it wrote (also `media/nika-hero.gif`) |
| `static-check-fix` | `nika check` catching two defects, the real fix, the clean re-check |
| `permits-audit` | the file's boundary drawn from its `permits:`, the escape the check catches, the widened fence |
| `on-error-recover` | a missing live feed absorbed by `on_error: recover`, the stale output and the recorded failure |
| `dag-execution` | a workflow's graph from `nika inspect`, its waves from `nika check` |
| `chat-to-workflow` | a request retyped every Monday, kept as a file that runs |
| `editor-diagnostics` | what `nika lsp` publishes, fixed by one keystroke |
| `workflow-gallery` | the jobs `nika try` lists |
| `full-loop` | the README's first file: compile, check, run, verify |
| `pr-check-comment` | a pull request's sticky nika-action comment: one finding, the fixing push, the clean verdict and its graph |
| `trace-proof` | a run's hash-chained trace verified intact, then one changed byte refused at the next line |
| `spec-anatomy` | one checked file with all nine envelope keys and the four verbs, labelled in the spec's words |
| `agent-plugin` | a coding agent's draft refused by the check, its repair, and a rehearsal run of the kept file |
| `first-session` | the Session from one sentence to a checked file, `yes`, `run it` and `/proof`, with no AI model |
| `cost-ceiling` | a workflow priced before any call, capped by one `max_tokens` line, and refused by a budget below its ceiling |
| `approval-gate` | a `nika:prompt` gate asking at a terminal, pausing in CI with exit 4, and resumed by the line it prints |

How they are made and checked is in
[the film README](../scripts/media/motion/intent-to-proof/README.md#feature-clips).

## Rules

- **No fake commands.** Every command shown in an asset exists in the CLI.
- **No fake output.** Terminal text and diagnostics are captured from the
  real binary and its language server (`scripts/media/capture-transcripts.sh`
  → `media/raw/`). The chat-to-workflow and nika-hero runs are a real local
  inference (`ollama/llama3.2:3b`); a `mock/echo` run is a rehearsal and
  says so. A clip whose story takes several commands owns its capture
  script in `scripts/media/capture/`; the pr-check-comment capture replays
  nika-action's own renderer from a checkout of that repository
  (`NIKA_ACTION`). The Session and the approval gate are driven through a
  pseudo-terminal, as a person would type them.
- **Every complete runnable workflow shown passes `nika check`**, except the
  deliberately broken fixtures (`broken-pr-review` in static-check-fix and
  editor-diagnostics, `permits-escape` in permits-audit), whose failure is
  the point. `scripts/media/validate-media.sh` enforces both directions.
- **Illustrations say so.** The product film labels fictional data and folded
  source excerpts; a clip's plate names what it illustrates. None may be
  described as a successful real integration.
- **Budgets** · README GIF ≤ 8 MB · poster PNG ≤ 1 MB.
- **Never edit exports by hand.** Edit the clip, scene or fixture, then
  regenerate.

## Layout

```
media/
  brand/     nika-logomark.svg — official butterfly mark (geometry preserved)
  gifs/      *.optimized.gif   — README embeds (clips: 960 px · 12 fps · ≤8MB)
  videos/    *.mp4 + *.webm    — docs + website embeds
  posters/   *.png             — static frame per animation (og:image, video poster)
  storyboards/*.png            — six-beat visual QA contact sheets
  social/    *.png             — share cards: the film's end card (og-card,
                               github-social-preview) and two clips' last
                               frames (check-before-run, chat-vs-keeping)
  raw/       *.txt + *.json    — captured CLI and language-server output
                               (the source of truth)
  nika-hero.gif                — the nika-hero clip at the path other repos hotlink
  clip-sources.json            — the clip file each clip's media were rendered
                               from (sha256, written by the renderer)
```

## Regenerate

```sh
bash scripts/media/capture-transcripts.sh     # refresh the captured output
cd scripts/media/motion/intent-to-proof
npm ci && npm run fonts                       # once
npm run clip -- all                           # every feature clip
bash ../../validate-media.sh                  # honesty + budget gate
```

The README film and the feature clips are code: every frame is a function of
time, drawn with Skia (see [the film README](../scripts/media/motion/intent-to-proof/README.md)).
The archived `intent-dag-proof` film is an HTML scene rendered in headless
Chrome by `scripts/media/render-motion.mjs` (`npm ci` in `scripts/media`;
`CHROME_PATH` selects a Chromium other than the system Chrome).

Workflow fixtures live in `scripts/media/fixtures/` and are gated in both
directions (a broken half must keep failing `nika check`, a fixed half must
stay clean). Every clip in `clips/` must ship its GIF, MP4, WebM and poster.

## Embedding

GitHub README → use the optimized GIF:

```md
![alt text](media/gifs/<clip>.optimized.gif)
```

Docs (Mintlify) and the website → prefer video with the poster:

```html
<video autoPlay muted loop playsInline poster="/images/posters/<clip>.png">
  <source src="/videos/<clip>.webm" type="video/webm" />
  <source src="/videos/<clip>.mp4" type="video/mp4" />
</video>
```

Social / OG cards → the posters are 1600×900 stills designed to work alone.
