# Research: Internal Organization for Small Startups (2-5 People)

> Date: 2026-03-30
> Sources: 30+ pages analyzed (HN threads, GitLab Handbook, Notion templates, Basecamp/Shape Up, Pixar pipeline docs, Apple ANPP process, open source repos)
> Confidence: High for folder structures, Medium for proprietary company internals (Apple, Pixar)

---

## Executive Summary

Small startups (2-5 people) do not need departments. They need **zones of responsibility** organized around how work actually flows, not org charts. The best systems combine three principles: (1) action-oriented taxonomy (PARA), (2) phase-based workflows (Pixar pipeline), and (3) radical transparency (GitLab handbook). Below are concrete, copy-pasteable folder structures synthesized from real companies and frameworks.

---

## 1. YC Startup Documentation Patterns

### What 2-Person Startups Actually Need

YC startups at pre-seed/seed avoid formal departments entirely. From HN threads and YC Library:

**Minimum Viable Sections (not departments):**

| Zone | What Lives Here | Owner |
|------|----------------|-------|
| Product | Roadmap, specs, user feedback, design files | Founder 1 |
| Engineering | Code docs, ADRs, deploy guides, incident playbooks | Founder 2 |
| Operations | Legal, finance, hiring, admin | Both |
| Growth | Marketing, content, analytics, outreach | Both |
| Knowledge | Research, competitive intel, learning, references | Both |

**The YC-style engineering-first pattern** (from HN discussions):
```
/                              # Root = the company
README.md                     # Company overview, mission, how we work
HANDBOOK.md                   # Values, processes, onboarding

docs/
  onboarding.md               # New hire guide (updated BY new hires)
  architecture.md              # System overview
  rfcs/                        # Decision records (RFC-001-feature-name.md)
  howto/                       # Goal-oriented: HOWTO-deploy.md, HOWTO-debug.md
  faq/                         # FAQ-billing.md, FAQ-infrastructure.md

src/                           # Code
tests/                         # Tests
```

**Key insight**: YC companies keep docs in the repo, not in Notion. The README is the hub. New hires update onboarding docs during their first week (so they stay current).

Sources: https://news.ycombinator.com/item?id=41415619, https://news.ycombinator.com/item?id=39370226

---

## 2. How Open Source Companies Organize Non-Code Assets

### GitLab: The Gold Standard (2,000+ page public handbook)

GitLab's handbook is the most documented company structure in existence:

```
handbook/
  company/
    values/
    mission/
    communication/
    structure/                 # Org chart, layers, working groups
  people-group/
    policies/
    hiring/
    diversity/
    total-rewards/
    learning-development/
  product/
    principles/
    security/
    categories/
  engineering/
    development/
    infrastructure/
    security-practices/
    customer-support/
  sales/
    commercial/
    customer-success/
    field-operations/
    solutions-architecture/
  marketing/
    brand/
    content/
    campaigns/
    community/
  finance/
    billing/
    procurement/
    reporting/
  legal/
    contracts/
    compliance/
    ip/
```

Source: handbook.gitlab.com

### Open Source Repo Non-Code Folders (Real Examples)

From cal.com, documenso, twenty, hoppscotch, infisical:

```
.github/
  ISSUE_TEMPLATE/
  PULL_REQUEST_TEMPLATE/
  CODEOWNERS
  FUNDING.yml
  workflows/

docs/                          # User/dev documentation
brand/                         # Logos, colors, fonts, guidelines
assets/                        # Screenshots, promo images, icons
contributing/                  # CONTRIBUTING.md, CODE_OF_CONDUCT.md
rfcs/                          # Architecture decision records
designs/                       # Wireframes, specs, Figma links
legal/                         # LICENSE, CLA, terms
marketing/                     # Landing page source, blog posts
```

### Basecamp/37signals: The Anti-Structure

37signals (60 people, fully remote) deliberately eliminates:
- Project managers (they make PM software, irony intentional)
- Middle management
- Dedicated sales team
- Traditional recruiters

