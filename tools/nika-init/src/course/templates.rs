// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Production-ready workflow templates — copy, configure, run.
//!
//! 10 real-world templates that solve actual problems. Each uses `inputs:` for
//! customization and `artifacts:` for file output. Users copy them, set their
//! provider, and run immediately.
//!
//! These are NOT exercises. They are working automation recipes.

/// A production-ready workflow template
pub struct Template {
    /// Filename (e.g., "daily-standup-report.nika.yaml")
    pub filename: &'static str,
    /// Human-readable name
    pub name: &'static str,
    /// One-line description
    pub description: &'static str,
    /// Category for grouping
    pub category: &'static str,
    /// Complete YAML content (uses {{PROVIDER}} and {{MODEL}} placeholders)
    pub content: &'static str,
}

/// All 10 production-ready templates
pub static TEMPLATES: &[Template] = &[
    Template {
        filename: "daily-standup-report.nika.yaml",
        name: "Daily Standup Report",
        description: "Generates daily standup report from git commits and calendar notes",
        category: "devops",
        content: DAILY_STANDUP_REPORT,
    },
    Template {
        filename: "pr-review-helper.nika.yaml",
        name: "PR Review Helper",
        description: "Analyzes a PR diff and provides structured code review feedback",
        category: "devops",
        content: PR_REVIEW_HELPER,
    },
    Template {
        filename: "changelog-generator.nika.yaml",
        name: "Changelog Generator",
        description: "Generates a changelog from git commits since the last tag",
        category: "devops",
        content: CHANGELOG_GENERATOR,
    },
    Template {
        filename: "seo-audit.nika.yaml",
        name: "SEO Audit",
        description: "Full SEO audit of a URL: metadata, links, performance, recommendations",
        category: "marketing",
        content: SEO_AUDIT,
    },
    Template {
        filename: "content-brief.nika.yaml",
        name: "Content Brief",
        description: "Generates a content brief from a keyword: research, outline, guidelines",
        category: "marketing",
        content: CONTENT_BRIEF,
    },
    Template {
        filename: "api-monitor.nika.yaml",
        name: "API Monitor",
        description: "Monitors multiple API endpoints and generates a status report",
        category: "devops",
        content: API_MONITOR,
    },
    Template {
        filename: "batch-translator.nika.yaml",
        name: "Batch Translator",
        description: "Translates content to multiple languages with quality verification",
        category: "content",
        content: BATCH_TRANSLATOR,
    },
    Template {
        filename: "meeting-summarizer.nika.yaml",
        name: "Meeting Summarizer",
        description: "Summarizes a meeting transcript into decisions, action items, and notes",
        category: "productivity",
        content: MEETING_SUMMARIZER,
    },
    Template {
        filename: "competitive-intel.nika.yaml",
        name: "Competitive Intel",
        description: "Scrapes and analyzes competitor websites for positioning insights",
        category: "marketing",
        content: COMPETITIVE_INTEL,
    },
    Template {
        filename: "knowledge-base-qa.nika.yaml",
        name: "Knowledge Base QA",
        description: "Generates FAQ entries from documentation pages",
        category: "content",
        content: KNOWLEDGE_BASE_QA,
    },
];

/// Return all templates as a Vec (for integration with init system)
pub fn get_templates() -> Vec<&'static Template> {
    TEMPLATES.iter().collect()
}

// =============================================================================
// 01: DAILY STANDUP REPORT
// =============================================================================

const DAILY_STANDUP_REPORT: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Daily Standup Report
# ═══════════════════════════════════════════════════════════════════
#
# Generates a structured standup report by collecting yesterday's
# git commits, today's calendar notes, and formatting them into
# a team-ready summary with blockers and priorities.
#
# USAGE:
#   nika run daily-standup-report.nika.yaml
#   nika run daily-standup-report.nika.yaml --input author="Jane Doe"
#   nika run daily-standup-report.nika.yaml --input days=3
#
# REQUIRES: Git repository, LLM provider
# SETUP:    nika keys set anthropic  (or: openai, mistral, groq)

schema: "nika/workflow@0.12"

workflow: daily-standup-report
description: "Generate a daily standup report from git history"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  author: ""
  days: 1
  include_stats: true

artifacts:
  dir: ./output/standup
  format: text
  manifest: true

tasks:
  # ── Step 1: Collect git commits from the last N days ─────────────
  - id: git_log
    exec:
      command: |
        if [ -n "{{inputs.author}}" ]; then
          git log --since="{{inputs.days}} days ago" --author="{{inputs.author}}" --pretty=format:"- %s (%h, %ar)" --no-merges
        else
          git log --since="{{inputs.days}} days ago" --pretty=format:"- %s (%h, %ar)" --no-merges
        fi
      timeout: 10

  # ── Step 2: Collect diff stats for context ───────────────────────
  - id: git_stats
    exec:
      command: |
        git diff --stat HEAD~10 HEAD 2>/dev/null | tail -5 || echo "No diff stats available"
      timeout: 10

  # ── Step 3: Check for any stale branches ─────────────────────────
  - id: stale_branches
    exec:
      command: |
        git branch --merged main 2>/dev/null | grep -v "main\|master\|\*" | head -5 || echo "None"
      timeout: 10

  # ── Step 4: Generate the standup report ──────────────────────────
  - id: report
    depends_on: [git_log, git_stats, stale_branches]
    with:
      commits: $git_log
      stats: $git_stats
      branches: $stale_branches
    infer:
      prompt: |
        Generate a daily standup report from this data.

        GIT COMMITS (last {{inputs.days}} day(s)):
        {{with.commits}}

        DIFF STATS:
        {{with.stats}}

        STALE BRANCHES (merged but not deleted):
        {{with.branches}}

        Format the report as:

        ## Standup Report — [today's date]

        ### Done (Yesterday)
        [Group commits by theme: features, fixes, chores. Be concise.]

        ### In Progress (Today)
        [Infer current work from recent commit patterns.]

        ### Blockers
        [Flag any potential issues: stale branches, large diffs, etc.]

        ### Stats
        [Summarize the diff stats in one line.]

        Keep it concise and actionable. No filler text.
      temperature: 0.3
      max_tokens: 800
    artifact:
      path: ./output/standup/standup-report.md
