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
tools/                  static font cuts, glyph outlines, hash refresh
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
npm run master                     # 3840×2160 · 60 fps master + 1920×1080 · 60 fps X cut
```

The master renders about 5,900 motion-blur subframes on four worker processes
into lossless segments. It then encodes:

- `.cache/dist/intent-to-proof-4k60.mp4`: H.264 High, CRF 13.
- `.cache/dist/intent-to-proof-x-1080p60.mp4`: the X upload, capped at 24 Mb/s.

Both carry AAC audio normalized by ffmpeg `loudnorm` (target −14 LUFS,
−1.5 dBTP; the encoded files measure about −13.8 LUFS and −1.4 dBTP).
Before encoding, a final grade removes 8-bit banding from the dark
gradients: it debands in 16-bit precision, then dithers back to 8 bits
with an ordered pattern and a static luma grain.

The committed exports are the web cut `media/videos/intent-to-proof.mp4`
(1600×900 at 30 fps like the other films, under 8 MB), the poster
`media/posters/intent-to-proof.png`, and the contact sheet
`media/storyboards/intent-to-proof.png`. Refresh them with
`npm run exports` after a master.

If you change the fixture program or `captured/plan.json`, run
`python3 tools/hashes.py`. `src/facts.mjs` refuses to render with stale hashes.

## Verification

- `nika check scripts/media/fixtures/invoice-payments.nika` must stay clean. `scripts/media/validate-media.sh` enforces it.
- The fixture run is reproducible:

  ```sh
  cd scripts/media/fixtures
  nika run invoice-payments.nika --answer approval=false
  nika trace peek <trace> totals    # the totals the film shows
  ```

- The film MP4 must be exactly 30.000 s (`validate-media.sh`).
- Sound was verified analytically, not by ear, in this build environment:
  - a spectrogram and short-term loudness curve read against the section and cue times;
  - octave-band balance against a pink reference;
  - integrated loudness and true peak through ffmpeg `ebur128`.