**Their structure**:
- 6-week cycles (Shape Up methodology)
- Every employee is a "manager of one"
- Senior staff do management work alongside IC work
- 5-level mastery progression (Junior to Principal)
- All work organized in Basecamp itself via Projects and Stacks

Sources: https://37signals.com, Shape Up book

---

## 3. The "Company-in-a-Folder" Concept

### Pattern A: Functional (by department)

Best for teams that think in terms of "who owns what":

```
company/
  strategy/
    vision.md                  # Mission, vision, values
    okrs/                      # OKR-2026-Q1.md
    board/                     # Board decks, investor updates
    fundraising/               # Pitch deck, data room, term sheets
  brand/
    guidelines/                # Brand book, voice & tone
    logos/                      # SVG, PNG, dark/light variants
    colors/                    # Palette files, Figma tokens
    fonts/                     # Licensed font files
    templates/                 # Slide decks, letterhead, email sigs
  marketing/
    content/                   # Blog posts, newsletters
    social/                    # Post templates, content calendar
    campaigns/                 # Campaign-name/brief.md, assets/
    analytics/                 # Reports, dashboards
    press/                     # Press kit, media list, PR templates
    competitive/               # Competitor profiles, feature matrices
  product/
    roadmap/                   # Roadmap-2026.md
    specs/                     # Feature specs, PRDs
    research/                  # User interviews, surveys
    design/                    # Figma exports, wireframes
    feedback/                  # User feedback log
  engineering/
    docs/                      # Architecture, ADRs, RFCs
    runbooks/                  # Incident response, deploy guides
    apis/                      # API docs
  operations/
    processes/                 # How we hire, how we ship, how we decide
    tools/                     # Tool inventory, access management
    vendors/                   # Vendor contacts, contracts
  legal/
    contracts/                 # Templates, signed agreements
    compliance/                # GDPR, privacy policy
    ip/                        # Trademarks, patents
  finance/
    budget/                    # Annual budget, forecasts
    invoices/                  # Sent/received
    reports/                   # Monthly P&L, runway calculations
  hiring/
    roles/                     # Job descriptions
    process/                   # Interview guides, scorecards
    pipeline/                  # Candidate tracker
```

### Pattern B: Temporal/Phase-Based (by lifecycle)

Best for teams that think in terms of "what phase is this in":

```
company/
  00-foundation/               # Things that rarely change
    mission.md
    values.md
    brand-guidelines/
    legal-templates/
  01-planning/                 # Current cycle planning
    okrs/
    roadmap/
    competitive-research/
    user-research/
  02-building/                 # Active work
    specs/
    designs/
    engineering-docs/
    sprint-notes/
  03-shipping/                 # Launch & distribution
    launch-plans/
    marketing-campaigns/
    press-kits/
    changelog/
  04-growing/                  # Post-launch optimization
    analytics/
    feedback/
    content-calendar/
    outreach/
  05-operating/                # Ongoing business
    finance/
    hiring/
    vendor-management/
    incident-reports/
  99-archive/                  # Completed/deprecated
    past-campaigns/
    old-specs/
    retired-products/
```

### Pattern C: PARA for Teams (Projects, Areas, Resources, Archives)

Adapted from Tiago Forte's system for company use:

```
company/
  projects/                    # Active, time-bound work
    2026-Q2-launch-v1/
      brief.md
      timeline.md
      assets/
      meeting-notes/
    2026-Q2-funding-round/
    2026-Q3-rebrand/
  areas/                       # Ongoing responsibilities (no end date)
    engineering/
    marketing/
    finance/
    legal/
    hiring/
  resources/                   # Reference material
    competitive-intel/
    market-research/
    brand-assets/
    templates/
    swipe-file/                # Inspiration, examples
  archives/                    # Completed or inactive
    2025-projects/
    old-brand/
    deprecated-docs/
```

### Pattern D: The Monorepo Hybrid (Code + Business)

For teams that want everything in one git repo:

```
supernovae/                    # The monorepo
  apps/                        # Code (deployable units)
    web/
    api/
    cli/
  packages/                    # Shared code libraries
    ui/
    config/
  docs/                        # Technical documentation
    architecture/
    adrs/
    rfcs/
  company/                     # Business operations (non-code)
    strategy/
    brand/
    marketing/
    legal/
    finance/
  .github/                     # CI/CD, templates
  dx/                          # Developer experience
    .claude/
    adr/
```

**Git considerations for sensitive files**:
- `.gitignore` financial details, signed contracts, credentials
- Use git-crypt or SOPS for encrypted secrets
- Keep templates in repo, actual signed docs in secure storage
- Link to external tools (Notion, Drive) for collaborative docs

---

## 4. Specific Folder Taxonomies That Work

### The Johnny Decimal System (for maximum findability)

Numerical codes eliminate ambiguity:

```
10-19  Company
  10  Foundation (mission, values, handbook)
  11  Strategy (OKRs, board, investors)
  12  Legal (contracts, compliance, IP)
  13  Finance (budget, invoices, runway)

20-29  Product
  20  Roadmap
  21  Specs & PRDs
  22  User Research
  23  Design System
  24  Feedback

30-39  Engineering
  30  Architecture
  31  ADRs & RFCs
  32  Runbooks
  33  API Docs

40-49  Brand & Design
  40  Brand Guidelines
  41  Logos & Assets
  42  Templates
  43  Photography

50-59  Marketing & Growth
  50  Content (blog, newsletter)
  51  Social Media
  52  Campaigns
  53  Press & PR
  54  Competitive Intel
  55  Analytics

60-69  Operations
  60  Processes & Playbooks
  61  Tools & Access
  62  Vendors
  63  Hiring

90-99  Archive
  90  Completed Projects
  91  Old Versions
```

### The Notion Sidebar (Seed-Stage Startup)

Concrete Notion workspace for 2-5 people:

```
Favorites (pinned)
  Weekly Sync Notes
  Current Sprint Board
  OKR Dashboard

Workspace
  Company Wiki
    Handbook (values, processes, onboarding)
    Roadmap (timeline database: milestones, owners, status)
    Meeting Notes (database: date, attendees, actions)

  Product
    Feature Specs (database: name, priority, status, owner)
    User Feedback (database: source, type, severity)
    Launch Checklists (templates)

  Engineering
    Sprint Board (database: tasks, assignee, effort, status)
    Bug Tracker (database: severity, repro, fix status)
    Architecture Docs (wiki pages)

  Marketing
    Content Calendar (database: title, channel, publish date, status)
    Campaign Tracker (database: name, goal, budget, metrics)
    Brand Assets (file gallery)

  Operations
    Task Board (database: task, due date, priority, assignee)
    Vendor Directory (database: name, service, contract, cost)

  Finance
    Budget Tracker (database: category, planned, actual)
    Runway Calculator (page with formulas)

  Hiring
    Open Roles (database: title, status, pipeline stage)
    Interview Notes (database: candidate, feedback, score)

  Legal
    Contracts (database: name, type, status, expiry)
    Compliance Checklist (page)

Shared (ad-hoc collaboration)
  1:1 Notes
  Brainstorm Docs

Private (personal drafts)
```

### The RFC/ADR System (Decision Documentation)

Used by Stripe, Uber, Google, HashiCorp, Sourcegraph, SoundCloud:

```
rfcs/
  RFC-001-authentication-system.md
  RFC-002-pricing-model.md
  RFC-003-launch-strategy.md

# Template:
## RFC-NNN: Title
- **Status**: Draft | Review | Accepted | Rejected | Retired
- **Author**: Name
- **Date**: YYYY-MM-DD
- **Reviewers**: Names

### Context
### Problem
### Proposal
### Alternatives Considered
### Success Criteria
### Open Questions
```

---

## 5. The Pixar/Apple Production Framework

### Pixar: 9-Stage Pipeline (3-4 years per film)