"##;

// =============================================================================
// 02: PR REVIEW HELPER
// =============================================================================

const PR_REVIEW_HELPER: &str = r##"# ═══════════════════════════════════════════════════════════════════
# PR Review Helper
# ═══════════════════════════════════════════════════════════════════
#
# Analyzes a git diff (or branch comparison) and produces structured
# code review feedback: summary, concerns, suggestions, and a
# review verdict.
#
# USAGE:
#   nika run pr-review-helper.nika.yaml
#   nika run pr-review-helper.nika.yaml --input base=main --input head=feature/auth
#   nika run pr-review-helper.nika.yaml --input focus="security"
#
# REQUIRES: Git repository, LLM provider
# SETUP:    nika keys set anthropic

schema: "nika/workflow@0.12"

workflow: pr-review-helper
description: "Analyze a PR diff and provide structured code review"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  base: "main"
  head: "HEAD"
  focus: "general"

artifacts:
  dir: ./output/reviews
  format: text
  manifest: true

tasks:
  # ── Step 1: Get the diff ─────────────────────────────────────────
  - id: diff
    exec:
      command: |
        git diff {{inputs.base}}...{{inputs.head}} -- . ':!*.lock' ':!package-lock.json' | head -3000
      timeout: 15

  # ── Step 2: Get commit messages for context ──────────────────────
  - id: commits
    exec:
      command: |
        git log {{inputs.base}}...{{inputs.head}} --pretty=format:"- %s%n  %b" --no-merges | head -50
      timeout: 10

  # ── Step 3: Get file list with stats ─────────────────────────────
  - id: file_stats
    exec:
      command: |
        git diff {{inputs.base}}...{{inputs.head}} --stat | tail -20
      timeout: 10

  # ── Step 4: Analyze the diff ─────────────────────────────────────
  - id: review
    depends_on: [diff, commits, file_stats]
    with:
      code_diff: $diff
      commit_msgs: $commits
      stats: $file_stats
    infer:
      system: |
        You are a senior code reviewer. You are thorough, constructive,
        and specific. You cite line numbers and file paths. You distinguish
        between blocking issues and suggestions. Review focus: {{inputs.focus}}.
      prompt: |
        Review this pull request.

        COMMITS:
        {{with.commit_msgs}}

        FILE STATS:
        {{with.stats}}

        DIFF:
        ```
        {{with.code_diff}}
        ```

        Provide your review in this format:

        ## PR Review

        ### Summary
        [1-2 sentence summary of what this PR does]

        ### Changes Overview
        | File | Change Type | Risk |
        |------|-------------|------|
        [Table of significant files changed]

        ### Issues (Blocking)
        [Numbered list of issues that MUST be fixed before merge. Be specific.]

        ### Suggestions (Non-blocking)
        [Numbered list of improvements. Nice-to-have, not required.]

        ### Security & Performance
        [Any security or performance concerns. Say "None identified" if clean.]

        ### Verdict
        [APPROVE / REQUEST_CHANGES / NEEDS_DISCUSSION — with one-line reason]
      temperature: 0.2
      max_tokens: 1500
    artifact:
      path: ./output/reviews/pr-review.md
"##;

// =============================================================================
// 03: CHANGELOG GENERATOR
// =============================================================================

const CHANGELOG_GENERATOR: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Changelog Generator
# ═══════════════════════════════════════════════════════════════════
#
# Generates a polished changelog from git commits since the last
# tag. Groups commits by type (features, fixes, chores), links
# authors, and produces Markdown ready for a release.
#
# USAGE:
#   nika run changelog-generator.nika.yaml
#   nika run changelog-generator.nika.yaml --input since=v0.35.0
#   nika run changelog-generator.nika.yaml --input repo_url="https://github.com/org/repo"
#
# REQUIRES: Git repository, LLM provider
# SETUP:    nika keys set anthropic

schema: "nika/workflow@0.12"

workflow: changelog-generator
description: "Generate a release changelog from git commits"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  since: ""
  repo_url: ""

artifacts:
  dir: ./output/changelog
  format: text
  manifest: true

