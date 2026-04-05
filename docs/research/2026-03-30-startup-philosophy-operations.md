# Research Report: Startup Philosophy & Operations for a 2-Person AI Studio

> Date: 2026-03-30 | Context: SuperNovae Studio (Paris), building Nika (open source AI workflow engine)
> Sources: 25+ pages analyzed across Pixar, Apple, Linear, YC, Basecamp, GitHub, Telegram ecosystems

---

## Table of Contents

1. [Pixar Braintrust -- Creative Feedback Without Authority](#1-pixar-braintrust)
2. [Apple Small Teams -- DRI and Saying No](#2-apple-small-teams)
3. [Linear Method -- Opinionated Software Principles](#3-linear-method)
4. [YC Advice -- Solo/Duo Founder Playbook](#4-yc-advice)
5. [Shape Up -- 6-Week Cycles for Small Teams](#5-shape-up)
6. [GitHub + Linear Integration -- Automation Playbook](#6-github-linear)
7. [Telegram Bots -- CI/Deploy/Release Notifications](#7-telegram-bots)
8. [Synthesis: The SuperNovae Playbook](#8-synthesis)

---

## 1. Pixar Braintrust

### The Core Idea

Every Pixar movie starts out bad. Ed Catmull's central thesis in *Creativity, Inc.* is that **"early on, all of our movies suck"** and the real work is "going from suck to not-suck." This is not a failure -- it is the process. The Braintrust is the mechanism that makes this transformation possible.

### The Ugly Baby Principle

Catmull defines early ideas as **"ugly babies" -- awkward and unformed, vulnerable and incomplete."** They are not miniature versions of the final product. They can be killed by "a barrage of well-meaning questions," "snarky comments," or simply being "ignored and lost." The rule: **protect the new.** Not everyone can see what ugly babies can grow into.

### Braintrust Rules

| Rule | Why It Matters |
|------|---------------|
| **No authority** -- the Braintrust advises but cannot dictate | Directors know notes will not undermine them, so they listen without defensiveness |
| **Candor is mandatory** -- feedback is honest, constructive, impersonal | Focus on story/product issues, not people |
| **Problem-focused, not solution-prescribing** | Explore what works and what does not; do not hand down answers |
| **Fluid membership** -- core group rotates based on relevance | Fresh eyes, no stale groupthink |
| **Trust built from shared passion and history** | Peers like Lasseter, Stanton, Docter, Unkrich, Ranft |

Catmull's quote: *"It is not the manager's job to prevent risks. It is the manager's job to make it safe to take them."*

### Applicable to a 2-Person Team

- **Schedule "Braintrust" sessions with each other every 2 weeks.** Show ugly babies. The rule: identify problems, do not prescribe solutions. The person who built the thing retains creative control.
- **Invite 1-2 external advisors quarterly** (trusted peers, not investors) for the same no-authority feedback.
- **Run postmortems after every release.** Not blame sessions. "What surprised us? What did we learn?"
- **Accept that v0.x will suck.** The goal is iterating from suck to not-suck, not shipping perfection.

### Sources

- Ed Catmull, *Creativity, Inc.* (2014)
- https://hbr.org/2008/09/how-pixar-fosters-collective-creativity
- https://review.firstround.com/spark-creativity-with-these-tips-from-pixars-president/
- https://makingsmallercircles.com/articles/the-creative-meeting-applying-lessons-from-pixar-brain-trust-to-improve-how-we-solve-problem/

---

## 2. Apple Small Teams

### The DRI (Directly Responsible Individual)

Apple coined this term. Every project, every meeting action item, every decision has **one person's name next to it**. Not a team. Not "shared ownership." One name. The question at Apple is always: **"Who's the DRI on that?"**

This eliminates confusion about accountability. The DRI has final decision-making authority, consults stakeholders for context, but pushes forward independently without needing consensus.

### Steve Jobs on Focus

The definitive quote:

> "People think focus means saying yes to the thing you've got to focus on. But that's not what it means at all. It means saying no to the hundred other good ideas that there are. You have to pick carefully. I'm actually as proud of the things we haven't done as the things I have done. Innovation is saying no to 1,000 things."

Jobs was notorious for ejecting people from meetings if they were not core to the discussion. He killed products (OpenDoc, Newton), he killed features, he killed entire divisions -- not because they were bad, but because they diluted focus.

### Apple's Functional Organization

Apple has **no general managers, no divisions, no separate P&L centers.** The entire company operates as one unit organized by function (design, engineering, marketing), not by product line. This means designers work across products. Engineers work across products. There is one company-wide profit and loss. This structure forces focus because there is nowhere to hide.

### Small Team Sizes

Research consistently shows teams of **3-7 people** have the highest productivity and innovation rates. Jobs kept core product teams deliberately small -- small enough that everyone knows what everyone else is doing, large enough to ship.

### Applicable to a 2-Person Team

- **Assign DRI for every area.** Even with 2 people, make it explicit: who owns runtime? Who owns TUI? Who owns marketing? One name per domain. No "we both do everything."
- **Say no by default.** Maintain a "Not Doing" list alongside your roadmap. Every feature request gets evaluated against: "Does this serve the core use case?" If not, it goes on the Not Doing list.
- **Kill features that dilute.** If something is 80% done but does not serve the core vision, kill it. Jobs killed things that were further along.
- **Run meetings with exactly the people needed.** For 2 people, this means: do not invite your advisor to the daily standup. Do not CC people on decisions they do not own.

### Sources

- https://handbook.gitlab.com/handbook/people-group/directly-responsible-individuals/
- https://www.bitesizelearning.co.uk/resources/directly-responsible-individual-dri-apple
- WWDC 1997 (Jobs on focus and saying no)

---

## 3. Linear Method

### Philosophy: Opinionated Software

Linear is **purpose-built for one use case** (issue tracking for individual contributors at growing companies) and deliberately refuses to be flexible. Karri Saarinen (CEO) describes their approach as forming "atom-level opinions" on everything -- from what an issue is to how sprints should work.

They optimize for **ICs (individual contributors), not managers.** This is a conscious choice that means saying no to features that serve reporting/oversight but add friction to the people doing the work.

### Core Principles from linear.app/method

| Principle | What It Means |
|-----------|--------------|
| **Set the product direction** | Align on vision before execution. Direction is not a backlog. |
| **Set useful goals** | Goals should be meaningful outcomes, not task counts |
| **Prioritize enablers and blockers** | Work on what unlocks other work first |
| **Scope projects down** | Smaller scope = higher quality = faster shipping |
| **Generate momentum** | Find a cadence and routine. Momentum, not sprinting. "The goal is to maintain a healthy momentum, not to rush towards the end." |
| **Write issues, not user stories** | Concrete tasks, not "As a user, I want..." |
| **Build with users** | Not for users, not at users -- with them |
| **Launch and keep launching** | Ship continuously, not in big bangs |

### On Speed and Quality

Linear's position: speed and quality are not tradeoffs. Speed comes from **removing friction** (keyboard shortcuts, instant UI, no unnecessary meetings), not from cutting corners. Quality comes from **craftsmanship** -- interactions, animations, design coherence.

They distinguish **product debt** (deliberate scope narrowing, e.g., minimal settings page) from **tech debt** (bad code). Product debt is strategic. Tech debt is not.

### On Estimation

Linear discourages traditional estimation. Break work into small enough issues that you can complete several per week. If an issue takes more than a few days, it is too big -- scope it down.

### Applicable to a 2-Person Team

- **Write issues, not stories.** "Fix DAG cycle detection for nested includes" is better than "As a workflow author, I want nested includes to work."
- **Scope every project down ruthlessly.** If you think it takes 2 weeks, find the version that takes 3 days.
- **Optimize for momentum over sprints.** Do not create artificial 2-week sprint boundaries. Ship continuously. The rhythm is the release cadence, not the sprint boundary.
- **Build opinionated software.** Do not add configuration options to avoid making a decision. Make the decision. If it is wrong, change it later.
- **Keyboard shortcuts for everything.** This is a craft decision that compounds daily.

### Sources

- https://linear.app/method
- https://www.figma.com/blog/the-linear-method-opinionated-software/
- https://newsletter.pragmaticengineer.com/p/linear

---

## 4. YC Advice for Duo Founders

### "Do Things That Don't Scale" (Paul Graham, 2013)

The most important essay for early-stage founders. Core argument: **the things that make a startup grow in the beginning are not the things that make it grow later.** You should actively do unscalable things.

**Specific techniques:**
- **Manually recruit users one by one.** Not with landing pages or ads. Personally reach out to people who have the problem you solve.
- **Be a consultant for your first 10 users.** Build features specifically for them. This is not wasteful -- it teaches you what the product needs to be.
- **Assemble by hand.** Meraki (YC W06) hand-assembled their routers. You should hand-craft workflows for early Nika users.

### "Make Something People Want"

YC's entire philosophy in one sentence. Not something clever. Not something technically impressive. Something that solves a real problem for real people who will pay (with money or attention) to have it solved.

### Michael Seibel's Tactical Advice

- **Ship imperfect products quickly.** "Time and time again I see founders fail when they don't launch soon enough."
- **Track one metric.** For early stage: weekly active users. Not monthly. Not downloads. Who is actually using it this week?
- **Aim for 10% week-over-week growth** in your core metric. This is aggressive but achievable with manual effort.
- **A bad co-founder is worse than no co-founder.** For an existing duo: ensure complementary skills and even equity.

### Role Split for a 2-Person Technical Startup

| Role | Person A (Builder) | Person B (Builder + Distribution) |
|------|-------------------|----------------------------------|
| Core | Architecture, runtime, engine | CLI, UX, integrations |
| Distribution | Open source community, docs | Outreach, content, partnerships |
| Users | Build features based on feedback | Find the next 10 users manually |

Note: In a 2-technical-founder startup, **both people build.** The split is which one also owns distribution.

### Applicable to a 2-Person Team

- **Launch something every week.** A release, a blog post, a showcase workflow, a tutorial. Continuous presence.
- **Find 10 users who love Nika, not 1000 who kind of use it.** Talk to them weekly. Build for them.
- **Do things that do not scale.** Write custom workflows for early users. Help them debug. Join their Slack/Discord. This is your competitive advantage against well-funded competitors.
- **Track weekly active GitHub stars, downloads, and community contributions.** One dashboard, updated weekly.

### Sources

- Paul Graham, "Do Things That Don't Scale" (2013): http://paulgraham.com/ds.html
- https://www.saastr.com/the-essential-startup-advice-for-founders-with-y-combinators-michael-seibel-podcast-488-and-video/
- https://charisol.io/how-to-get-into-y-combinator-as-a-solo-founder/

---

## 5. Shape Up (Basecamp/37signals)

### The Anti-Scrum

Shape Up was created by Ryan Singer at Basecamp as an alternative to Scrum. The key insight: **Scrum gives you a process but not a strategy for deciding what to build.** Shape Up provides both.

### The Three Phases

```
SHAPING (2 people, ~2 weeks)     BETTING TABLE (30 min)     BUILDING (6 weeks, protected)
Define problem + boundaries  -->  Decide what to bet on  -->  Ship it, no interruptions
Rough solution, not spec         1-2 projects max             Fixed time, variable scope
De-risk before committing        Kill what does not fit       Cool-down follows (1 week)
```

### Key Concepts

**Appetite vs. Estimation:**
Estimation asks "how long will this take?" -- leading to scope creep and padded timelines. Appetite asks **"how much time is this worth?"** -- leading to creative scoping. Example: "We are willing to spend 3 weeks on user dashboard" means you find the version of the dashboard that fits in 3 weeks. Not the full version with padding.

**Fixed Time, Variable Scope:**
The cycle length never changes. What changes is what you ship within it. If the full feature takes 6 weeks but you have 3, you find the 3-week version. This is not cutting corners -- it is creative scoping.

**Hill Charts:**
A progress visualization. Left side of the hill = "figuring things out" (unknowns). Right side = "making it happen" (execution). Tasks start on the left and move over the hill. This shows real progress better than percentage bars. If something is stuck on the left side, it signals a scoping/design problem, not an effort problem.

**The Betting Table:**
A short meeting where shaped pitches are evaluated. You "bet" your team's time on 1-2 projects. Unselected pitches are not put in a backlog -- they are discarded. If they matter, they will come back. This prevents backlog anxiety.

**Cool-Down (1 week between cycles):**
Not a sprint. Activities: bug fixes, housekeeping, skill-building, initial shaping for next cycle. No new building starts.

### Adapting for a 2-Person Team

The standard 6-week cycle is designed for teams of 3-6. For 2 people, adapt:

| Standard Shape Up | 2-Person Adaptation |
|-------------------|-------------------|
| 6-week building cycle | **3-week cycle** (enough to ship something real, short enough to feel the deadline) |
| 2-week cool-down | **1-week cool-down** (bugs, docs, shaping) |
| Separate shaper and builders | **Both people shape together** in cool-down week, then build together |
| Betting table with stakeholders | **30-minute conversation** between the two of you: "What is worth our next 3 weeks?" |
| Multiple parallel projects | **One project per cycle.** Full focus. |
| Formal pitches | **1-page pitch** (problem, appetite, solution sketch, rabbit holes to avoid) |

### The 3-Week Rhythm

```
Week 1-3: BUILD (one shaped project, full focus, no interruptions)
Week 4:   COOL-DOWN (bugs, docs, community, shape next cycle)
```

This gives you **~13 cycles per year** instead of Shape Up's 8. More bets, faster iteration.

### Applicable to a 2-Person Team

- **Replace sprints with 3-week cycles + 1-week cool-down.** Each cycle has one clear bet.
- **Write a 1-page pitch before each cycle.** Problem, appetite (3 weeks max), rough solution, rabbit holes.
- **Use hill charts** (even on paper) to track whether you are in "figuring out" or "making it happen" mode.
- **No backlog.** Pitches that do not get bet on are discarded. If they matter, they resurface.
- **Protect the cycle.** No new work enters a cycle once it starts. Bugs and requests go to cool-down.

### Sources

- Ryan Singer, *Shape Up* (2019): https://basecamp.com/shapeup
- https://37signals.com/06
- https://www.productplan.com/glossary/shape-up-method/
- https://marmelab.com/blog/2024/09/26/shape-up.html

---

## 6. GitHub + Linear Integration

### Native Integration (Do This First)

Linear has a built-in GitHub integration. Settings > Integrations > GitHub. Enable it.

**What it does automatically:**
- Links commits, branches, and PRs to Linear issues via issue identifiers
- Updates issue status based on PR lifecycle (open -> In Progress, merge -> Done)
- Shows PR status and CI checks on the Linear issue

### Magic Words in Commits and PRs

Linear recognizes **magic words** in commit messages and PR descriptions:

| Magic Word | Effect on Merge |
|------------|----------------|
| `closes ENG-123` | Moves issue to Done |
| `fixes ENG-123` | Moves issue to Done |
| `resolves ENG-123` | Moves issue to Done |
| `refs ENG-123` | Links without status change |
| `references ENG-123` | Links without status change |

When you push a branch with a closing magic word, the issue moves to **In Progress**. When the PR merges, it moves to **Done**.

### Branch Naming Convention

Use the Linear issue identifier in the branch name:

```
git checkout -b fix/ENG-123-dag-cycle-detection
git checkout -b feat/ENG-456-structured-output-repair
```

Linear auto-detects the `ENG-123` pattern and links the branch to the issue.

### Recommended Commit Format for Nika + Linear

Combine your existing commit convention with Linear identifiers:

```
fix(runtime): resolve DAG cycle in nested includes

Fixes executor panic when include chains form cycles.

closes ENG-123

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

### GitHub Action: Auto-Link PRs to Linear Issues

```yaml
# .github/workflows/linear-link.yml
name: Link PR to Linear
on:
  pull_request:
    branches: [main]
    types: [opened, edited, reopened, synchronize]
permissions:
  pull-requests: write
jobs:
  linear-link:
    runs-on: ubuntu-latest
    steps:
      - name: Find or create Linear issue
        uses: ctriolo/action-find-or-create-linear-issue@v1
        id: linear
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
          linear-api-key: ${{ secrets.LINEAR_API_KEY }}
          linear-team-key: "ENG"
      - name: Comment PR with Linear URL
        uses: actions/github-script@v7
        with:
          script: |
            github.rest.issues.createComment({
              issue_number: context.issue.number,
              owner: context.repo.owner,
              repo: context.repo.repo,
              body: `Linear issue: ${{ steps.linear.outputs.linear-issue-url }}`
            })
```

### Secrets Needed

| Secret | Where to Get It |
|--------|----------------|
| `LINEAR_API_KEY` | Linear > Settings > API > Personal API Key |
| `GITHUB_TOKEN` | Auto-provided by GitHub Actions |

### Applicable to a 2-Person Team

- **Enable native integration first.** It handles 90% of needs with zero config.
- **Use `closes ENG-123` in every commit.** Make it muscle memory. Issues auto-close on merge.
- **Branch names include issue IDs.** `feat/ENG-123-description` is the pattern.
- **Add the GitHub Action only if you need auto-creation** of issues from PRs (useful when community contributors submit PRs without Linear context).

### Sources

- https://linear.app/docs/github-integration
- https://linear.app/integrations/github
- https://github.com/marketplace/actions/find-or-create-linear-issue-and-prefix-pr
- https://github.com/linear/linear

---

## 7. Telegram Bots for CI/Deploy/Release

### Why Telegram (Not Slack/Discord)

For a 2-person team, Telegram is the fastest notification channel: always on your phone, instant delivery, no workspace overhead. Use it as your mission control alert system.

### Step 1: Create a Telegram Bot

1. Message **@BotFather** on Telegram
2. Send `/newbot`, follow prompts to name it (e.g., "Nika CI Bot")
3. Save the **bot token** (format: `123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11`)
4. Start a chat with your bot (send any message)
5. Visit `https://api.telegram.org/bot<TOKEN>/getUpdates` to find your **chat ID**
6. Store both as GitHub secrets: `TELEGRAM_TOKEN` and `TELEGRAM_TO`

### Step 2: GitHub Actions Notifications

Use `appleboy/telegram-action` -- the most maintained Telegram action.

#### Notify on CI Failure

```yaml
# .github/workflows/ci-notify.yml
name: CI with Telegram Alerts
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run tests
        run: cd tools && cargo test --workspace --lib
      - name: Alert on failure
        if: failure()
        uses: appleboy/telegram-action@v1.1.0
        with:
          to: ${{ secrets.TELEGRAM_TO }}
          token: ${{ secrets.TELEGRAM_TOKEN }}
          format: markdown
          message: |
            *CI FAILED* on `${{ github.ref_name }}`
            Commit: `${{ github.sha }}`
            By: ${{ github.actor }}
            [View logs](${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }})
```

#### Notify on Release

```yaml
# .github/workflows/release-notify.yml
name: Release Notification
on:
  release:
    types: [published]
jobs:
  notify:
    runs-on: ubuntu-latest
    steps:
      - name: Telegram release alert
        uses: appleboy/telegram-action@v1.1.0
        with:
          to: ${{ secrets.TELEGRAM_TO }}
          token: ${{ secrets.TELEGRAM_TOKEN }}
          format: markdown
          message: |
            *Nika ${{ github.event.release.tag_name }} released*
            ${{ github.event.release.body }}
            [Download](${{ github.event.release.html_url }})
```

#### Notify on PR Merged

```yaml
# .github/workflows/pr-notify.yml
name: PR Merged Notification
on:
  pull_request:
    types: [closed]
jobs:
  notify:
    if: github.event.pull_request.merged == true
    runs-on: ubuntu-latest
    steps:
      - name: Telegram PR alert
        uses: appleboy/telegram-action@v1.1.0
        with:
          to: ${{ secrets.TELEGRAM_TO }}
          token: ${{ secrets.TELEGRAM_TOKEN }}
          format: markdown
          message: |
            *PR merged:* ${{ github.event.pull_request.title }}
            By: ${{ github.event.pull_request.user.login }}
            [View](${{ github.event.pull_request.html_url }})
```

### Step 3: Linear Notifications via Webhook (Advanced)

Linear supports outgoing webhooks. To forward Linear events to Telegram:

**Option A: Serverless function (simplest)**
1. Create a Cloudflare Worker or Vercel Edge Function
2. Linear webhook URL = your function endpoint
3. Function parses the Linear payload and calls Telegram API

**Option B: Rust bot with teloxide (if you want it in your stack)**

```toml
# Cargo.toml
[dependencies]
teloxide = { version = "0.12", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
axum = "0.7"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

The bot runs a webhook server that:
1. Receives POST from Linear (issue created, status changed, etc.)
2. Formats a message
3. Sends to your Telegram chat via Bot API

### Bot Framework Comparison

| Framework | Language | Best For |
|-----------|----------|----------|
| **teloxide** | Rust | Production bots in your Rust stack, full Telegram Bot API v9.1 |
| **python-telegram-bot** | Python | Quick prototypes, Lambda/CF Worker deployments |
| **appleboy/telegram-action** | YAML | GitHub Actions notifications (no custom bot needed) |
| **Direct curl** | Any | Simplest possible -- one HTTP POST, no dependencies |

### Applicable to a 2-Person Team

- **Start with `appleboy/telegram-action` for GitHub events.** 15 minutes to set up.
- **Add Linear webhooks via a Cloudflare Worker** when you adopt Linear. 30 minutes.
- **Do not build a custom teloxide bot** unless you specifically want Telegram as a command interface (e.g., `/deploy`, `/status`).
- **One Telegram group for all alerts.** Do not over-channel. Two people do not need separate channels for CI, releases, and issues.

### Sources

- https://github.com/appleboy/telegram-action
- https://core.telegram.org/bots/api
- https://github.com/teloxide/teloxide
- https://codewords.ai/blog/a-complete-guide-to-telegram-automations

---

## 8. Synthesis: The SuperNovae Playbook

### Combining All Seven Philosophies

Here is how these philosophies combine into a concrete operating system for a 2-person Paris AI startup building open source tools:

### The Rhythm (Shape Up adapted)

```
3-week BUILD cycle --> 1-week COOL-DOWN --> repeat
~13 cycles per year
```

- **Monday of cool-down week:** Postmortem on last cycle (Pixar). What surprised us? What did we learn?
- **Tuesday-Wednesday:** Bug fixes, docs, community engagement (YC -- do things that do not scale)
- **Thursday:** Shape next cycle. Write 1-page pitch. Problem, appetite, rabbit holes.
- **Friday:** Betting table (30 min). Pick ONE project for next cycle.

### The Decision Framework (Apple + Linear)

```
Every decision: "Does this serve the core use case?"
  YES --> Does it fit in 3 weeks? --> Scope down until it does --> Build
  NO  --> "Not Doing" list --> Move on
```

- **DRI for everything.** Person A owns runtime/engine. Person B owns CLI/UX/distribution.
- **Write issues, not stories.** "Fix NIKA-041 template resolution for nested for_each" not "As a user..."
- **Opinionated defaults.** Do not add config options. Make the decision.

### The Quality Culture (Pixar + Linear)

- **Accept ugly babies.** v0.x will be rough. That is the process.
- **Braintrust every 2 weeks.** Show work in progress. Identify problems. Do not prescribe solutions.
- **Craft matters.** Speed of the CLI, quality of error messages, keyboard shortcuts -- these compound.
- **Product debt is strategic.** "We chose not to build X yet" is different from "X is broken."

### The Growth Engine (YC)

- **Find 10 users who love Nika.** Not 1000 who signed up.
- **Talk to users weekly.** Not surveys. Actual conversations.
- **Do things that do not scale.** Write custom workflows for early adopters. Debug with them. Join their projects.
- **Launch something every week.** Release, blog post, showcase, tutorial.
- **Track one metric.** Weekly active users or weekly workflow runs.

### The Toolchain (GitHub + Linear + Telegram)

```
Linear (plan) --> GitHub (build) --> Telegram (monitor)

1. Create Linear issue: ENG-123
2. Branch: feat/ENG-123-structured-output
3. Commit: fix(runtime): repair structured output validation\n\ncloses ENG-123
4. PR merges --> Linear auto-closes ENG-123
5. Release published --> Telegram bot notifies both founders
6. CI fails --> Telegram bot alerts immediately
```

**Setup checklist:**
- [ ] Enable Linear <> GitHub native integration
- [ ] Add `TELEGRAM_TOKEN` and `TELEGRAM_TO` to GitHub secrets
- [ ] Add `appleboy/telegram-action` to CI workflow (failure alerts)
- [ ] Add `appleboy/telegram-action` to release workflow (release alerts)
- [ ] Commit convention: `type(scope): description\n\ncloses ENG-123`
- [ ] Branch convention: `type/ENG-123-short-description`

### The Anti-Patterns (What NOT to Do)

| Anti-Pattern | Why | Instead |
|-------------|-----|---------|
| Sprint planning meetings | 2 people do not need ceremonies | 30-min betting table, 1x per cycle |
| Jira-style backlogs | Backlog anxiety, stale items | Discard unbet pitches. They come back if they matter. |
| Estimation in hours | Leads to padding and guilt | Appetite: "Is this worth 3 weeks?" |
| Feature flags for everything | Complexity tax | Ship or do not ship. Decide. |
| "We should add an option for that" | Config is a design failure | Make the opinionated choice. |
| Separate channels for everything | Alert fatigue | One Telegram group. Two people. |
| Weekly all-hands | You are two people | Talk when needed. Protect deep work. |
| Roadmap presentations | No audience | 1-page pitch per cycle. Done. |

---

## Confidence Level

**High** for philosophical principles (Pixar, Apple, Linear, YC, Shape Up) -- these are well-documented, widely cited, and internally consistent.

**High** for GitHub + Linear integration -- verified against official documentation and working GitHub Actions.

**Medium-High** for Telegram setup -- `appleboy/telegram-action` is actively maintained and widely used, but version pinning should be verified at setup time.

## Further Research Suggestions

- **Linear API for custom automations** (e.g., auto-create issues from GitHub Discussions)
- **Cloudflare Workers for Linear -> Telegram bridge** (specific implementation)
- **Open source growth playbook** (how Astro, Deno, Bun built early communities)
- **Revenue models for open source AI tools** (e.g., hosted version, enterprise features)
