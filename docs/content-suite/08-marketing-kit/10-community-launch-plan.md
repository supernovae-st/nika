# Community Launch Plan -- Nika

> Community building strategy for the first 100 users and beyond.
> Discord, GitHub Discussions, content calendar, ambassador program, conference talks.

---

## 1. Community Platform Setup

### GitHub Discussions (Primary)

GitHub Discussions is the primary community hub -- it lives where the code lives.

**Categories to create:**

| Category | Type | Purpose |
|----------|------|---------|
| Announcements | Announcement | Release notes, roadmap updates, events |
| Show & Tell | Show & Tell | User-built workflows, showcase contributions |
| Q&A | Question/Answer | Technical questions, troubleshooting |
| Ideas | Idea | Feature requests, design proposals |
| General | General | Off-topic, introductions, meta |

**Pinned discussions:**
1. "Welcome to Nika -- Start Here" (quickstart, links, expectations)
2. "Roadmap: What's Coming in v0.42" (transparency, input invitation)
3. "FAQ: Common Questions" (AGPL, YAML, Rust, providers)

### Discord Server (Secondary)

Discord for real-time conversation and community building.

**Channel structure:**

```
NIKA
├── #welcome           Rules, roles, getting started
├── #announcements     Releases, events (read-only)
├── #general           Open conversation
├── #help              Technical support
├── #showcase          Share what you built
├── #ideas             Feature brainstorming
├── #rust-dev          Contributor discussion
├── #yaml-workshop     Workflow design help
└── #off-topic         Everything else

VOICE
├── #pair-programming  Live coding sessions
└── #office-hours      Weekly Q&A with maintainers
```

**Bot setup:**
- GitHub integration: new issues, PRs, releases posted to #announcements
- Welcome bot: greet new members, link to quickstart
- Role assignment: Contributor, Course Graduate, Showcase Author

### GitHub Issues (Bug Tracking)

**Issue templates:**
1. Bug Report (NIKA-XXX error code, reproduction steps, expected vs actual)
2. Feature Request (use case, proposed YAML syntax, alternatives considered)
3. Course Feedback (level, exercise, what was confusing)
4. Showcase Submission (workflow description, YAML file, category)

**Labels:**
- `good first issue` -- Onboarding for new contributors
- `help wanted` -- Community contributions welcome
- `course` -- Course-related issues
- `media-tools` -- Media pipeline issues
- `provider:*` -- Provider-specific issues
- `tui` -- Terminal UI issues
- `lsp` -- Language server issues

---

## 2. First 100 Users Strategy

### Phase 1: Seeds (Users 1-10)

**Timeline:** Pre-launch to launch day
**Goal:** 10 power users who understand Nika deeply

**Tactics:**
1. **Personal outreach** -- DM 20 developers who have tweeted about YAML/AI/Rust workflow tools. Send them early access with a personal note.
2. **Rust community** -- Post in r/rust, Rust users forum, and Rust Discord. Lead with the technical story (451K lines, 2-phase AST, tokio DAG scheduler).
3. **AI builder community** -- Share in AI Discord servers (Latent Space, MLOps Community, AI Engineers). Lead with the 5-verb paradigm.
4. **One-on-one pairing** -- Offer to pair-program with the first 10 users over video call. Build their first workflow together. Record (with permission) for future content.

**Success metric:** 10 users who have completed at least 3 course levels.

### Phase 2: Sparks (Users 10-50)

**Timeline:** Launch day to week 2
**Goal:** 50 users with diverse use cases

**Tactics:**
1. **Product Hunt launch** -- Full campaign (see 01-product-hunt-launch.md)
2. **Hacker News Show HN** -- Technical post (see 03-hacker-news-launch.md)
3. **Dev.to article** -- Long-form technical piece (see 05-dev-to-article.md)
4. **Twitter thread series** -- 5 threads over 5 days (see 04-twitter-thread-series.md)
5. **Reddit posts** -- r/rust, r/artificial, r/opensource, r/programming
6. **"What would you build?" challenge** -- Invite 50 people to build a workflow and share it. Best submission gets featured in the showcase.

**Success metric:** 50 GitHub stars, 20 cargo installs, 5 user-submitted showcase workflows.

### Phase 3: Flame (Users 50-100)

**Timeline:** Weeks 2-4
**Goal:** Self-sustaining community activity