tasks:
  # ── Step 1: Determine the last tag ──────────────────────────────
  - id: last_tag
    exec:
      command: |
        if [ -n "{{inputs.since}}" ]; then
          echo "{{inputs.since}}"
        else
          git describe --tags --abbrev=0 2>/dev/null || echo ""
        fi
      timeout: 10

  # ── Step 2: Get commits since last tag ──────────────────────────
  - id: raw_commits
    depends_on: [last_tag]
    with:
      tag: $last_tag
    exec:
      command: |
        if [ -n "{{with.tag}}" ]; then
          git log "{{with.tag}}"..HEAD --pretty=format:"%h|%s|%an|%ad" --date=short --no-merges
        else
          git log --pretty=format:"%h|%s|%an|%ad" --date=short --no-merges -50
        fi
      timeout: 10

  # ── Step 3: Get diff stats ──────────────────────────────────────
  - id: diff_stats
    depends_on: [last_tag]
    with:
      tag: $last_tag
    exec:
      command: |
        if [ -n "{{with.tag}}" ]; then
          git diff "{{with.tag}}"..HEAD --shortstat
        else
          git diff HEAD~50..HEAD --shortstat
        fi
      timeout: 10

  # ── Step 4: Generate the changelog ──────────────────────────────
  - id: changelog
    depends_on: [raw_commits, diff_stats, last_tag]
    with:
      commits: $raw_commits
      stats: $diff_stats
      tag: $last_tag
    infer:
      prompt: |
        Generate a professional changelog from these git commits.

        Previous tag: {{with.tag}}
        Repository URL: {{inputs.repo_url}}

        RAW COMMITS (format: hash|subject|author|date):
        {{with.commits}}

        DIFF STATS:
        {{with.stats}}

        Format as Markdown:

        # Changelog

        ## [Next Release] — [today's date]

        [One-paragraph summary of this release.]

        ### Features
        [List new features. Use commit subjects. Prefix with commit hash.]

        ### Bug Fixes
        [List fixes.]

        ### Maintenance
        [List chores, refactors, dependency updates, docs.]

        ### Contributors
        [Unique authors from the commits.]

        ### Stats
        [Files changed, insertions, deletions — from diff stats.]

        ---

        Rules:
        - Group by type using conventional commit prefixes (feat, fix, chore, etc.)
        - If a commit has no prefix, infer the category from the subject
        - Each entry should be one line with the commit hash in parentheses
        - Skip merge commits
        - If repo_url is provided, link commit hashes to the repo
      temperature: 0.2
      max_tokens: 1500
    artifact:
      path: ./output/changelog/CHANGELOG.md
"##;

// =============================================================================
// 04: SEO AUDIT
// =============================================================================

const SEO_AUDIT: &str = r##"# ═══════════════════════════════════════════════════════════════════
# SEO Audit
# ═══════════════════════════════════════════════════════════════════
#
# Performs a comprehensive SEO audit of a URL by extracting metadata,
# analyzing link structure, checking content, and producing an
# actionable report with scores and recommendations.
#
# USAGE:
#   nika run seo-audit.nika.yaml --input url="https://example.com"
#
# REQUIRES: LLM provider
# SETUP:    nika keys set anthropic
# FEATURES: fetch-html, fetch-markdown (for extract: metadata, links, article)

schema: "nika/workflow@0.12"

workflow: seo-audit
description: "Full SEO audit of a URL with actionable recommendations"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  url: "https://example.com"

artifacts:
  dir: ./output/seo
  format: text
  manifest: true

tasks:
  # ── Step 1: Extract metadata (OG tags, Twitter Cards, JSON-LD) ──
  - id: metadata
    fetch:
      url: "{{inputs.url}}"
      extract: metadata
      timeout: 20

  # ── Step 2: Extract and classify all links ──────────────────────
  - id: links
    fetch:
      url: "{{inputs.url}}"
      extract: links
      timeout: 20

  # ── Step 3: Extract page content as article ─────────────────────
  - id: content
    fetch:
      url: "{{inputs.url}}"
      extract: article
      timeout: 20

  # ── Step 4: Get full HTTP response for technical checks ─────────
  - id: headers
    fetch:
      url: "{{inputs.url}}"
      response: full
      timeout: 15

  # ── Step 5: Check robots.txt ────────────────────────────────────
  - id: robots
    fetch:
      url: "{{inputs.url}}/robots.txt"
      method: GET
      timeout: 10

  # ── Step 6: Analyze everything and produce the report ───────────
  - id: report
    depends_on: [metadata, links, content, headers, robots]
    with:
      meta: $metadata
      link_data: $links
      page_content: $content
      http_response: $headers
      robots_txt: $robots
    infer:
      system: |
        You are an expert SEO auditor. You produce actionable, data-driven
        reports. Score each category 0-100. Be specific about what to fix.
      prompt: |
        Perform a full SEO audit of {{inputs.url}}.

        METADATA (OG, Twitter Cards, JSON-LD, SEO tags):
        {{with.meta}}

        LINK ANALYSIS (internal/external, nav/content/footer):
        {{with.link_data}}

        PAGE CONTENT (article text):
        {{with.page_content}}

        HTTP HEADERS (status, headers, final URL):
        {{with.http_response}}

        ROBOTS.TXT:
        {{with.robots_txt}}

        Generate this report:

        # SEO Audit — {{inputs.url}}

        ## Overall Score: [X/100]

        ## 1. Technical SEO [X/100]
        - HTTP Status & Redirects
        - Response Headers (cache, security)
        - Robots.txt analysis
        - Canonical URL

        ## 2. On-Page SEO [X/100]
        - Title tag (length, keywords)
        - Meta description (length, quality)
        - Heading structure (H1, H2, H3)
        - Content length and quality

        ## 3. Social & Structured Data [X/100]
        - Open Graph tags (completeness)
        - Twitter Card tags
        - JSON-LD / Schema.org markup

        ## 4. Link Profile [X/100]
        - Internal link count and structure
        - External link count
        - Broken link indicators
        - Navigation vs content links

        ## 5. Priority Fixes
        [Numbered list of the top 5 most impactful fixes, ordered by priority]

        ## 6. Quick Wins
        [3-5 easy improvements that can be done in under an hour]
      temperature: 0.2
      max_tokens: 2000
    artifact:
      path: ./output/seo/seo-audit.md
