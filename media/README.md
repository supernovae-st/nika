# Nika media

Official visual assets for the README, docs, website and social surfaces.
The feature clips below are captured from the released `nika 0.123.0
(0bfde3765)` ([`raw/nika-version.txt`](raw/nika-version.txt)); each clip's
plate names the release it was captured with.

## README hero

**The Session** (`first-session`): [README GIF](gifs/first-session.optimized.gif),
[MP4](videos/first-session.mp4), [WebM](videos/first-session.webm),
[poster](posters/first-session.png). One sentence becomes a checked
`.nika` file; `yes` saves it, `run it` keeps the two paid rows, and `/proof`
shows what the run recorded. The Session's plain view, restyled, with no AI
model involved; the folders and the data flow are illustrations.

## Architecture film (illustration)

**From intent to proof:** [30-second MP4 with sound](videos/intent-to-proof.mp4),
[GIF](gifs/intent-to-proof.optimized.gif) (a 12-second cut of five
beats), [poster](posters/intent-to-proof.png) and
[contact sheet](storyboards/intent-to-proof.png).

This is a **design illustration with fixture data**, not a recording, and
it is no longer the README film. It follows one request through the
target architecture the 0.123 work converged toward. Some components it
names (Reflex, CLM, a single READY certificate) are not in 0.123 and are
not planned, and the payment it shows as done did not run: the capture
declined the gate. What is real is listed in
[the film README](../scripts/media/motion/intent-to-proof/README.md): the
program it draws, `scripts/media/fixtures/invoice-payments.nika`, passes
`nika check`, and its check rows, task durations and totals (€228.00) are
captured from the binary.

## Earlier films (archived)

**From intent to impact** ([60-second MP4](videos/intent-to-impact.mp4),
[GIF](gifs/intent-to-impact.optimized.gif), [poster](posters/intent-to-impact.png),
[contact sheet](storyboards/intent-to-impact.png)) was the README film before
that. It is an illustrative product film with fictional data and no live
integrations: checkout research, a reviewable file, preflight, bounded parallel
work, approval and concrete results. Its source and timing tests are in
[its README](../scripts/media/motion/intent-to-impact/README.md).

The older `intent-dag-proof` film also remains an archived alternative.
Neither is maintained or shown on a current surface.

## Feature clips

Short clips for READMEs, the docs and the site, each an MP4 and WebM, a
README GIF and a poster, rendered by the same engine as the architecture
film from captured output:

| Clip | What it shows |
|---|---|
| `first-session` | the Session from one sentence to a checked file, `yes`, `run it` and `/proof`, with no AI model (the README hero) |
| `static-check-fix` | `nika check` catching two defects, the real fix, the clean re-check |
| `nika-hero` | the audit, then a real local model run and the items it wrote (also `media/nika-hero.gif`) |
| `trace-proof` | a run's hash-chained trace verified intact, then one changed byte refused at the next line |
| `cost-ceiling` | a workflow priced before any call, capped by one `max_tokens` line, and refused by a budget below its ceiling |
| `approval-gate` | a `nika:prompt` gate asking at a terminal, pausing in CI with exit 4, and resumed by the line it prints |
| `full-loop` | the README's first file: compile, check, run, verify |
| `permits-audit` | the file's boundary drawn from its `permits:`, the escape the check catches, the widened fence |
| `on-error-recover` | a missing live feed absorbed by `on_error: recover`, the stale output and the recorded failure |
| `dag-execution` | a workflow's graph from `nika inspect`, its waves from `nika check` |
| `chat-to-workflow` | a request retyped every Monday, kept as a file that runs |
| `spec-anatomy` | one checked file with all nine envelope keys and the four verbs, labelled in the spec's words |
| `agent-plugin` | a coding agent's draft refused by the check, its repair, and a rehearsal run of the kept file |
| `pr-check-comment` | a pull request's sticky nika-action comment: one finding, the fixing push, the clean verdict and its graph |
| `editor-diagnostics` | what `nika lsp` publishes for any LSP client, fixed by one keystroke (the editor is an illustration) |
| `workflow-gallery` | the jobs `nika try` lists |
| `typescript-client` | a Node app running a checked workflow through `@supernovae-st/nika` 0.123.0 and the engine it bundles, its typed result, and the receipt the engine verifies |
| `sdk-doors` | the TypeScript SDK's two doors: `new Nika()` starts the engine the package bundles and `new Nika({ url, token })` reaches a `nika serve`, the same methods with other options on two lines (an `idempotencyKey` in place of `maxCostUsd`); the check that refuses a file before any run, the events, the sealed record and `traceVerify()` on both (the cards, lanes, packets and pacing are illustration) |