```
PRE-PRODUCTION (Story, 1-3 years)
  Story/                       # Script drafts, story treatments
  Storyboards/                 # Scene-by-scene sketches
  Story Reels/                 # Animatic (storyboards + rough audio)
  Art/                         # Character designs, color scripts, mood boards
  Research/                    # Real-world reference (Pixar visits locations)
  Voice/                       # Casting, scratch recordings

PRODUCTION (Execution, 1-2 years)
  Modeling/                    # 3D character/environment models
  Rigging/                     # Skeleton + controls for animation
  Surfaces/                    # Textures, materials, shaders
  Layout/                      # Virtual camera placement, scene blocking
  Animation/                   # Key frames, motion, acting
  Simulation/                  # Hair, cloth, water, particles

POST-PRODUCTION (Polish, 6-12 months)
  Lighting/                    # Virtual lighting setups per scene
  Rendering/                   # Final frame computation
  Compositing/                 # Layer combination, effects
  Sound/                       # Foley, music, mixing
  Editorial/                   # Final cut, timing, pacing
```

**Key Pixar principle**: They spend 2-3 years in pre-production (story) before committing heavy resources. The story reel (rough animatic) is screened internally and iterated dozens of times before production begins.

### Apple: ANPP (Apple New Product Process)

```
PHASE 1: INVESTIGATION
  Business case (few pages only)
  Problem definition
  Customer need validation
  1-2 deliverables max

PHASE 2: CONCEPT
  High-level design
  Delivery details
  Technical feasibility
  Initial prototypes (mock-ups)

PHASE 3: DEVELOPMENT
  Proto (first functional hardware)
  PVTE (pre-production validation testing)
  Go-to-market plan
  4-6 week iteration cycles (continuous redesign)
  Weekly executive product review meetings

PHASE 4: VALIDATION
  MP (mass production)
  Hardware testing
  Final QA
  "Rules of the Road" document (launch action plan)

LAUNCH
  DRI (Directly Responsible Individual) for each action item
  Coordinated hardware + software + marketing + retail
  EPM (Engineering Program Manager) owns device handoff
```

**Key Apple principles**:
- Each phase has only 1-2 deliverables (lean gates)
- Exception management process catches edge cases
- Weekly executive reviews prevent decision delays
- "Rules of the Road" = comprehensive pre-launch checklist with named owners
- DRI system: one person is accountable for each deliverable, no committees

### Mapping to Software Product Launch

```
SOFTWARE EQUIVALENT                    PIXAR PARALLEL         APPLE PARALLEL

01-DISCOVERY (Pre-production)          Story/Storyboards      Investigation
  user-research/                       Research/              Problem definition
  competitive-analysis/                Reference/             Business case
  problem-definition.md                Story treatment        Few-page doc
  personas/                            Character designs      Customer profiles
  wireframes/                          Storyboards            Concept sketches
  prototype/                           Story reel             Initial prototype

02-DESIGN (Pre-production/Production)  Art/Layout             Concept
  design-system/                       Art direction          High-level design
  ui-mockups/                          Layout                 Delivery details
  user-flows/                          Scene blocking         Technical feasibility
  architecture.md                      Production bible       System design

03-BUILD (Production)                  Animation/Modeling     Development
  sprints/                             Animation cycles       4-6 week iterations
  features/                            Scene production       Feature development
  api/                                 Rigging/surfaces       Infrastructure
  testing/                             Simulation             Proto/PVTE

04-POLISH (Post-production)            Lighting/Compositing   Validation
  performance/                         Rendering              Final QA
  accessibility/                       Sound design           Hardware testing
  security-audit/                      Editorial              MP validation
  beta-feedback/                       Test screenings        Beta testing

05-LAUNCH (Release)                    Premiere               Rules of the Road
  launch-plan.md                       Marketing campaign     Launch action plan
  press-kit/                           Press junket           PR coordination
  changelog.md                         Credits                Release notes
  analytics-setup/                     Box office tracking    Metrics dashboard
  support-docs/                        Viewer guides          Help center

06-GROW (Post-release)                 Home video/Sequel      Post-launch
  feedback/                            Audience reception     Customer feedback
  iteration/                           Director's cut         Feature updates
  content/                             Behind-the-scenes      Marketing content
  retrospective.md                     Post-mortem            Lessons learned
```