"##;

// =============================================================================
// 05: CONTENT BRIEF
// =============================================================================

const CONTENT_BRIEF: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Content Brief Generator
# ═══════════════════════════════════════════════════════════════════
#
# Generates a comprehensive content brief from a target keyword.
# Researches the topic via web search, analyzes top results,
# and produces a structured brief with outline, guidelines,
# and SEO recommendations.
#
# USAGE:
#   nika run content-brief.nika.yaml --input keyword="workflow automation"
#   nika run content-brief.nika.yaml --input keyword="rust async" --input audience="senior devs"
#
# REQUIRES: LLM provider
# SETUP:    nika keys set anthropic
# FEATURES: fetch-markdown (for extract: article)

schema: "nika/workflow@0.12"

workflow: content-brief
description: "Generate a content brief from keyword research"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  keyword: "workflow automation"
  audience: "technical professionals"
  word_count: 2000
  tone: "authoritative but approachable"

artifacts:
  dir: ./output/briefs
  format: text
  manifest: true

tasks:
  # ── Step 1: Research the keyword via a search API ───────────────
  - id: search_results
    fetch:
      url: "https://en.wikipedia.org/w/api.php?action=query&list=search&srsearch={{inputs.keyword}}&format=json&srlimit=5"
      method: GET
      timeout: 15

  # ── Step 2: Fetch a reference article for depth ─────────────────
  - id: reference_article
    fetch:
      url: "https://en.wikipedia.org/w/api.php?action=query&titles={{inputs.keyword}}&prop=extracts&exintro=true&format=json&explaintext=true"
      method: GET
      timeout: 15

  # ── Step 3: Generate the content brief ──────────────────────────
  - id: brief
    depends_on: [search_results, reference_article]
    with:
      search: $search_results
      reference: $reference_article
    infer:
      system: |
        You are a senior content strategist who creates detailed briefs
        for writers. Your briefs are actionable, SEO-aware, and produce
        high-ranking content consistently.
      prompt: |
        Create a comprehensive content brief for the keyword: "{{inputs.keyword}}"

        TARGET AUDIENCE: {{inputs.audience}}
        TARGET LENGTH: {{inputs.word_count}} words
        TONE: {{inputs.tone}}

        SEARCH DATA:
        {{with.search}}

        REFERENCE CONTENT:
        {{with.reference}}

        Generate this brief:

        # Content Brief — "{{inputs.keyword}}"

        ## Overview
        - Primary keyword: {{inputs.keyword}}
        - Target audience: {{inputs.audience}}
        - Word count: {{inputs.word_count}}
        - Tone: {{inputs.tone}}

        ## Search Intent
        [What is the user trying to accomplish? Informational / transactional / navigational]

        ## Key Topics to Cover
        [Bulleted list of 8-12 subtopics that MUST be addressed]

        ## Suggested Outline
        ### H1: [Title suggestion with keyword]
        ### H2: [Section 1]
        - Key points
        ### H2: [Section 2]
        - Key points
        [Continue for 5-7 sections]

        ## SEO Guidelines
        - Title tag: [suggestion, max 60 chars]
        - Meta description: [suggestion, max 155 chars]
        - URL slug: [suggestion]
        - Internal linking opportunities: [3-5 suggestions]

        ## Content Guidelines
        - Opening hook: [specific suggestion]
        - Key statistics to include: [3-5 data points]
        - Competitor gaps: [what existing content misses]
        - Call to action: [specific CTA suggestion]

        ## Do NOT Include
        [3-5 things to explicitly avoid]
      temperature: 0.4
      max_tokens: 2000
    artifact:
      path: ./output/briefs/content-brief.md

  # ── Step 4: Generate title variations ───────────────────────────
  - id: titles
    depends_on: [brief]
    with:
      brief_content: $brief
    infer:
      prompt: |
        Based on this content brief, generate 10 title variations for the keyword "{{inputs.keyword}}".

        Brief context:
        {{with.brief_content}}

        For each title:
        1. Keep under 60 characters
        2. Include the primary keyword
        3. Use power words where appropriate

        Format as a numbered list. Include the character count in brackets after each.
      temperature: 0.7
      max_tokens: 400
    artifact:
      path: ./output/briefs/title-variations.md
"##;

// =============================================================================
// 06: API MONITOR
// =============================================================================

const API_MONITOR: &str = r##"# ═══════════════════════════════════════════════════════════════════
# API Monitor
# ═══════════════════════════════════════════════════════════════════
#
# Monitors multiple API endpoints in parallel and generates a
# health status report. Checks HTTP status, response time,
# and response structure. Outputs a Markdown dashboard.
#
# USAGE:
#   nika run api-monitor.nika.yaml
#   nika run api-monitor.nika.yaml --input timeout=5
#
# REQUIRES: LLM provider (for summary). No API keys for the default endpoints.
# SETUP:    nika keys set anthropic
#
# TIP: Edit the fetch URLs below to monitor your own APIs.

schema: "nika/workflow@0.12"

workflow: api-monitor
description: "Monitor multiple API endpoints and report status"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  report_title: "API Health Report"

artifacts:
  dir: ./output/monitoring
  format: text
  manifest: true

