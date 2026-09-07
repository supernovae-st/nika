# Nika: from intent to impact

A 60-second GSAP product film: an intention becomes a reviewable file,
a checked graph, a bounded run and five concrete deliverables.

The example uses fictional checkout data. It saves a brief, creates a GitHub
issue, notifies Telegram and Slack, and updates Linear, only after approval.
No model calls, customer accounts or live messages are involved. The YAML is
a focused excerpt, not a runnable workflow export or a screen capture of Nika.

## Preview and render

Use Node 22.12+, npm, Chrome and ffmpeg. From this directory:

```sh
npm ci
npm test
npm run build
npm run dev -- --port 4174 --strictPort
```

In another terminal, from the repository root:

```sh
npm ci --prefix scripts/media
node scripts/media/render-product-film.mjs http://127.0.0.1:4174 --qa
node scripts/media/render-product-film.mjs http://127.0.0.1:4174
```

The renderer captures 1,800 frames from the actual timeline. It exports the
60-second H.264 MP4, full-length optimized GIF, poster and contact sheet.
Controls are omitted. The export is silent; no music license is implied.

## Fonts and assets

Martian Grotesk, Martian Mono and Geist are included with their OFL notices.
Clash Display is proprietary freeware and is not redistributed here. For
the production typography, obtain Clash Display 600 directly from
[Fontshare](https://www.fontshare.com/fonts/clash-display) under its
[license](https://www.fontshare.com/licenses/itf-ffl), then place its WOFF2 at
`assets/fonts/clash-display-600.woff2`. That local file is Git-ignored.
Without it, browser fallback typography changes the composition.

Portraits are illustrative generated avatars. App marks identify the services
in the example; they do not imply endorsement.

## The one-minute edit

| Time | Beat |
|---|---|
| 0–11 s | Read the intention; words gain meaning, colors and app icons. |
| 11–16 s | The same card moves left; its fields populate the contract. |
| 16–19 s | Read indented YAML and bounded iteration. |
| 19–21.5 s | Review a change and preserve the file's history. |
| 21.5–25 s | Source folds into blocks; the same elements become the graph. |
| 25–28 s | Check the plan. Nothing has run yet. |
| 28–32 s | Retrieve context and calculate from captured CSV data. |
| 32–38 s | Three bounded branches, a retry, AI extraction and a join. |
| 38–42.5 s | Apply explicit rules; AI drafts a proposal, not a verdict. |
| 42.5–45.5 s | Wait for approval. |
| 45.5–48 s | Save, create, notify both channels and update Linear. |
| 48–57.5 s | Read the brief, issue, team updates and recorded evidence. |
| 57.5–60 s | Your intent. Your plan. Your result. |

`film-timing.js` maps the continuous 104-second source choreography onto
60 seconds. The review and code-folding sequence takes 7 seconds, rather
than 22. The result keeps 9.5 seconds. No source interval is skipped or reversed.

Space toggles playback, R replays, and chapters seek on the edited clock.
Reduced motion opens the result paused. `window.__film.seek(seconds)` is
the frame-accurate export seam shared with the browser player.

## Verification

Ten model/timing tests check topology, non-overlapping graph nodes, approval
before writes, notification dependencies, calculation, unknown preservation,
source indentation, concurrency parity and a continuous exactly-60-second edit.

Browser QA samples twelve beats, including YAML review, graph, checks and
outputs. The media validator checks duration and file budgets. These checks
validate the film, not a live integration or an AI answer.