**Tactics:**
1. **Weekly Office Hours** -- 30-minute Discord voice call. Maintainer answers questions live. Record and post as community content.
2. **Course Completion Campaign** -- Challenge users to complete all 12 levels. Those who do get a "Course Graduate" Discord role and a shout-out in the newsletter.
3. **Showcase Sprint** -- Host a 48-hour event where participants build and submit workflows. Categories: "Most Creative," "Most Practical," "Best Multi-Model."
4. **Blog post series** -- "How I Built X with Nika" posts by early users. Offer to ghostwrite for users who have good stories but don't want to write.
5. **Contributor onboarding** -- Label 10 issues as "good first issue." Write detailed contribution guides for each. Personally mentor first-time contributors.

**Success metric:** 100 GitHub stars, 50 Discord members, 10 user-contributed showcase workflows, 3 external blog posts.

---

## 3. Content Calendar (First 30 Days)

### Week 0 (Pre-Launch)

| Day | Content | Platform | Owner |
|-----|---------|----------|-------|
| Mon | Teaser email to newsletter | Email | Thibaut |
| Tue | "Something is coming" tweet with code snippet | X | Thibaut |
| Wed | Early access DMs to 20 target users | X/DMs | Thibaut |
| Thu | Set up GitHub Discussions + Discord | GitHub/Discord | Thibaut |
| Fri | Preview blog post: "Why 5 verbs?" | Blog draft | Thibaut |

### Week 1 (Launch Week)

| Day | Content | Platform | Owner |
|-----|---------|----------|-------|
| Mon | **LAUNCH DAY** -- PH, HN, email, Twitter Thread 1 | All | Thibaut |
| Tue | Twitter Thread 3: "5 Verbs Is All You Need" | X | Thibaut |
| Tue | Dev.to article published | Dev.to | Thibaut |
| Wed | Twitter Thread 2: "Why We Chose Rust" | X | Thibaut |
| Wed | r/rust post | Reddit | Thibaut |
| Thu | Twitter Thread 5: "What You Can Build" | X | Thibaut |
| Thu | r/artificial, r/programming posts | Reddit | Thibaut |
| Fri | Twitter Thread 4: "Open Source Is Liberation" | X | Thibaut |
| Fri | Week 1 recap email to newsletter | Email | Thibaut |

### Week 2 (Momentum)

| Day | Content | Platform | Owner |
|-----|---------|----------|-------|
| Mon | First Office Hours (Discord voice) | Discord | Thibaut |
| Tue | "How to Build a Content Pipeline with Nika" tutorial | Blog | Thibaut |
| Wed | User spotlight: first showcase submission | X + GitHub | Thibaut |
| Thu | "Nika vs LangChain: A Honest Comparison" post | Blog/Dev.to | Thibaut |
| Fri | Course completion challenge announcement | Discord/X | Thibaut |

### Week 3 (Community)

| Day | Content | Platform | Owner |
|-----|---------|----------|-------|
| Mon | Office Hours #2 | Discord | Thibaut |
| Tue | "Advanced: Multi-Model Cost Optimization" tutorial | Blog | Thibaut |
| Wed | Contributor spotlight: first merged PR | X + GitHub | Thibaut |
| Thu | Showcase Sprint announcement | Discord/X | Thibaut |
| Fri | "State of the Nika Community: Week 3" newsletter | Email | Thibaut |

### Week 4 (Establish Rhythm)

| Day | Content | Platform | Owner |
|-----|---------|----------|-------|
| Mon | Office Hours #3 | Discord | Thibaut |
| Tue | "Building a Media Pipeline with Nika" tutorial | Blog | Thibaut |
| Wed | Showcase Sprint (48 hours) starts | Discord | Community |
| Fri | Showcase Sprint ends, winners announced | Discord/X | Thibaut |
| Fri | Month 1 retrospective newsletter | Email | Thibaut |

---

## 4. Ambassador Program

### "Nika Navigators" -- Community Ambassador Program

**Goal:** Identify and empower 5-10 community leaders who extend the project's reach.

### Criteria

- Completed at least 8/12 course levels
- Contributed at least 1 showcase workflow OR 1 PR
- Active in Discord/Discussions for 2+ weeks
- Genuine enthusiasm (not just clout-chasing)

### Benefits

- **Discord role:** "Navigator" with distinct color
- **Early access:** Preview releases before public launch
- **Direct channel:** Private Discord channel with Thibaut
- **Conference support:** Travel stipend for speaking about Nika at conferences
- **Merch:** Nika-branded stickers, t-shirt (butterfly logo)
- **Credit:** Named in CONTRIBUTORS.md and release notes