tasks:
  # ── Endpoint 1: httpbin (baseline) ───────────────────────────────
  - id: check_httpbin
    fetch:
      url: "https://httpbin.org/get"
      method: GET
      response: full
      timeout: 10

  # ── Endpoint 2: JSONPlaceholder ──────────────────────────────────
  - id: check_jsonplaceholder
    fetch:
      url: "https://jsonplaceholder.typicode.com/posts/1"
      method: GET
      response: full
      timeout: 10

  # ── Endpoint 3: GitHub API ───────────────────────────────────────
  - id: check_github
    fetch:
      url: "https://api.github.com/zen"
      method: GET
      response: full
      timeout: 10

  # ── Endpoint 4: WorldTimeAPI ─────────────────────────────────────
  - id: check_worldtime
    fetch:
      url: "https://worldtimeapi.org/api/timezone/Etc/UTC"
      method: GET
      response: full
      timeout: 10

  # ── Log the monitoring run ───────────────────────────────────────
  - id: timestamp
    exec:
      command: date -u '+%Y-%m-%d %H:%M:%S UTC'
      timeout: 5

  # ── Generate status report ───────────────────────────────────────
  - id: report
    depends_on: [check_httpbin, check_jsonplaceholder, check_github, check_worldtime, timestamp]
    with:
      httpbin: $check_httpbin
      jsonplaceholder: $check_jsonplaceholder
      github: $check_github
      worldtime: $check_worldtime
      run_time: $timestamp
    infer:
      prompt: |
        Generate an API health monitoring report from these endpoint checks.
        Run timestamp: {{with.run_time}}

        ENDPOINT 1 — httpbin.org/get:
        {{with.httpbin}}

        ENDPOINT 2 — jsonplaceholder.typicode.com/posts/1:
        {{with.jsonplaceholder}}

        ENDPOINT 3 — api.github.com/zen:
        {{with.github}}

        ENDPOINT 4 — worldtimeapi.org/api/timezone/Etc/UTC:
        {{with.worldtime}}

        Format as:

        # {{inputs.report_title}} — {{with.run_time}}

        ## Status Dashboard

        | Endpoint | Status | Code | Response |
        |----------|--------|------|----------|
        [For each endpoint: name, UP/DOWN/DEGRADED, HTTP status code, brief note]

        ## Overall Health: [ALL GREEN / DEGRADED / OUTAGE]

        ## Details
        [For each endpoint that is not 200, explain what happened]

        ## Recommendations
        [Any actions needed based on the results]

        Keep it concise. This is an automated monitoring report.
      temperature: 0.1
      max_tokens: 800
    artifact:
      path: ./output/monitoring/api-status.md
"##;

// =============================================================================
// 07: BATCH TRANSLATOR
// =============================================================================

const BATCH_TRANSLATOR: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Batch Translator
# ═══════════════════════════════════════════════════════════════════
#
# Translates content to multiple languages in parallel, then runs
# a quality verification pass comparing each translation back
# against the original. Outputs one file per language.
#
# USAGE:
#   nika run batch-translator.nika.yaml
#   nika run batch-translator.nika.yaml --input content="Your text here"
#
# REQUIRES: LLM provider
# SETUP:    nika keys set anthropic

schema: "nika/workflow@0.12"

workflow: batch-translator
description: "Translate content to multiple languages with quality check"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  content: |
    Nika is a semantic workflow engine for AI tasks. It uses a declarative
    YAML syntax with five core verbs: infer, exec, fetch, invoke, and agent.
    Workflows are validated at parse time, executed as DAGs, and produce
    traceable, reproducible results. No code required.

artifacts:
  dir: ./output/translations
  format: text
  manifest: true

tasks:
  # ── Step 1: Translate to French ──────────────────────────────────
  - id: translate_fr
    infer:
      system: |
        You are a professional translator. Translate accurately while
        preserving technical terms, tone, and formatting. Do not add
        explanations. Return ONLY the translation.
      prompt: |
        Translate the following text to French (fr-FR):

        {{inputs.content}}
      temperature: 0.2
      max_tokens: 1000
    artifact:
      path: ./output/translations/fr.md

  # ── Step 2: Translate to Spanish ─────────────────────────────────
  - id: translate_es
    infer:
      system: |
        You are a professional translator. Translate accurately while
        preserving technical terms, tone, and formatting. Do not add
        explanations. Return ONLY the translation.
      prompt: |
        Translate the following text to Spanish (es-ES):

        {{inputs.content}}
      temperature: 0.2
      max_tokens: 1000
    artifact:
      path: ./output/translations/es.md

  # ── Step 3: Translate to German ──────────────────────────────────
  - id: translate_de
    infer:
      system: |
        You are a professional translator. Translate accurately while
        preserving technical terms, tone, and formatting. Do not add
        explanations. Return ONLY the translation.
      prompt: |
        Translate the following text to German (de-DE):

        {{inputs.content}}
      temperature: 0.2
      max_tokens: 1000
    artifact:
      path: ./output/translations/de.md

  # ── Step 4: Translate to Japanese ────────────────────────────────
  - id: translate_ja
    infer:
      system: |
        You are a professional translator. Translate accurately while
        preserving technical terms, tone, and formatting. Do not add
        explanations. Return ONLY the translation.
      prompt: |
        Translate the following text to Japanese (ja-JP):

        {{inputs.content}}
      temperature: 0.2
      max_tokens: 1000
    artifact:
      path: ./output/translations/ja.md

  # ── Step 5: Quality verification ─────────────────────────────────
  - id: quality_check
    depends_on: [translate_fr, translate_es, translate_de, translate_ja]
    with:
      french: $translate_fr
      spanish: $translate_es
      german: $translate_de
      japanese: $translate_ja
    infer:
      system: |
        You are a multilingual quality assurance specialist. Compare
        translations against the original for accuracy, completeness,
        and naturalness. Be specific about issues.
      prompt: |
        Verify these translations against the original text.

        ORIGINAL (English):
        {{inputs.content}}

        FRENCH:
        {{with.french}}

        SPANISH:
        {{with.spanish}}

        GERMAN:
        {{with.german}}

        JAPANESE:
        {{with.japanese}}

        For each language, report:

        # Translation Quality Report

        ## French [X/10]
        - Accuracy: [brief assessment]
        - Naturalness: [brief assessment]
        - Issues: [list any problems, or "None"]

        ## Spanish [X/10]
        [same format]

        ## German [X/10]
        [same format]

        ## Japanese [X/10]
        [same format]

        ## Overall Score: [average/10]
        ## Summary: [one sentence]
      temperature: 0.2
      max_tokens: 1000
    artifact:
      path: ./output/translations/quality-report.md