---

## Synthesis: The SuperNovae Pattern

For a 2-person open source AI startup, combining the best of each approach:

```
supernovae/                            # The company monorepo
  README.md                            # Company overview + links to everything
  HANDBOOK.md                          # How we work (values, processes, norms)

  # === CODE (Engineering) ===
  novanet/                             # Product 1: Knowledge graph
  nika/                                # Product 2: Workflow engine

  # === DX (Developer Experience) ===
  dx/
    .claude/                           # AI editor rules
    adr/                               # Architecture decisions

  # === COMPANY (Business Operations) ===
  company/
    00-foundation/
      mission.md
      values.md
      brand/
        guidelines.md
        logos/
        colors.md
        fonts/
        templates/
    01-strategy/
      vision-2026.md
      okrs/
      fundraising/
        pitch-deck/
        data-room/
    02-product/
      roadmap.md
      rfcs/
      user-research/
      competitive-intel/
    03-marketing/
      content/
        blog/
        newsletter/
      social/
        calendar.md
        templates/
      press/
        press-kit/
        media-list.md
      campaigns/
      launch-plans/
    04-growth/
      analytics/
      outreach/
      community/
      partnerships/
    05-operations/
      processes/
      tools.md
      vendors/
      hiring/
        roles/
        interview-guides/
    06-legal/
      licenses/
      contracts/
      privacy-policy.md
      terms-of-service.md
    07-finance/
      budget.md
      runway.md
      invoices/

  docs/                                # Cross-project technical docs
```

### Why This Works

1. **Code and business live together** -- one `git pull` gives you the entire company
2. **Numbered prefixes** (00-07) enforce visual ordering without Johnny Decimal overhead
3. **Foundation is immutable** -- mission/values/brand change rarely, sit at 00
4. **Strategy drives everything** -- 01 feeds into 02 (product) which feeds into 03 (marketing)
5. **Temporal flow** -- the numbers roughly follow the product lifecycle (foundation -> strategy -> build -> market -> grow -> operate)
6. **Sensitive files** -- `.gitignore` actual financials, use templates only in repo
7. **Scales from 2 to 20** -- add subfolders per zone as team grows, never restructure top level

---

## Sources

1. HN: Internal Documentation -- https://news.ycombinator.com/item?id=41415619
2. HN: Organizing Software Docs -- https://news.ycombinator.com/item?id=39370226
3. HN: Team Tools for Startups -- https://news.ycombinator.com/item?id=36386008
4. GitLab Handbook -- https://handbook.gitlab.com
5. YC Startup Library: Mechanics -- https://www.ycombinator.com/library/JR-how-to-start-a-startup-startup-mechanics
6. Basecamp Shape Up -- https://basecamp.com/shapeup
7. Pixar Production Pipeline -- https://sciencebehindpixar.org/pipeline/animation
8. Pixar 9 Stages -- https://www.viajaconapina.com/en/post/the-science-of-pixar-what-the-9-stages-of-animated-film-production-look-like
9. Apple ANPP Process -- via AMT Lab, multiple sources on Apple product development
10. CNCF: OSS Project Structure -- https://www.cncf.io/blog/2023/04/03/outlining-the-structure-of-your-open-source-software-project/
11. RFC Processes (Uber, Google, HashiCorp, Sourcegraph, SoundCloud) -- via Pragmatic Engineer
12. PARA Method -- Tiago Forte, Building a Second Brain
13. Johnny Decimal System -- https://johnnydecimal.com
14. Open Source Repos (cal.com, documenso, twenty, hoppscotch, infisical) -- GitHub

## Methodology

- Tools used: Perplexity API (sonar model), 8 parallel searches
- Pages analyzed: 30+ (HN threads, company handbooks, blog posts, production docs)
- Frameworks compared: PARA, Johnny Decimal, GitLab Handbook, Shape Up, Pixar Pipeline, Apple ANPP
