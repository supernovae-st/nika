# Scheduling UX Bible — Every Interaction, Every Detail

> The user should NEVER feel lost. Every screen has a next step.
> Every error has a fix. Every success has a celebration.

---

## Principle: The 3-Second Rule

If a user can't figure out what to do in 3 seconds, we failed.
Every output must have: (1) what happened, (2) what to do next.

---

## 1. `nika every` — The Magic Moment

### 1.1 Zero-arg wizard — full hand-holding

```
$ nika every

  ┌  nika every · Schedule a recurring workflow
  │
  ◆  Which workflow?
  │  ⌕ █
  │
  │  ┌──────────────────────────────────────────────────────────┐
  │  │  report.nika.yaml            "Generate daily summary"    │
  │  │  health-check.nika.yaml      "API uptime monitor"        │
  │  │  translate.nika.yaml         "Translate content"          │
  │  │  competitor-watch.nika.yaml  "Track competitor pricing"   │
  │  └──────────────────────────────────────────────────────────┘
  │
  │  ↑↓ navigate  ⏎ select  type to search
  │  💡 Showing 4 workflows from ./  (12 total, type to filter)
```

**Wow details:**
- Shows workflow `description:` from YAML next to filename (dim white)
- Bottom hint shows total count + filter tip
- Fuzzy search: typing "rep" highlights `report.nika.yaml`
- If only 1 workflow exists → auto-select, skip this step

### 1.2 Frequency picker — natural language first

```
  ◇  Which workflow?
  │  report.nika.yaml
  │
  ◆  How often should it run?
  │
  │  ┌──────────────────────────────────────────────────────────┐
  │  │                                                          │
  │  │  ● Every few hours          "every 2h", "every 6h"      │
  │  │    Every day                 "daily at 9am"              │
  │  │    Every weekday             "Mon–Fri at 9am"            │
  │  │    Every week                "Mondays at 9am"            │
  │  │    Every month               "1st of month at midnight"  │
  │  │    Type it yourself          cron or natural language     │
  │  │                                                          │
  │  └──────────────────────────────────────────────────────────┘
  │
  │  💡 Most users choose "Every day" — it's 80% of schedules
```

After selecting "Every few hours":

```
  ◆  How many hours between each run?
  │
  │  ┌──────────────────────────────────────────────────────────┐
  │  │    Every 1 hour        (24 runs/day · ~$0.72/day)        │
  │  │  ● Every 2 hours       (12 runs/day · ~$0.36/day)        │
  │  │    Every 4 hours       ( 6 runs/day · ~$0.18/day)        │
  │  │    Every 6 hours       ( 4 runs/day · ~$0.12/day)        │
  │  │    Every 8 hours       ( 3 runs/day · ~$0.09/day)        │
  │  │    Every 12 hours      ( 2 runs/day · ~$0.06/day)        │
  │  │    Custom...            type any number                   │
  │  └──────────────────────────────────────────────────────────┘
  │
  │  💡 Cost based on last run of report.nika.yaml ($0.03)
```

**Wow details:**
- **Cost preview on EVERY option** — users see the financial impact before choosing
- Cost calculated from workflow's last execution (if available)
- If no history: "cost: first run will calibrate"
- Runs/day helps users grasp frequency intuitively

### 1.3 "Every day" → time picker with smart default

```
  ◆  What time should it run?
  │
  │  09:00 █
  │  ──────
  │  ✓ "Every day at 09:00" → cron: 0 9 * * *
  │
  │  💡 Default: 09:00 (most teams start at 9).
  │     Format: HH:MM (24h). Examples: 09:00, 14:30, 00:00
```