"##;

// =============================================================================
// 08: MEETING SUMMARIZER
// =============================================================================

const MEETING_SUMMARIZER: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Meeting Summarizer
# ═══════════════════════════════════════════════════════════════════
#
# Takes a meeting transcript (pasted as input or from a file) and
# produces a structured summary with decisions, action items,
# open questions, and a follow-up email draft.
#
# USAGE:
#   nika run meeting-summarizer.nika.yaml --input transcript="[paste transcript]"
#   nika run meeting-summarizer.nika.yaml --input meeting_title="Sprint Planning"
#
# REQUIRES: LLM provider
# SETUP:    nika keys set anthropic

schema: "nika/workflow@0.12"

workflow: meeting-summarizer
description: "Summarize meeting transcript into decisions and action items"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  transcript: |
    Alice: Let's kick off the sprint planning. We have 3 main items today.
    Bob: First, the authentication refactor. I've been working on the OAuth2
    integration and it's about 70% done. Should be ready by Wednesday.
    Alice: Good. Any blockers?
    Bob: I need the API keys for the staging environment. Carol, can you
    provision those today?
    Carol: Sure, I'll have them by end of day. Also, I wanted to bring up
    the database migration. We need to decide if we're doing it this sprint.
    Alice: What's the scope?
    Carol: It's the PostgreSQL 15 upgrade. About 2 days of work, plus
    testing. Risk is moderate — we need to coordinate with the ops team.
    Alice: Let's push it to next sprint. We already have a full plate.
    Bob: Agreed. Third item — the customer dashboard. The designs are
    finalized. I think we can start frontend implementation.
    Carol: I can take that. I'll need the Figma link from the design team.
    Alice: I'll send it after this meeting. Let's aim to have a working
    prototype by Friday. Any other business?
    Bob: Just a reminder — code freeze is next Wednesday for the release.
    Alice: Right. Let's wrap up. Good meeting everyone.
  meeting_title: "Sprint Planning"
  attendees: ""

artifacts:
  dir: ./output/meetings
  format: text
  manifest: true

tasks:
  # ── Step 1: Extract structured information ───────────────────────
  - id: extract
    infer:
      system: |
        You are an expert meeting analyst. Extract structured information
        from transcripts with precision. Be exhaustive — miss nothing.
        If something is ambiguous, flag it as an open question.
      prompt: |
        Analyze this meeting transcript and extract all structured information.

        Meeting: {{inputs.meeting_title}}
        Attendees: {{inputs.attendees}}

        TRANSCRIPT:
        {{inputs.transcript}}

        Extract and format as:

        # Meeting Summary — {{inputs.meeting_title}}

        ## Key Decisions
        [Numbered list of every decision made, with who decided]

        ## Action Items
        | # | Action | Owner | Deadline | Status |
        |---|--------|-------|----------|--------|
        [Every commitment, task, or follow-up mentioned]

        ## Discussion Topics
        [Brief summary of each topic discussed]

        ## Open Questions
        [Anything unresolved or needing follow-up]

        ## Deferred Items
        [Anything explicitly postponed, with reason]

        ## Key Dates
        [Any dates or deadlines mentioned]
      temperature: 0.1
      max_tokens: 1200
    artifact:
      path: ./output/meetings/summary.md

  # ── Step 2: Generate follow-up email ─────────────────────────────
  - id: email
    depends_on: [extract]
    with:
      summary: $extract
    infer:
      prompt: |
        Write a professional follow-up email based on this meeting summary.

        {{with.summary}}

        The email should:
        - Have a clear subject line
        - Start with "Hi team,"
        - Briefly summarize decisions (2-3 sentences)
        - List action items with owners and deadlines
        - Mention deferred items
        - End with next steps
        - Be concise (under 200 words)

        Format as:

        **Subject:** [subject line]

        **Body:**
        [email body]
      temperature: 0.3
      max_tokens: 500
    artifact:
      path: ./output/meetings/follow-up-email.md
"##;

// =============================================================================
// 09: COMPETITIVE INTEL
// =============================================================================

const COMPETITIVE_INTEL: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Competitive Intelligence
# ═══════════════════════════════════════════════════════════════════
#
# Scrapes competitor websites in parallel, extracts metadata
# and content, then produces a competitive analysis report
# with positioning insights.
#
# USAGE:
#   nika run competitive-intel.nika.yaml --input competitor_1="https://competitor.com"
#   nika run competitive-intel.nika.yaml --input company_name="Acme Inc"
#
# REQUIRES: LLM provider
# SETUP:    nika keys set anthropic
# FEATURES: fetch-html, fetch-article (for extract: metadata, article)

schema: "nika/workflow@0.12"

