# Nika: from intent to proof

A 30-second, code-generated motion film about the V9 target architecture:
a human intention becomes a verified program, runs under human consent, and
leaves a real result with evidence. It follows one request through every stage:

> Read my invoices. Ignore rejected ones. Sum by customer. Pay only after I approve.

Every frame and every sound comes from code. There is no stock footage, no
image or video model, and no licensed music. Pictures come from Skia
(`@napi-rs/canvas`), sound from a NumPy synthesizer, and ffmpeg encodes both.

## What is real and what is illustration

The film is an **illustration of the architecture with fixture data**. It is
not a screen recording. Where it shows a fact about Nika, that fact is loaded
from a real source at render time (`src/facts.mjs`).

| On screen | Source |
|---|---|
| The `.nika` program, its line numbers and folds | `scripts/media/fixtures/invoice-payments.nika`, read at render time. It passes `nika check`. |
| `nika check` rows | `captured/nika-check.txt`, from the real binary |
| Task durations in the runtime (3 ms, 7 ms…) | `captured/nika-run-declined.txt`, a real `nika run` with `--answer approval=false` |
| Totals ACME 12000 · BRAVO 10800 (cents) = €228.00 | The same real run: `nika trace peek … totals` (`captured/nika-trace-totals.txt`) |
| "chain intact" | `nika trace verify` on that run (`captured/nika-trace-verify.txt`) |
| `program` hash | sha256 of the fixture program |
| `plan` hash | sha256 of `captured/plan.json`, the film's plan as data |
| `proposal` hash | blake3 of the exact consent preview text (`captured/hashes.json`) |
| Session acts NEW WORK · ANSWER · DISCUSS · REQUEST RUN | `TurnAct` in `crates/nika-session/src/turn.rs` |
| `nika:read` · `nika:jq` · `nika:prompt` gate · `nika:fetch` POST guarded by `when: ${{ with.approved == true }}` | The idiom of the compiler's own READY output (`crates/nika-compile/tests/fixtures/contract/money-approval-answered.json`) |

What is illustration:

- **The data is fictional.** It lives in `scripts/media/fixtures/invoices.json`.
- **The payment is illustrative.** The fixture targets a documentation host (RFC 2606), which the transport never dials. The capture declined the gate, so no effect ran. The film shows the approved path, and it does not claim that run happened.
- **Some target components are named as the V9 design names them.** They are not all shipped as single artifacts in the current release:

| Film name | In the current source |
|---|---|
| Observed world | `observed_world` |
| Foundry | An optional knowledge snapshot |
| NikaBlock | Checked `block` entries in that snapshot |
| Reflex, CLM | Target components, not in the source |
| Jev / DecisionSeat | An optional decision model plus the `DecisionSeat` trait |
| Semantic plan | The private `Plan` in `nika-compile-reader` |
| Meaning closure | The session's `/meaning` dispositions (`Represented`…) and fidelity laws |
| READY certificate | Folds together `CompileStatus::Ready`, the obligation ledger, and the `plan_sha256` / `candidate_sha256` provenance |

The HUD says `ILLUSTRATION · FIXTURE DATA` throughout.

## Timing: one clock for picture and sound

`src/timeline.mjs` is the single source of timing. It runs at 120 BPM, so a
beat is 30 frames at 60 fps and the film is exactly 15 bars. Every cue is a
beat position. The renderer reads it, and `soundCues()` hands the same numbers
to `audio/score.py` through `.cache/timeline.json`. Nothing is timed twice, so
the edit and the music cannot drift.

## Creative rules

The edit follows the creative laws of [/brag](https://github.com/latent-spaces/brag),
a launch-video skill for coding agents, applied inside this film's own
pipeline:

- **The hook is the request.** The sentence types from the first frames. The
  reader sweep and the fracture wait until its last clause has been readable.
- **Readable, not flashed.** A line meant to be read stays settled for about
  0.3 s per word, and a label of one to three words for 0.8 s. Each of the four
  principles holds as the title of its whole scene. `npm run readability`
  samples every text draw at 30 fps and checks each must-read line (22 px and
  up) against that floor. Rolling counters and words in flight are reported as
  transient or motion, not as reads.
- **No muddy crossfades.** Where two busy layouts meet, the old one leaves
  before the new one arrives.
- **Sound in one piece.** Pitched effects sit in the score's key (D minor
  pentatonic). The one deliberate exception is the amber unknown, a clash until
  the answer resolves it. Ticks have rounded attacks and no bare broadband
  click, and repeated small sounds sit under the bed.
- **Clear to a stranger.** The end card carries the name, the promise, the four
  roles and where to get it.
- **A chosen thumbnail.** The settled payoff frame (€228.00, PROOF, VERIFIED)
  is the poster. It replaces frame 0 of the X cut and the web cut, because
  platforms take frame 0 as the idle image. `share-copy.txt` is the caption.

The film keeps its brief's 30 seconds and 15 bars, longer than the 15–25 s
/brag suggests. A few secondary labels therefore settle for somewhat less than
their floor; the audit lists them.

## Structure

```
src/timeline.mjs        cues, scene windows, motion-blur ranges, sound cues
src/film.mjs            frame compositor · two passes (main + emissive glow) · temporal supersampling
src/engine/             canvas primitives, easing/noise, a small perspective camera
src/hud.mjs             instrument frame: session act, timecode, stage rail
src/facts.mjs           everything factual, loaded from real sources and hash-verified
src/scenes/s1…s11       intent → observe → propose → plan → prove/ask → lower/check/meaning
                        → ready/consent → run → result+proof → reveal → title
audio/score.py          procedural score + sound design (reads the same timeline)
captured/               transcripts from the real binary, plan data, hashes
tools/                  static font cuts, glyph outlines, hash refresh, reading-time audit
share-copy.txt          the caption to post with the film
```