**Wow details:**
- **Live cron preview** as user types — updates on every keystroke
- Default is 09:00 (not midnight — because that's what humans actually want)
- Shows the human-readable AND cron translation simultaneously
- Invalid input: gentle inline error, not a rejection

```
  │  25:00 █
  │  ──────
  │  ✗ Invalid hour — must be 00–23
  │    Did you mean 15:00?
```

### 1.4 Timezone — auto-detect with one-tap confirm

```
  ◆  Timezone
  │
  │  ● Europe/Paris (UTC+2) — detected from your system
  │    UTC
  │    Other...
  │
  │  💡 Your system timezone is Europe/Paris. Press ⏎ to confirm.
```

**Wow details:**
- **Auto-detects system timezone** — 90% of users just press Enter
- Shows UTC offset for clarity
- "Other..." opens fuzzy search across IANA names
- If system tz detection fails → default to UTC with explanation

### 1.5 The Preview — where the wow happens

```
  ◆  Preview
  │
  │  ╭──────────────────────────────────────────────────────────────╮
  │  │                                                              │
  │  │   📋  report.nika.yaml                                       │
  │  │   🔄  Every 2 hours                                          │
  │  │   🕐  cron: 0 */2 * * *                                      │
  │  │   🌍  Europe/Paris (CEST, UTC+2)                              │
  │  │                                                              │
  │  ├──────────────────────────────────────────────────────────────┤
  │  │                                                              │
  │  │   Next 5 runs                                                │
  │  │                                                              │
  │  │   1.  Today      19:00    in 47m                             │
  │  │   2.  Today      21:00    in 2h 47m                          │
  │  │   3.  Today      23:00    in 4h 47m                          │
  │  │   4.  Tomorrow   01:00    in 6h 47m                          │
  │  │   5.  Tomorrow   03:00    in 8h 47m                          │
  │  │                                                              │
  │  ├──────────────────────────────────────────────────────────────┤
  │  │                                                              │
  │  │   💰 Cost estimate (based on last run: $0.031)               │
  │  │                                                              │
  │  │   Per run      $0.031    ~2,400 tokens in · ~800 out         │
  │  │   Per day      $0.37     12 runs                             │
  │  │   Per month    $11.16    ~360 runs                           │
  │  │                                                              │
  │  │   ⚠ $11/month — want to reduce? Try "every 6h" ($3.60/mo)   │
  │  │                                                              │
  │  ╰──────────────────────────────────────────────────────────────╯
  │
  ◆  Create this schedule? (Y/n)
```

**Wow details:**
- **Cost warning** when monthly estimate > $10 — suggests cheaper alternatives
- Relative timestamps: "in 47m", "in 2h 47m" — instantly graspable
- Token breakdown helps power users optimize prompts
- "Based on last run" gives confidence the estimate is real

### 1.6 The Celebration — cascading success

After pressing Y, the animation plays step by step with ~200ms delay between each:

```
  ◇  Create this schedule?
  │  Yes
  │
  │  ⠋ Validating cron expression...
```
```
  │  ✓ Cron valid: 0 */2 * * *
  │  ⠋ Registering with daemon...
```
```
  │  ✓ Cron valid: 0 */2 * * *
  │  ✓ Registered: report-2h
  │  ⠋ Computing next runs...
```
```
  │  ✓ Cron valid: 0 */2 * * *
  │  ✓ Registered: report-2h
  │  ✓ Next run: today 19:00 (in 47m)
  │
  ╭──────────────────────────────────────────────────────────────╮
  │                                                              │
  │  ✓  report-2h is live!                                       │
  │                                                              │
  │  View:     nika schedule show report-2h                      │
  │  Pause:    nika schedule pause report-2h                     │
  │  Run now:  nika schedule trigger report-2h                   │
  │  All:      nika schedule list                                │
  │                                                              │
  ╰──────────────────────────────────────────────────────────────╯
  │
  ◆  Run it now to test? (Y/n)
```

**Wow details:**
- **3-step cascade** with spinners → checkmarks. Each step has ~200ms delay. Creates momentum.
- **"is live!"** — not "created" or "saved". Emotional language.
- **4 contextual commands** — user always knows what to do next
- **"Run it now to test?"** — proactive offer, not a dead end

### 1.7 First run — live task view

If user says Yes to "Run now?":

```
  │  Running report.nika.yaml...
  │
  │  ┌──────────────────────────────────────────────────────────┐
  │  │  1/3  research        ⠹ Fetching 5 articles...    2.1s  │
  │  └──────────────────────────────────────────────────────────┘
```

Tasks appear one by one as they complete:

```
  │  ┌──────────────────────────────────────────────────────────┐
  │  │  1/3  research        ✓ 5 articles fetched         2.1s  │
  │  │  2/3  summarize       ⠹ Generating summary...      0.8s  │
  │  └──────────────────────────────────────────────────────────┘
```

```
  │  ┌──────────────────────────────────────────────────────────┐
  │  │  1/3  research        ✓ 5 articles fetched         2.1s  │
  │  │  2/3  summarize       ✓ 847 words generated        3.4s  │
  │  │  3/3  format          ✓ output/report.md written    0.2s  │
  │  └──────────────────────────────────────────────────────────┘
  │
  │  ✓ Completed in 5.7s · $0.031 · output/report.md
  │
  └  Next run: today 19:00 (in 46m). Have a great day! 🦋
```

**Wow details:**
- **Live progress** per task with spinner animation
- Task output summary: "5 articles fetched", "847 words" — not just "done"
- Final line: total time + cost + output path
- **Butterfly emoji** 🦋 — Nika's identity, only on success celebrations
- "Have a great day!" — emotional closure (varies: "See you in 2h!", "Happy hacking!")

---

## 2. Smart Helpers — Never Let the User Fail

### 2.1 Did-you-mean for EVERYTHING

**Misspelled subcommand:**
```
$ nika shedule list

  ✗ Unknown command: shedule

  Did you mean?
    → nika schedule list

  See all commands: nika help
```

**Misspelled schedule name:**
```
$ nika schedule show daily-repost

  ✗ Schedule "daily-repost" not found

  Did you mean?
    → daily-report     (active, every day at 09:00)

  All schedules: nika schedule list
```

**Almost-valid cron:**
```
$ nika every "0 9 * *" report.nika.yaml

  ✗ Invalid cron: 4 fields (expected 5)

    0 9 * *
    ▔▔▔▔▔▔▔
    minute hour day month ← missing weekday

  Did you mean?
    → 0 9 * * *      Every day at 09:00
    → 0 9 * * 1-5    Weekdays at 09:00

  Or use natural language:
    → nika every "day at 9am" report.nika.yaml
```

**Wrong cron field value:**
```
$ nika every --cron "0 25 * * *" report.nika.yaml

  ✗ Invalid cron: hour must be 0–23, got 25

    0 25 * * *
      ▔▔
      ↑ hour field

  Did you mean?
    → 0 15 * * *     Daily at 15:00 (3 PM)
    → 0 5 * * *      Daily at 05:00 (5 AM)

  Cheat sheet: nika help cron
```

### 2.2 Proactive warnings

**Expensive schedule:**
```
$ nika every 5m expensive-pipeline.nika.yaml

  ⚠ Heads up — this will run 288 times per day

  Last run cost:     $0.45
  Estimated daily:   $129.60
  Estimated monthly: $3,888.00

  Are you sure? Consider:
    → every 30m    $21.60/day    (6x cheaper)
    → every 1h     $10.80/day    (12x cheaper)
    → every 6h     $1.80/day     (72x cheaper)

  Continue anyway? (y/N)   ← Default is NO
```

**Schedule overlap warning:**
```
$ nika every "day at 9am" report.nika.yaml

  ⚠ Time conflict detected

  3 workflows already run between 08:30–09:30:
    ● data-sync         09:00
    ● metrics-collect    09:00
    ● your new schedule  09:00

  This may cause API rate limiting. Consider:
    → nika every "day at 9:15am" report.nika.yaml    (stagger by 15m)
    → nika every "day at 10am" report.nika.yaml      (different hour)

  Continue at 09:00 anyway? (Y/n)
```

**Daemon not running:**
```
$ nika every 6h report.nika.yaml

  ✓ Schedule saved: report-6h

  ⚠ The daemon is not running — your schedule won't fire until you start it.

  Start the daemon:
    nika daemon start              Run in background
    nika daemon start --foreground  Stay in terminal

  Check daemon:
    nika daemon status

  Your schedule is saved and will start firing once the daemon is up.
```

### 2.3 Contextual hints in every output

Every `nika schedule list` shows a hint line at the bottom:

```
  💡 nika schedule show <name> for details · nika every to add new
```

Empty dashboard:

```
  No schedules yet — your workflows run on-demand.

  Get started in 10 seconds:
    nika every 6h report.nika.yaml

  Or explore interactively:
    nika every
```

One failing schedule:

```
  💡 1 schedule failing · nika schedule show deploy-staging for details
```

All healthy:

```
  💡 All 4 schedules healthy · next: data-sync in 12m
```

---

## 3. `nika schedule show` — The Information Palace

### 3.1 Active schedule — everything at a glance

```
$ nika schedule show daily-report

  ╭─ daily-report ────────────────────────────────────────────────────╮
  │                                                                    │
  │  📋 Workflow     report.nika.yaml                                   │
  │  🔄 Schedule     Every day at 09:00                                 │
  │  🕐 Cron         0 9 * * *                                          │
  │  🌍 Timezone     Europe/Paris (CEST, UTC+2)                         │
  │  📅 Created      8 days ago (Mar 28)                                │
  │  ●  Status       Active — running smoothly                          │
  │                                                                    │
  ├─ Where are we? ────────────────────────────────────────────────────┤
  │                                                                    │
  │  Last run      Today 09:03  ✓  3.2s  $0.031                       │
  │  Next run      Tomorrow 09:00  (in 14h 23m)                       │
  │                                                                    │
  │  ▐██████████████████████████████████████░░░░░░░░░░░░░░░░▌  60%     │
  │  09:00                      now                    09:00            │
  │  ◆──────────── 9h 37m ──────────┤── 14h 23m ──────○                │
  │  last run                       │               next run            │
  │                                                                    │
  ├─ Upcoming ─────────────────────┬─ Recent ──────────────────────────┤
  │                                │                                    │
  │  Tomorrow  09:00  in 14h       │  Today     09:03  ✓  3.2s  $0.031 │
  │  Mon 07    09:00  in 1d 14h    │  Yesterday 09:01  ✓  3.5s  $0.033 │
  │  Tue 08    09:00  in 2d 14h    │  Thu 03    09:02  ✗  1.1s  $0.008 │
  │  Wed 09    09:00  in 3d 14h    │  Wed 02    09:00  ✓  3.1s  $0.029 │
  │  Thu 10    09:00  in 4d 14h    │  Tue 01    09:01  ✓  2.9s  $0.028 │
  │                                │                                    │
  ├─ Health (30 days) ─────────────┴───────────────────────────────────┤
  │                                                                    │
  │  ✓✓✓✓✓✓✓✓✓✗✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓✓                                 │
  │  ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔                                  │
  │  Mar 6                              Apr 5                          │
  │                                                                    │
  │  Success     ████████████████████████████░  97% (29/30)            │
  │  Duration    avg 3.1s · p95 4.8s                                   │
  │  Cost        $0.031/run · $0.93/month                              │
  │                                                                    │
  ╰────────────────────────────────────────────────────────────────────╯

  Actions:
    nika schedule trigger daily-report      Run now (won't affect schedule)
    nika schedule pause daily-report        Pause future runs
    nika schedule remove daily-report       Delete this schedule

  History:
    nika trace list --schedule daily-report   Full execution traces
```

### 3.2 Failing schedule — diagnosis mode

```
$ nika schedule show deploy-staging

  ╭─ deploy-staging ──────────────────────────────────────────────────╮
  │                                                                    │
  │  📋 Workflow     deploy.nika.yaml                                   │
  │  🔄 Schedule     Every day at 14:00                                 │
  │  ✗  Status       FAILING — 3 consecutive failures                   │
  │                                                                    │
  ├─ What's wrong? ────────────────────────────────────────────────────┤
  │                                                                    │
  │  Last 3 runs all failed with the same error:                       │
  │                                                                    │
  │  NIKA-045 · Fetch timeout                                          │
  │  Task "deploy" → fetch https://api.staging.example.com/deploy      │
  │  Timeout after 30s — server not responding                         │
  │                                                                    │
  │  ┌─ Suggested fixes ─────────────────────────────────────────────┐ │
  │  │                                                               │ │
  │  │  1. Check if staging server is up:                            │ │
  │  │     curl -v https://api.staging.example.com/deploy            │ │
  │  │                                                               │ │
  │  │  2. Increase timeout in your workflow:                        │ │
  │  │     fetch: { url: "...", timeout: 60 }  (currently: 30)       │ │
  │  │                                                               │ │
  │  │  3. Add on_error: to handle gracefully:                       │ │
  │  │     on_error: { ignore: true }                                │ │
  │  │                                                               │ │
  │  │  4. Test manually:                                            │ │
  │  │     nika run deploy.nika.yaml --verbose                       │ │
  │  │                                                               │ │
  │  └───────────────────────────────────────────────────────────────┘ │
  │                                                                    │
  │  ⚠ Auto-pause in 2 more failures (threshold: 5)                    │
  │                                                                    │
  ├─ Recent ───────────────────────────────────────────────────────────┤
  │                                                                    │
  │  Today     14:01  ✗  NIKA-045  timeout 30s    $0.001               │
  │  Yesterday 14:02  ✗  NIKA-045  timeout 30s    $0.001               │
  │  Wed 03    14:00  ✗  NIKA-045  timeout 30s    $0.001               │
  │  Tue 02    14:01  ✓  completed  12.4s         $0.089               │
  │  Mon 01    14:00  ✓  completed  11.8s         $0.085               │
  │                                                                    │
  ╰────────────────────────────────────────────────────────────────────╯

  Quick actions:
    nika schedule trigger deploy-staging     Retry now
    nika schedule pause deploy-staging       Stop failing runs
    nika trace show <last-run-id>            See full error trace
```

**Wow details:**
- **"What's wrong?" section** — analyses the error pattern (3 same errors → likely infrastructure)
- **Numbered fix suggestions** — from most likely to least
- **Auto-pause countdown** — user knows when it'll auto-stop
- Error messages reference ACTUAL workflow content (`timeout: 30`)

---

## 4. Animations Reference

### Spinner styles (Braille, cycles at 80ms)

```
⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏
```

### Step cascade timing

```
Step 1: ⠋ Validating...     (200ms min display)
Step 1: ✓ Valid              (hold 150ms)
Step 2: ⠋ Registering...    (200ms min display)
Step 2: ✓ Registered         (hold 150ms)
Step 3: ⠋ Computing...      (200ms min display)
Step 3: ✓ Done               (hold 150ms)
Final card: appears with 50ms fade-in per line
```

### Progress bar fill (left to right, 50ms per segment)

```
▐░░░░░░░░░░░░░░░░░░░░▌  0%
▐████░░░░░░░░░░░░░░░░▌  20%
▐████████░░░░░░░░░░░░▌  40%
▐████████████░░░░░░░░▌  60%
▐████████████████░░░░▌  80%
▐████████████████████▌  100%
```

### History dots (appear one by one, 30ms per dot)

```
✓         (30ms)
✓✓        (30ms)
✓✓✓       (30ms)
✓✓✓✗      (30ms — red flash)
✓✓✓✗✓     (30ms)
```

---

## 5. Closing Lines — Emotional Micro-copy

Success varies by context:

| Context | Closing line |
|---------|-------------|
| First schedule ever | "Your first schedule! Welcome to automation. 🦋" |
| High-frequency (< 1h) | "That's a busy schedule! Running every {N}m." |
| Daily schedule | "See you tomorrow at {time}! 🦋" |
| Weekly schedule | "See you next {day}! 🦋" |
| After "run now" test | "Looks good! Next automatic run: {time}." |
| Pause | "Paused. Resume anytime: nika schedule resume {name}" |
| Resume | "Back in action! Next run: {time}" |
| Remove | "Schedule removed. {N} historical runs preserved in traces." |

---

## 6. Inline Help — `nika help cron`

```
$ nika help cron

  ╭─ Cron Cheat Sheet ─────────────────────────────────────────────╮
  │                                                                 │
  │  ┌─────┬──────┬──────┬───────┬─────────┐                       │
  │  │ min │ hour │ day  │ month │ weekday │                       │
  │  │0-59 │ 0-23 │ 1-31 │ 1-12  │  0-6    │                       │
  │  └─────┴──────┴──────┴───────┴─────────┘                       │
  │                                                                 │
  │  Symbols:                                                       │
  │    *     any value           */N   every N units                │
  │    1,3,5 specific values     1-5   range                        │
  │                                                                 │
  │  Common patterns:                                               │
  │    0 9 * * *       Daily at 09:00                               │
  │    0 */6 * * *     Every 6 hours                                │
  │    0 9 * * 1-5     Weekdays at 09:00                            │
  │    */15 * * * *    Every 15 minutes                             │
  │    0 0 1 * *       First of every month                         │
  │                                                                 │
  │  Presets:                                                       │
  │    @hourly @daily @weekly @monthly @yearly                      │
  │                                                                 │
  │  Or just use natural language:                                  │
  │    nika every "day at 9am" workflow.nika.yaml                   │
  │    nika every "weekday at 9am" workflow.nika.yaml               │
  │                                                                 │
  │  Try it: https://crontab.guru                                   │
  │                                                                 │
  ╰─────────────────────────────────────────────────────────────────╯
```

