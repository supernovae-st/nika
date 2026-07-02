# Nika media

Official visual assets for the README, docs, website and social surfaces.

## Rules

- **No fake commands.** Every command shown in an asset exists in the CLI.
- **No fake output.** Terminal text is captured from the real binary
  (`scripts/media/capture-transcripts.sh` → `media/raw/*.txt`). The
  chat-to-workflow run is a real local inference (`ollama/llama3.2:3b`).
- **Every workflow shown passes `nika check`** — except the deliberately
  broken fixture in the static-check-fix asset, whose failure is the point.
  `scripts/media/validate-media.sh` enforces both directions.
- **Budgets** · README GIF ≤ 8 MB · poster PNG ≤ 1 MB.
- **Never edit exports by hand.** Edit the motion scene or fixture, then
  regenerate.

## Layout

```
media/
  gifs/      *.optimized.gif   — README embeds (1280px · 12fps · ≤8MB)
  videos/    *.mp4 + *.webm    — docs + website embeds
  posters/   *.png             — static frame per animation (og:image, video poster)
  raw/       *.txt + *.json    — captured CLI transcripts (the source of truth)
  nika-hero.gif                — real terminal capture (check + run)
```

## Regenerate

```sh
bash scripts/media/capture-transcripts.sh     # refresh real CLI transcripts
cd scripts/media && npm install               # once (playwright-core)
node render-motion.mjs static-check-fix chat-to-workflow dag-execution
bash scripts/media/validate-media.sh          # honesty + budget gate
```

The motion scenes live in `scripts/media/motion/*.html` — self-contained
HTML/SVG timelines rendered frame-by-frame in headless Chrome. Open any
scene in a browser to preview it live.
