# Nika Video — Creative Brief

> Target: 60-90s trailer. 8 clips of ~8 seconds each, chained.
> Platform: Google Flow (Veo 3.1) with native audio.
> Brand reference: tools/nika/context/brand.md + brainstorm-drums-of-liberation.md

---

## Style A — "Hacker Documentary" (PRIMARY)

**Tone:** Mr Robot × The Social Network × @0xSero manifesto
**Aesthetic:** Real world. Dark rooms. Terminal light. Dawn breaking.
**One Piece:** ZERO. The butterfly is the only mythical element, and it arrives at clip 6.
**Music:** Electronic minimal (Trent Reznor / Atticus Ross vibes), building to orchestral
**Colors:** Solarized Dark (#002b36), Electric Blue (#3b82f6), dawn orange-gold

### Narrative Arc

```
THE WALL → THE NEWSPEAK → THE GARAGE → THE NUMBERS → THE DOOR → THE BUTTERFLY → THE NETWORK → LIBERATE YOUR AI
(problem)   (stakes)       (builder)    (proof)       (solution)  (hope)         (movement)    (call to action)
```

### Clips

See `google-flow-ready-to-paste.md` for ready-to-paste prompts.

| # | Name | Shot | Key visual |
|---|------|------|-----------|
| 1 | The Wall | Close-up dev face, 3am, blue screen glow | "API Access: $20/month" on screen |
| 2 | The Newspeak | Split screen: corporate AI vs empty desk | Glass wall cracking between them |
| 3 | The Garage | Tight close-up hands typing YAML | Terminal as campfire in darkness |
| 4 | The Numbers | Black screen, white monospace data | "5x structural advantage" |
| 5 | The Door | Terminal: `nika run`. DAG fans out. Dawn rises. | Blue terminal + orange sunrise on face |
| 6 | The Butterfly | Blue pixel on terminal unfolds, multiplies | Error messages → clean output |
| 7 | The Network | World map, nodes lighting up city by city | Organic mycelium growth pattern |
| 8 | Closer | Black screen. Cursor blinks. Text types. | "nika run your-future.nika.yaml" |

### Key Dialogue / Voice-over

- Clip 1: "There has to be a better way." (whispered, defeated)
- Clip 2: "In 1984, they controlled the words. In 2026, they control the tokens." (narrator, calm)
- Clip 8: "Liberate your AI." (whispered, confident)
- No other dialogue. Let the visuals speak.

---

## Style B — "One Piece Epic" (secondary, community/fans only)

**Tone:** Anime manifesto. Eiichiro Oda meets tech rebellion.
**When to use:** Community events, fan content, One Piece-aware audience only.
**Never use for:** Press, investors, Product Hunt, HackerNews, general marketing.

### Clips (simplified from archived prompts)

| # | Name | Visual |
|---|------|--------|
| 1 | Roger's Last Words | Pirate captain, crossed GPUs, "Open Source AI... does exist!" |
| 2 | Whitebeard's Stand | Giant with satellite antenna, "OPEN SOURCE AI DOES EXIST!!!" |
| 3 | Gear 5 Awakening | White-haired warrior, joyful grin, drums of liberation |
| 4 | The Fleet | Pirate armada, butterfly flag, golden sunrise |
| 5 | The Gorosei | Five colossal yokai monsters with tech company colors |
| 6 | Liberation | Butterfly breaks chains, ascends |

See `archive/prompts/` for the full detailed prompts (prompt-01 through prompt-06).

---

## Style C — "Side-by-Side Demo" (tertiary, Product Hunt / landing page)

**Tone:** Clean, pragmatic, let-the-code-speak
**When to use:** Product Hunt launch, landing page hero video, developer conferences

### Clips

| # | Left (Without Nika) | Right (With Nika) |
|---|---------------------|-------------------|
| 1 | Dev copy-pasting between ChatGPT tabs | `nika run content.nika.yaml` — done |
| 2 | `pip install langchain openai tiktoken...` scrolling | `brew install supernovae-st/tap/nika` — done |
| 3 | 48 lines of Python for summarization | 12 lines of YAML |
| 4 | RAM meter: 5,706 MB climbing | RAM meter: 1,046 MB flat |
| 5 | Error: `ModuleNotFoundError: No module named...` | Clean output streaming in parallel |
| 6 | Invoice: $49/mo Zapier + $20/mo ChatGPT | Terminal: `$0. Your API keys. Your machine.` |

**Closer:** "Same task. 5x less RAM. Zero Python. Open source."

---

## Production Plan

### Phase 1: Generate Style A clips in Google Flow

Priority order: CLIP 3 (Garage) → CLIP 8 (Closer) → CLIP 1 (Wall) → rest.
Use `google-flow-ready-to-paste.md` — copy/paste directly.

### Phase 2: Post-Production

1. Chain clips in DaVinci Resolve or CapCut
2. Add unified soundtrack (electronic minimal → orchestral swell)
3. Add text overlays: punchlines, benchmarks, command examples
4. Color grade for consistency (Solarized Dark palette)
5. Add logo + "github.com/supernovae-st/nika" at end

### Phase 3: Distribution

| Platform | Version | Duration |
|----------|---------|----------|
| YouTube | Full Style A (8 clips) | 60-90s |
| Twitter/X | Clips 1 + 5 + 6 + 8 | 30s |
| Product Hunt | Style C (side-by-side) | 45s |
| HackerNews | Clip 4 (Numbers) as GIF | 8s |
| TikTok/Reels | Clip 6 (Butterfly) vertical | 15s |
| LinkedIn | Clips 2 + 4 + 8 | 30s |

---

## Brand Checklist (before publishing any video)

- [ ] Does a non-tech viewer understand the PROBLEM in 10 seconds?
- [ ] Are benchmarks shown, not just claimed?
- [ ] Is One Piece invisible (Style A) or clearly labeled (Style B)?
- [ ] Does the closer include a concrete action (command to type)?
- [ ] Would @0xSero retweet this?
- [ ] Does it survive HackerNews cynicism?