Glow is a property of individual elements, never a global filter. Every
element that emits light also draws into a quarter-resolution emissive
buffer, which is blurred at two radii and added back. Motion blur is real
temporal supersampling (up to 8 subframes, about a 130° shutter) on the fast
moves only.

## Render

Prerequisites: Node 22, Python 3 with `numpy scipy fonttools brotli blake3`, and
ffmpeg with libx264. The fonts are the repository's OFL cuts in
`../intent-to-impact/assets/fonts/`; `tools/build-fonts.py` instantiates static
weights from them.

```sh
cd scripts/media/motion/intent-to-proof
npm ci
npm run fonts                      # static cuts + glyph outlines → .cache/
npm run stills -- 2.3,12.8,25.5    # QA stills → .cache/stills/
npm run preview                    # 960×540 · 30 fps · with sound, ~1 min on 4 cores
npm run readability                # reading time of every must-read line (~15 s)
npm run master                     # 3840×2160 · 60 fps master + 1920×1080 · 60 fps X cut
npm run exports                    # web cut, poster, contact sheet, thumbnail, caption
```

The master renders about 5,900 motion-blur subframes on four worker processes
into lossless segments. It then encodes:

- `.cache/dist/intent-to-proof-4k60.mp4`: H.264 High, CRF 13.
- `.cache/dist/intent-to-proof-x-1080p60.mp4`: the X upload, capped at 24 Mb/s, with the poster as frame 0.

Both carry AAC audio normalized by ffmpeg `loudnorm` (target −14 LUFS,
−1.5 dBTP; the encoded files measure about −13.9 LUFS and −1.4 dBTP).
Before encoding, a final grade removes 8-bit banding from the dark
gradients: it debands in 16-bit precision, then dithers back to 8 bits
with an ordered pattern and a static luma grain.

The committed exports are the web cut `media/videos/intent-to-proof.mp4`
(1600×900 at 30 fps like the other films, under 8 MB), the README GIF
`media/gifs/intent-to-proof.optimized.gif` (a 12-second cut of five beats,
960 px at 12 fps, under the 8 MB GIF budget), the poster
`media/posters/intent-to-proof.png`, and the contact sheet
`media/storyboards/intent-to-proof.png`. Refresh them with
`npm run exports` after a master. The same step writes the upload thumbnail
`.cache/dist/intent-to-proof-poster.jpg` and `.cache/dist/share-copy.txt`.

If you change the fixture program or `captured/plan.json`, run
`python3 tools/hashes.py`. `src/facts.mjs` refuses to render with stale hashes.

## Feature clips

The same engine renders the short clips the ecosystem embeds: `nika-hero`
(the audit-then-run story, also copied to `media/nika-hero.gif`, the path
other repositories hotlink), `static-check-fix`, `permits-audit`,
`on-error-recover`, `dag-execution`, `chat-to-workflow`,
`editor-diagnostics`, `workflow-gallery`, `full-loop`, `pr-check-comment`,
`trace-proof`, `spec-anatomy`, `agent-plugin`, `first-session`,
`cost-ceiling` and `approval-gate`. Each is one file in
`clips/`, built from `clips/kit.mjs`: code cards that animate a real line
diff, terminals that stream captured lines, row highlights and a camera.

- Every program line, CLI line and diagnostic on screen is read from
  `scripts/media/fixtures/` or `media/raw/`, captured from the binary and
  its language server by `scripts/media/capture-transcripts.sh` (a clip
  whose story takes several commands owns a script in
  `scripts/media/capture/`, which that script runs). Where a
  clip's story rests on a capture (the escape the check must catch, the
  chain verify must read back), it checks it and refuses to render
  otherwise. What is
  illustration (the chat, the editor's chrome, the lighting of the DAG
  waves, the phrase-to-task mapping) says so on the clip's plate.
- The camera pushes in on what is being read, so reading moments land at
  22 to 33 px on the 1920 px frame, and pulls back for the last frame. That
  frame is the poster; for `static-check-fix` and `chat-to-workflow` it is
  also a social card.
- Outputs: `media/videos/<name>.mp4` (1600×900, 30 fps) and `.webm`,
  `media/gifs/<name>.optimized.gif` (960 px, 12 fps, 128 colours) and
  `media/posters/<name>.png`. A camera move repaints every pixel of a GIF
  frame, so moves are short and holds are long (a held frame costs
  almost nothing); every clip stays under the 8 MB budget.

```sh
npm run clip -- static-check-fix               # one clip, or `all`
npm run clip-stills -- permits-audit 4,9.5     # QA stills → .cache/clips/<name>/
npm run readability -- --clip dag-execution    # reading time, pushed-in text included
```

A change to shared code (`clips/kit.mjs`, `src/clip.mjs`, `src/engine/`)
changes the clips drawn with it: re-render them in the same change. Each
render records the clip file it was drawn from (its sha256, in
`media/clip-sources.json`), and `validate-media.sh` fails any clip whose
file has changed since.

## Verification

- `nika check scripts/media/fixtures/invoice-payments.nika` must stay clean. `scripts/media/validate-media.sh` enforces it.
- The fixture run is reproducible:

  ```sh
  cd scripts/media/fixtures
  nika run invoice-payments.nika --answer approval=false
  nika trace peek <trace> totals    # the totals the film shows
  ```

- The film MP4 must be exactly 30.000 s (`validate-media.sh`).
- Reading time: `npm run readability` (add `--strict` to fail on a miss).
- Sound was verified analytically, not by ear, in this build environment:
  - a spectrogram and short-term loudness curve read against the section and cue times;
  - octave-band balance against a pink reference;
  - integrated loudness and true peak through ffmpeg `ebur128`.