Two clips show a local model run (`ollama/llama3.2:3b`) that is older than
their check: `nika-hero` and `chat-to-workflow`. That run was captured with
the release named in [`raw/run-meeting-version.txt`](raw/run-meeting-version.txt)
(`nika 0.109.2`), and both plates say so.

How they are made and checked is in
[the film README](../scripts/media/motion/intent-to-proof/README.md#feature-clips).

## Rules

- **No fake commands.** Every command shown in an asset exists in the CLI.
- **No fake output.** Terminal text and diagnostics are captured from the
  real binary and its language server (`scripts/media/capture-transcripts.sh`
  → `media/raw/`). The chat-to-workflow and nika-hero runs are a real local
  inference (`ollama/llama3.2:3b`), refreshed only where an Ollama server
  answers and `NIKA_MEDIA_LOCAL_RUN` is not `keep`; otherwise the committed
  run stays, with the release it was captured with. A `mock/echo` run is a
  rehearsal and says so. A clip whose story takes several commands owns its
  capture script in `scripts/media/capture/`; the pr-check-comment capture
  replays nika-action's own renderer from a checkout of that repository
  (`NIKA_ACTION`), and the typescript-client capture runs the TypeScript
  package's quick start from a built nika-client checkout
  (`NIKA_CLIENT_DIR`), on the engine that package bundles. The sdk-doors
  capture (`node scripts/media/capture/sdk-doors.mjs`) installs the
  published `@supernovae-st/nika` from the npm registry in a scratch app,
  runs the README's `demo.mts` and drives both doors: the engine the package
  bundles, and a `nika serve` it starts on loopback with a 0600 token file it
  never prints, where it also records what the server door refuses of the
  quick start's own options. The Session and
  the approval gate are driven through a pseudo-terminal, as a person
  would type them.
- **Every complete runnable workflow shown passes `nika check`**, except the
  deliberately broken fixtures (`broken-pr-review` in static-check-fix and
  editor-diagnostics, `permits-escape` in permits-audit, `list-files.nika`
  in sdk-doors), whose failure is the point. `scripts/media/validate-media.sh`
  enforces both directions for the fixtures; the sdk-doors clip refuses to
  render unless its capture records the refusal and the clean check.
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
# the release to capture first on PATH, with node, python3 and GNU timeout
NIKA_MEDIA_LOCAL_RUN=keep \
NIKA_ACTION=../nika-action NIKA_CLIENT_DIR=../nika-client \
  bash scripts/media/capture-transcripts.sh   # refresh the captured output
node scripts/media/capture/sdk-doors.mjs      # sdk-doors: the published package, from the registry
cd scripts/media/motion/intent-to-proof
npm ci && npm run fonts                       # once
npm run clip -- all                           # every feature clip
bash ../../validate-media.sh                  # honesty + budget gate
```

Run the capture with a scratch `HOME` and `NIKA_KEYCHAIN=off`, so no key
or setting of the capturing machine reaches a transcript. Without
`NIKA_ACTION` or `NIKA_CLIENT_DIR`, the committed pr-check-comment and
typescript-client captures stay as they are.

The architecture film and the feature clips are code: every frame is a
function of time, drawn with Skia (see [the film README](../scripts/media/motion/intent-to-proof/README.md)).
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