### Responsibilities

- Answer community questions in Discord/Discussions (2-3 per week)
- Write or review 1 tutorial per month
- Test pre-releases and provide feedback
- Represent Nika at local meetups or online events (optional)

### Onboarding

1. Invitation via personal DM
2. 30-minute video call with Thibaut (expectations, resources, feedback)
3. Added to private Navigator channel
4. First task: write a "Why I Use Nika" blog post

---

## 5. Conference Talk Proposals

### Talk 1: "5 Verbs to Replace Your AI SDK"

**Format:** 25-minute talk + 5-minute Q&A
**Target conferences:** RustConf, EuroRust, Rust Nation, Strange Loop
**Abstract:**

> AI workflow tools in 2026 force a choice: drag-and-drop builders that can't be version-controlled, or Python SDKs with hundreds of abstractions. This talk introduces a third option: 5 declarative YAML verbs that compose into DAG-scheduled AI pipelines.
>
> We'll live-code a multi-model content pipeline, show how automatic DAG scheduling eliminates manual ordering, demonstrate structured output validation with JSON Schema, and explain why we wrote 451K lines of Rust for what could have been a Python script.
>
> Attendees will leave with a working understanding of declarative AI workflows and why constraints (5 verbs, not 50) can be a feature, not a limitation.

**Key demo moments:**
1. Write a 3-task workflow live (fetch -> infer -> exec)
2. Show automatic parallel execution (add a second fetch, watch both run simultaneously)
3. Switch providers by changing one line (Claude -> Groq -> DeepSeek)
4. Show the TUI visualizing the DAG in real-time

---

### Talk 2: "451K Lines of Rust: Building a Workflow Engine That Compiles"

**Format:** 40-minute talk + 10-minute Q&A
**Target conferences:** RustConf, FOSDEM Rust devroom, Rustaceans meetups
**Abstract:**

> This is the story of building a 451K-line Rust application -- a workflow engine for AI tasks. We'll cover the architectural decisions that made Rust the right choice: a 2-phase AST with source spans, a DAG scheduler using tokio JoinSet, a content-addressable media store, and a 92K-line TUI in ratatui.
>
> We'll also cover what was hard: async closures with lifetime annotations, dynamic dispatch in a DAG scheduler, and the tension between Rust's ownership model and workflow engine patterns that want shared mutable state.
>
> This talk is for Rust developers building large applications who want real-world war stories about async, ratatui, serde, and the crate ecosystem.

---

### Talk 3: "YAML-First AI Workflows: From Skeptic to Convert"

**Format:** 20-minute lightning talk
**Target conferences:** AI Engineer Summit, MLOps World, DevOps Days
**Abstract:**

> "YAML for AI workflows? Really?" This talk addresses the skepticism head-on: why YAML is actually the right format for AI pipelines when you constrain it to 5 semantic verbs, how automatic DAG scheduling makes explicit ordering unnecessary, and why workflows-as-documentation changes how teams collaborate on AI.
>
> Live demo: build a multi-model pipeline that would take 200 lines of LangChain in 20 lines of YAML.

---

### Talk 4: "Open Source Under AGPL: Why We Chose the Nuclear Option"

**Format:** 15-minute talk
**Target conferences:** FOSDEM, Open Source Summit, All Things Open
**Abstract:**

> We chose AGPL-3.0 for a 451K-line open source project. This talk explains why: the pattern of cloud providers strip-mining open source projects (Elasticsearch, Redis, Terraform), why MIT/Apache don't protect the commons, and how the AGPL aligns incentives between creators and users.
>
> We'll share practical advice: how to communicate the AGPL to enterprise users, what it means (and doesn't mean) for your users, and whether it actually scares away contributions (spoiler: no).

---

## 6. Meetup Topics

### Monthly Meetup Series: "Nika Workshop"

Format: 90-minute virtual workshop. 30 min presentation, 60 min hands-on coding.

| Month | Topic | Skill Level |
|------:|-------|-------------|
| 1 | "Your First 5 Workflows" -- one per verb | Beginner |
| 2 | "Multi-Model Pipelines" -- cost optimization | Intermediate |
| 3 | "Building Agents with Guardrails" -- the agent verb | Intermediate |
| 4 | "Media Pipeline Deep Dive" -- 24 tools | Intermediate |
| 5 | "MCP Integration" -- connecting to external tools | Advanced |
| 6 | "Contributing to Nika" -- crate tour, testing, PRs | Advanced |