workflow: competitive-intel
description: "Scrape and analyze competitor websites for positioning insights"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  competitor_1: "https://linear.app"
  competitor_2: "https://shortcut.com"
  competitor_3: "https://asana.com"
  company_name: "Your Company"

artifacts:
  dir: ./output/intel
  format: text
  manifest: true

tasks:
  # ── Scrape competitor 1 ──────────────────────────────────────────
  - id: scrape_1_meta
    fetch:
      url: "{{inputs.competitor_1}}"
      extract: metadata
      timeout: 20

  - id: scrape_1_content
    fetch:
      url: "{{inputs.competitor_1}}"
      extract: article
      timeout: 20

  # ── Scrape competitor 2 ──────────────────────────────────────────
  - id: scrape_2_meta
    fetch:
      url: "{{inputs.competitor_2}}"
      extract: metadata
      timeout: 20

  - id: scrape_2_content
    fetch:
      url: "{{inputs.competitor_2}}"
      extract: article
      timeout: 20

  # ── Scrape competitor 3 ──────────────────────────────────────────
  - id: scrape_3_meta
    fetch:
      url: "{{inputs.competitor_3}}"
      extract: metadata
      timeout: 20

  - id: scrape_3_content
    fetch:
      url: "{{inputs.competitor_3}}"
      extract: article
      timeout: 20

  # ── Analyze all competitors ──────────────────────────────────────
  - id: analysis
    depends_on: [scrape_1_meta, scrape_1_content, scrape_2_meta, scrape_2_content, scrape_3_meta, scrape_3_content]
    with:
      c1_meta: $scrape_1_meta
      c1_content: $scrape_1_content
      c2_meta: $scrape_2_meta
      c2_content: $scrape_2_content
      c3_meta: $scrape_3_meta
      c3_content: $scrape_3_content
    infer:
      system: |
        You are a competitive intelligence analyst. You identify patterns
        in positioning, messaging, and feature emphasis. Be specific and
        cite evidence from the scraped content.
      prompt: |
        Analyze these three competitors for {{inputs.company_name}}.

        COMPETITOR 1: {{inputs.competitor_1}}
        Metadata: {{with.c1_meta}}
        Content: {{with.c1_content}}

        COMPETITOR 2: {{inputs.competitor_2}}
        Metadata: {{with.c2_meta}}
        Content: {{with.c2_content}}

        COMPETITOR 3: {{inputs.competitor_3}}
        Metadata: {{with.c3_meta}}
        Content: {{with.c3_content}}

        Generate this report:

        # Competitive Intelligence Report — {{inputs.company_name}}

        ## Competitor Overview

        | Company | Tagline | Target Audience | Key Differentiator |
        |---------|---------|-----------------|-------------------|
        [One row per competitor]

        ## Messaging Analysis
        [How each competitor positions themselves. What language do they use?
         What pain points do they address? What emotions do they target?]

        ## Feature Comparison
        | Feature / Capability | Competitor 1 | Competitor 2 | Competitor 3 |
        |---------------------|--------------|--------------|--------------|
        [Compare mentioned features/capabilities]

        ## SEO & Content Strategy
        [What keywords are they targeting? Content themes?
         Meta description analysis. OG tags quality.]

        ## Gaps & Opportunities
        [What are competitors NOT saying? What audience segments are
         underserved? Where can {{inputs.company_name}} differentiate?]

        ## Recommended Positioning
        [Specific positioning recommendations for {{inputs.company_name}}
         based on the competitive landscape.]
      temperature: 0.3
      max_tokens: 2000
    artifact:
      path: ./output/intel/competitive-analysis.md
"##;

// =============================================================================
// 10: KNOWLEDGE BASE QA
// =============================================================================

const KNOWLEDGE_BASE_QA: &str = r##"# ═══════════════════════════════════════════════════════════════════
# Knowledge Base QA Generator
# ═══════════════════════════════════════════════════════════════════
#
# Fetches documentation pages, extracts content, and generates
# FAQ entries with clear questions and concise answers. Produces
# a structured FAQ document ready for a help center.
#
# USAGE:
#   nika run knowledge-base-qa.nika.yaml --input doc_url="https://docs.example.com"
#   nika run knowledge-base-qa.nika.yaml --input product_name="Nika"
#
# REQUIRES: LLM provider
# SETUP:    nika keys set anthropic
# FEATURES: fetch-markdown (for extract: markdown)

schema: "nika/workflow@0.12"

workflow: knowledge-base-qa
description: "Generate FAQ entries from documentation pages"

provider: "{{PROVIDER}}"
model: "{{MODEL}}"

inputs:
  doc_url: "https://docs.github.com/en/get-started/start-your-journey/about-github-and-git"
  product_name: "GitHub"
  faq_count: 15

artifacts:
  dir: ./output/faq
  format: text
  manifest: true

