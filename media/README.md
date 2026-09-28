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

## Rules

- **No fake commands.** Every command shown in an asset exists in the CLI.
- **No fake output.** Terminal text is captured from the real binary
  (`scripts/media/capture-transcripts.sh` → `media/raw/*.txt`). The
  chat-to-workflow run is a real local inference (`ollama/llama3.2:3b`).
- **Every complete runnable workflow shown passes `nika check`** — except the deliberately
  broken fixture in the static-check-fix asset, whose failure is the point.
  `scripts/media/validate-media.sh` enforces both directions.
- **Illustrations say so.** The product film labels fictional data and folded
  source excerpts. It must never be described as a successful real integration.
- **Budgets** · README GIF ≤ 8 MB · poster PNG ≤ 1 MB.
- **Never edit exports by hand.** Edit the motion scene or fixture, then
  regenerate.

## Layout

```
media/
  brand/     nika-logomark.svg — official butterfly mark (geometry preserved)
  gifs/      *.optimized.gif   — README embeds (scene width/fps · ≤8MB)
  videos/    *.mp4 + *.webm    — docs + website embeds
  posters/   *.png             — static frame per animation (og:image, video poster)
  storyboards/*.png            — six-beat visual QA contact sheets
  social/    og-card + github-social-preview — share cards (scene: motion/og-card.html)
  raw/       *.txt + *.json    — captured CLI transcripts (the source of truth)
  nika-hero.gif                — the nika-hero clip at the path other repos hotlink
```

## Regenerate

```sh
bash scripts/media/capture-transcripts.sh     # refresh real CLI transcripts
cd scripts/media && npm install               # once (Playwright + GSAP + Geist)
node render-motion.mjs intent-dag-proof static-check-fix chat-to-workflow dag-execution
bash scripts/media/validate-media.sh          # honesty + budget gate
```

The motion scenes live in `scripts/media/motion/*.html` — deterministic
HTML/SVG timelines rendered frame-by-frame in headless Chrome. The README hero
has a checked-in production storyboard beside its scene. Open any scene in a
browser to preview it live.

Terminal captures (the second lane) are VHS tapes in `scripts/media/tapes/`,
rendered against the real installed binary in a staged workdir:

```sh
brew install vhs                              # once
bash scripts/media/render-tape.sh full-loop   # → media/gifs/full-loop.optimized.gif
bash scripts/media/validate-media.sh          # same honesty + budget gate
```

Their workflow fixtures live in `scripts/media/fixtures/` and are gated in
both directions like everything else (the broken half must keep failing
`nika check`, the fixed half must stay clean).

## Embedding

GitHub README → use the optimized GIF:

```md
![alt text](media/gifs/<scene>.optimized.gif)
```

Docs (Mintlify) and the website → prefer video with the poster:

```html
<video autoPlay muted loop playsInline poster="/images/posters/<scene>.png">
  <source src="/videos/<scene>.webm" type="video/webm" />
  <source src="/videos/<scene>.mp4" type="video/mp4" />
</video>
```

Social / OG cards → the posters are 1600×900 stills designed to work alone.