### Format

1. **Introduction** (5 min): what we'll build today
2. **Live coding** (25 min): build the workflow step by step
3. **Hands-on** (50 min): participants build their own variation
4. **Show & Tell** (10 min): participants share what they built

### Tools needed
- Pre-built Codespace with Nika installed
- Shared GitHub repo with exercise templates
- Discord voice channel for live Q&A

---

## 7. Community Metrics and Goals

### Month 1 Goals

| Metric | Target | How to Measure |
|--------|--------|---------------|
| GitHub Stars | 200 | GitHub API |
| cargo installs | 100 | crates.io API |
| Discord members | 50 | Discord analytics |
| Course completions (L1-3) | 30 | Community survey |
| Full course completions (L12) | 5 | Community survey |
| User-submitted showcases | 10 | GitHub PRs |
| External blog posts | 3 | Social media monitoring |
| Issues filed | 25 | GitHub API |
| PRs from non-maintainers | 5 | GitHub API |
| Newsletter subscribers | 200 | Email platform |

### Month 3 Goals

| Metric | Target |
|--------|--------|
| GitHub Stars | 1,000 |
| Monthly cargo installs | 500 |
| Discord members | 200 |
| Navigators (ambassadors) | 5 |
| Conference talks submitted | 3 |
| Total showcase workflows | 50 (250+ total with built-in) |
| Newsletter subscribers | 500 |

### Month 6 Goals

| Metric | Target |
|--------|--------|
| GitHub Stars | 5,000 |
| Monthly cargo installs | 2,000 |
| Discord members | 500 |
| Active contributors (past 30 days) | 20 |
| Conference talks accepted | 2 |
| Homebrew installs | 500/month |
| First corporate adopter | 1 |

---

## 8. Community Values

### The Nika Community Code

1. **Teach, don't gatekeep.** Everyone started with zero knowledge. Help them get to five verbs.
2. **Show your workflows.** The best community content is working YAML, not opinions about YAML.
3. **Honest comparisons.** LangChain, Dify, Temporal -- they're good tools for different jobs. No tribalism.
4. **Contributions over complaints.** Found a bug? File an issue with the NIKA-XXX code. Have an idea? Show us the YAML.
5. **AGPL means shared.** We protect the commons. You protect the commons. That's the deal.

### Moderation

- **Zero tolerance:** harassment, discrimination, spam
- **One warning:** off-topic threads, self-promotion without context
- **Community-first:** feature discussions welcome; vendor pitches are not
- **Language:** English as the primary language; other languages welcome in dedicated channels

---

## 9. Growth Flywheel

```
New User
   |
   v
nika init --course  ------>  Course Completion  ------>  Showcase Submission
   |                              |                            |
   v                              v                            v
Discord Join              "I built X" tweet             Featured in showcase
   |                              |                            |
   v                              v                            v
Asks question             Others see tweet              Others try Nika
   |                              |                            |
   v                              v                            v
Gets helped               New users arrive              More showcases
   |                                                           |
   v                                                           v
Becomes helper   <---------  Becomes Navigator  <---------  Repeat
```

**The key insight:** The course system is the top of the funnel. Every user who runs `nika init --course` is a potential community member. The progressive structure (12 levels, constellation map) creates completion incentive. Completed users become showcase contributors. Showcase content attracts new users.

The course IS the growth engine.

---

## 10. Partnership Opportunities

### Potential Integration Partners

| Partner | Integration | Mutual Benefit |
|---------|------------|----------------|
| **Anthropic** | Nika is a Claude workflow tool | They get ecosystem, we get visibility |
| **MCP ecosystem** | Nika is a top MCP client | Nika drives MCP adoption |
| **ratatui** | Nika's TUI is 92K lines of ratatui | Showcase project for their framework |
| **rig-core** | Nika uses rig-core for LLM abstraction | Validation of their library |
| **Homebrew** | Distribution channel | Broader reach |
| **VS Code Marketplace** | LSP extension | IDE integration |

### Content Partnerships

- **Dev.to:** Featured author / sponsored article
- **Rust Blog:** Guest post on building large Rust applications
- **AI Engineer newsletter:** Sponsored mention
- **Changelog podcast:** Episode on YAML-first AI workflows
- **Rustacean Station podcast:** Episode on 451K lines of Rust

---

*Prepared for SuperNovae Studio. Last updated 2026-03-23.*