tasks:
  # ── Step 1: Fetch documentation as markdown ──────────────────────
  - id: doc_content
    fetch:
      url: "{{inputs.doc_url}}"
      extract: markdown
      timeout: 20

  # ── Step 2: Fetch metadata for context ───────────────────────────
  - id: doc_metadata
    fetch:
      url: "{{inputs.doc_url}}"
      extract: metadata
      timeout: 15

  # ── Step 3: Extract link structure for related topics ────────────
  - id: doc_links
    fetch:
      url: "{{inputs.doc_url}}"
      extract: links
      timeout: 15

  # ── Step 4: Generate FAQ entries ─────────────────────────────────
  - id: faq
    depends_on: [doc_content, doc_metadata, doc_links]
    with:
      content: $doc_content
      meta: $doc_metadata
      links: $doc_links
    infer:
      system: |
        You are a technical writer who creates clear, concise FAQ entries.
        Each answer should be self-contained — a reader should understand
        it without reading the source documentation. Use simple language.
        Include code examples where helpful.
      prompt: |
        Generate {{inputs.faq_count}} FAQ entries from this documentation.

        Product: {{inputs.product_name}}
        Source: {{inputs.doc_url}}

        PAGE METADATA:
        {{with.meta}}

        DOCUMENTATION CONTENT:
        {{with.content}}

        RELATED LINKS:
        {{with.links}}

        Format as:

        # {{inputs.product_name}} — Frequently Asked Questions

        > Generated from: {{inputs.doc_url}}

        ---

        ## General

        ### Q: [question in natural language]
        **A:** [clear, concise answer in 2-4 sentences. Include specifics.]

        ### Q: [next question]
        **A:** [answer]

        [Continue for {{inputs.faq_count}} questions total]

        ---

        ## Related Topics
        [List 3-5 related documentation links from the page]

        Rules:
        - Questions should cover: what, how, why, troubleshooting
        - Order from basic to advanced
        - Include at least 2 "How do I..." questions
        - Include at least 1 troubleshooting question
        - Answers must be factually grounded in the source content
        - If the doc mentions commands or code, include them in answers
      temperature: 0.3
      max_tokens: 2500
    artifact:
      path: ./output/faq/faq.md

  # ── Step 5: Generate a summary index ─────────────────────────────
  - id: index
    depends_on: [faq]
    with:
      faq_content: $faq
    infer:
      prompt: |
        Create a quick-reference index for this FAQ document.

        {{with.faq_content}}

        Format as a numbered list with just the questions (no answers).
        Group by category if patterns emerge. Add a one-line description
        of what the FAQ covers at the top.
      temperature: 0.2
      max_tokens: 400
    artifact:
      path: ./output/faq/index.md
"##;

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_count() {
        assert_eq!(TEMPLATES.len(), 10, "Should have exactly 10 templates");
    }

    #[test]
    fn test_template_filenames_unique() {
        let mut names: Vec<&str> = TEMPLATES.iter().map(|t| t.filename).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), len, "All template filenames must be unique");
    }

    #[test]
    fn test_template_filenames_format() {
        for t in TEMPLATES {
            assert!(
                t.filename.ends_with(".nika.yaml"),
                "Template {} must end with .nika.yaml",
                t.filename
            );
            assert!(
                !t.filename.contains(' '),
                "Template filename {} must not contain spaces",
                t.filename
            );
        }
    }

    #[test]
    fn test_templates_have_schema() {
        for t in TEMPLATES {
            assert!(
                t.content.contains("schema: \"nika/workflow@0.12\""),
                "Template {} must declare schema",
                t.filename
            );
        }
    }

    #[test]
    fn test_templates_have_workflow_name() {
        for t in TEMPLATES {
            assert!(
                t.content.contains("workflow:"),
                "Template {} must have workflow: declaration",
                t.filename
            );
        }
    }

    #[test]
    fn test_templates_have_tasks() {
        for t in TEMPLATES {
            assert!(
                t.content.contains("tasks:"),
                "Template {} must have tasks: section",
                t.filename
            );
        }
    }

    #[test]
    fn test_templates_have_inputs() {
        for t in TEMPLATES {
            assert!(
                t.content.contains("inputs:"),
                "Template {} must have inputs: for customization",
                t.filename
            );
        }
    }

    #[test]
    fn test_templates_have_artifacts() {
        for t in TEMPLATES {
            assert!(
                t.content.contains("artifact"),
                "Template {} must have artifact configuration",
                t.filename
            );
        }
    }

    #[test]
    fn test_templates_have_description() {
        for t in TEMPLATES {
            assert!(
                t.content.contains("description:"),
                "Template {} must have a description",
                t.filename
            );
            assert!(
                !t.description.is_empty(),
                "Template {} must have a non-empty description field",
                t.filename
            );
        }
    }

    #[test]
    fn test_templates_have_provider_placeholder() {
        // All templates use LLM, so they need provider/model placeholders
        for t in TEMPLATES {
            assert!(
                t.content.contains("{{PROVIDER}}") || t.content.contains("{{MODEL}}"),
                "Template {} should use {{{{PROVIDER}}}} or {{{{MODEL}}}} placeholders",
                t.filename
            );
        }
    }

    #[test]
    fn test_templates_have_usage_comments() {
        for t in TEMPLATES {
            assert!(
                t.content.contains("USAGE:") || t.content.contains("nika run"),
                "Template {} should have usage instructions in comments",
                t.filename
            );
        }
    }

    #[test]
    fn test_template_names_not_empty() {
        for t in TEMPLATES {
            assert!(!t.name.is_empty(), "Template name must not be empty");
            assert!(
                !t.category.is_empty(),
                "Template {} category must not be empty",
                t.filename
            );
        }
    }

    #[test]
    fn test_get_templates_returns_all() {
        let templates = get_templates();
        assert_eq!(templates.len(), 10);
    }

    #[test]
    fn test_templates_categories() {
        let categories: Vec<&str> = TEMPLATES.iter().map(|t| t.category).collect();
        assert!(
            categories.contains(&"devops"),
            "Should have devops category"
        );
        assert!(
            categories.contains(&"marketing"),
            "Should have marketing category"
        );
        assert!(
            categories.contains(&"content"),
            "Should have content category"
        );
        assert!(
            categories.contains(&"productivity"),
            "Should have productivity category"
        );
    }
}
