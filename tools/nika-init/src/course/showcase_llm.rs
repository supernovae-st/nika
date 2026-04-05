//! Showcase LLM workflows — 20 production-ready multi-verb demos
//!
//! Each workflow combines 2+ verbs (fetch+infer, exec+infer, infer+for_each, etc.)
//! and demonstrates real-world LLM patterns: structured output, artifact generation,
//! for_each iteration, DAG orchestration.
//!
//! All use `{{PROVIDER}}` / `{{MODEL}}` placeholders for provider portability.
//! All require an LLM provider (`requires_llm: true`).

use super::showcase::ShowcaseWorkflow;

/// All 20 LLM-powered showcase workflows
pub static SHOWCASE_LLM: &[ShowcaseWorkflow] = &[
    // =========================================================================
    // CATEGORY: CONTENT (1-5)
    // =========================================================================
    ShowcaseWorkflow {
        name: "blog-post-generator",
        description: "Research a topic, outline, write sections in parallel, assemble final post",
        category: "content",
        content: BLOG_POST_GENERATOR,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "email-draft-generator",
        description: "Three-pass email: draft, self-review with scoring, polish to final version",
        category: "content",
        content: EMAIL_DRAFT_GENERATOR,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "social-media-calendar",
        description: "Generate a week of posts, write platform-specific copy, compile calendar",
        category: "content",
        content: SOCIAL_MEDIA_CALENDAR,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "product-description-writer",
        description: "Extract features, write descriptions for 4 platforms in parallel",
        category: "content",
        content: PRODUCT_DESCRIPTION_WRITER,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "startup-pitch-generator",
        description: "Market research, problem validation, solution design, 10-slide pitch deck",
        category: "content",
        content: STARTUP_PITCH_GENERATOR,
        requires_llm: true,
    },
    // =========================================================================
    // CATEGORY: ENGINEERING (6-10)
    // =========================================================================
    ShowcaseWorkflow {
        name: "code-review-assistant",
        description: "Capture git diff, analyze for bugs and security issues, structured report",
        category: "engineering",
        content: CODE_REVIEW_ASSISTANT,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "release-notes-generator",
        description: "Parse git history, categorize commits, generate polished release notes",
        category: "engineering",
        content: RELEASE_NOTES_GENERATOR,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "technical-rfc-writer",
        description:
            "Problem analysis, solution design, alternatives evaluation, full RFC document",
        category: "engineering",
        content: TECHNICAL_RFC_WRITER,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "api-docs-generator",
        description: "Fetch OpenAPI spec, parse endpoints, generate reference docs and quickstart",
        category: "engineering",
        content: API_DOCS_GENERATOR,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "vulnerability-scanner-report",
        description:
            "Run cargo audit, analyze findings with CVSS scoring, executive security report",
        category: "engineering",
        content: VULNERABILITY_SCANNER_REPORT,
        requires_llm: true,
    },
    // =========================================================================
    // CATEGORY: ANALYSIS (11-15)
    // =========================================================================
    ShowcaseWorkflow {
        name: "competitive-analysis",
        description:
            "Scrape 3 competitor sites, extract profiles in parallel, strategic comparison",
        category: "analysis",
        content: COMPETITIVE_ANALYSIS,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "seo-content-optimizer",
        description: "Fetch metadata, content, and links, perform full SEO audit with scoring",
        category: "analysis",
        content: SEO_CONTENT_OPTIMIZER,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "data-analysis-report",
        description: "Generate sample data, compute statistics, extract insights, executive report",
        category: "analysis",
        content: DATA_ANALYSIS_REPORT,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "customer-feedback-analyzer",
        description: "Categorize feedback by sentiment, aggregate NPS metrics, insights report",
        category: "analysis",
        content: CUSTOMER_FEEDBACK_ANALYZER,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "knowledge-base-builder",
        description: "Scrape docs, extract Q&A pairs, organize FAQ, generate chatbot training data",
        category: "analysis",
        content: KNOWLEDGE_BASE_BUILDER,
        requires_llm: true,
    },
    // =========================================================================
    // CATEGORY: OPERATIONS (16-20)
    // =========================================================================
    ShowcaseWorkflow {
        name: "meeting-notes-processor",
        description: "Summarize notes, extract structured action items, generate follow-up email",
        category: "operations",
        content: MEETING_NOTES_PROCESSOR,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "interview-question-generator",
        description: "Analyze role, generate questions per category in parallel, interview guide",
        category: "operations",
        content: INTERVIEW_QUESTION_GENERATOR,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "sprint-retrospective",
        description:
            "Analyze sprint metrics, structured went-well/needs-improvement, retro document",
        category: "operations",
        content: SPRINT_RETROSPECTIVE,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "translation-pipeline",
        description: "Prepare source text, translate to 5 languages in parallel, quality report",
        category: "operations",
        content: TRANSLATION_PIPELINE,
        requires_llm: true,
    },
    ShowcaseWorkflow {
        name: "content-localization",
        description:
            "Write marketing copy, localize for 4 markets with cultural adaptation, review matrix",
        category: "operations",
        content: CONTENT_LOCALIZATION,
        requires_llm: true,
    },
];

// =============================================================================
// 01: BLOG POST GENERATOR
// fetch: topic research -> infer: outline -> for_each: write sections -> infer: assemble
// =============================================================================

const BLOG_POST_GENERATOR: &str = r##"# Blog Post Generator
# Researches a topic online, generates an outline, writes each section
# in parallel, then assembles the final post with intro and conclusion.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  topic:
    type: string
    default: "Why Rust is the Future of Infrastructure Software"

tasks:
  - id: research
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20

  - id: outline
    depends_on: [research]
    with:
      sources: $research
    infer:
      prompt: |
        Create a detailed blog post outline about "{{inputs.topic}}".
        Use these sources for context: {{with.sources | first(2000)}}
        Return JSON: { "title": "...", "sections": [{"heading": "...", "key_points": ["..."]}] }
        Include 4-5 sections with 3 key points each.
      response_format: json
      temperature: 0.5
      max_tokens: 1000
    artifact:
      path: output/blog-outline.json
      format: json

  - id: write_sections
    depends_on: [outline]
    with:
      plan: $outline
    for_each: "$outline.sections"
    as: section
    concurrency: 3
    infer:
      prompt: |
        Write this blog section about "{{inputs.topic}}":
        Heading: {{with.section.heading}}
        Key points to cover: {{with.section.key_points}}
        Write 200-400 words. Professional but engaging tone.
        Include code examples where relevant.
      temperature: 0.6
      max_tokens: 800

  - id: assemble
    depends_on: [outline, write_sections]
    with:
      plan: $outline
      sections: $write_sections
    infer:
      prompt: |
        Assemble this blog post with a compelling intro and conclusion.
        Outline: {{with.plan}}
        Sections: {{with.sections}}
        Add a TL;DR at the top and a call-to-action at the bottom.
      temperature: 0.4
      max_tokens: 3000
    artifact:
      path: output/blog-post.md
"##;

// =============================================================================
// 02: EMAIL DRAFT GENERATOR
// infer: draft -> infer: review -> infer: polish
// =============================================================================

const EMAIL_DRAFT_GENERATOR: &str = r##"# Email Draft Generator
# Three-pass email writing: draft, self-review with scoring, then polish + summary.
# Produces professional emails with the right tone for any context.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  context:
    type: string
    default: "Declining a job offer from TechCorp while keeping the door open for future opportunities"
  tone:
    type: string
    default: "professional, warm, grateful"

tasks:
  - id: draft
    infer:
      system: "You are an expert business communicator and email writer."
      prompt: |
        Write an email draft for this situation:
        Context: {{inputs.context}}
        Desired tone: {{inputs.tone}}
        Include subject line, greeting, body, and sign-off.
      temperature: 0.6
      max_tokens: 600

  - id: review
    depends_on: [draft]
    with:
      email: $draft
    infer:
      system: |
        You are an email communication coach. Review emails for
        tone, clarity, potential misinterpretation, and professionalism.
      prompt: |
        Review this email draft and provide specific feedback:
        {{with.email}}

        Return JSON: {
          "tone_score": 1-10,
          "clarity_score": 1-10,
          "issues": ["..."],
          "suggestions": ["..."],
          "rewrite_needed": true/false
        }
      response_format: json
      temperature: 0.2
      max_tokens: 600

  - id: polish
    depends_on: [draft, review]
    with:
      original: $draft
      feedback: $review
    infer:
      prompt: |
        Polish this email based on the review feedback:
        Original: {{with.original}}
        Feedback: {{with.feedback}}
        Apply all suggestions. Keep the tone: {{inputs.tone}}.
        Return only the final email, ready to send.
      temperature: 0.3
      max_tokens: 600
    artifact:
      path: output/polished-email.md

  - id: summary
    depends_on: [review, polish]
    with:
      scores: $review
      final_email: $polish
    exec:
      command: |
        echo "=== EMAIL GENERATION SUMMARY ==="
        echo "Tone requested: {{inputs.tone}}"
        echo "Review scores: {{with.scores}}"
        echo "Final email length: $(echo '{{with.final_email}}' | wc -w | tr -d ' ') words"
        echo "Artifact: output/polished-email.md"
      shell: true
"##;

// =============================================================================
// 03: SOCIAL MEDIA CALENDAR
// infer: topics -> for_each: write posts -> infer: compile
// =============================================================================

const SOCIAL_MEDIA_CALENDAR: &str = r##"# Social Media Content Calendar
# Generates a week of social media content across platforms,
# with optimized copy for each platform's format and audience.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  brand:
    type: string
    default: "A developer tools startup building open-source workflow automation"
  week_theme:
    type: string
    default: "Launch week: introducing our new CLI tool"

tasks:
  - id: generate_topics
    infer:
      system: "You are a social media strategist for tech brands."
      prompt: |
        Create a 7-day content calendar for: {{inputs.brand}}
        Week theme: {{inputs.week_theme}}
        Return JSON: {
          "posts": [
            {"day": "Monday", "topic": "...", "hook": "...", "cta": "...", "hashtags": ["..."]}
          ]
        }
        Mix types: educational, behind-the-scenes, announcement, engagement, meme.
      response_format: json
      temperature: 0.7
      max_tokens: 1200
    artifact:
      path: output/content-calendar.json
      format: json

  - id: write_posts
    depends_on: [generate_topics]
    with:
      calendar: $generate_topics
    for_each: "$generate_topics.posts"
    as: post
    concurrency: 4
    fail_fast: false
    infer:
      prompt: |
        Write platform-specific versions of this social media post:
        Day: {{with.post.day}} | Topic: {{with.post.topic}}
        Hook: {{with.post.hook}} | CTA: {{with.post.cta}}
        Hashtags: {{with.post.hashtags}}

        Write 3 versions:
        - Twitter/X (max 280 chars, punchy)
        - LinkedIn (professional, 100-200 words)
        - Mastodon/Bluesky (casual, 300 chars max)
      temperature: 0.7
      max_tokens: 800

  - id: compile_calendar
    depends_on: [write_posts]
    with:
      all_posts: $write_posts
    infer:
      prompt: |
        Compile all posts into a formatted content calendar:
        {{with.all_posts}}
        Create a Markdown table: Day | Platform | Copy | Hashtags.
        Add posting time recommendations for each platform.
      temperature: 0.3
      max_tokens: 3000
    artifact:
      path: output/social-media-calendar.md

  - id: summary
    depends_on: [compile_calendar]
    with:
      calendar: $compile_calendar
    exec:
      command: |
        echo "=== SOCIAL MEDIA CALENDAR GENERATED ==="
        echo "Brand: {{inputs.brand}}"
        echo "Theme: {{inputs.week_theme}}"
        echo "Word count: $(echo '{{with.calendar}}' | wc -w | tr -d ' ') words"
        echo "Artifact: output/social-media-calendar.md"
      shell: true
"##;

// =============================================================================
// 04: PRODUCT DESCRIPTION WRITER
// infer: extract features -> for_each: write per-platform -> infer: compile
// =============================================================================

const PRODUCT_DESCRIPTION_WRITER: &str = r##"# Product Description Writer
# Generates platform-optimized product descriptions for multiple
# marketplaces from a single feature list.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  product:
    type: string
    default: "Wireless noise-canceling headphones with 40h battery, spatial audio, and adaptive EQ"
  price:
    type: string
    default: "$199"

tasks:
  - id: extract_features
    infer:
      prompt: |
        Analyze this product and extract structured features:
        Product: {{inputs.product}} | Price: {{inputs.price}}
        Return JSON: {
          "name": "...", "category": "...",
          "key_features": ["..."], "target_audience": ["..."],
          "unique_selling_points": ["..."],
          "technical_specs": {"...": "..."}
        }
      response_format: json
      temperature: 0.2
      max_tokens: 600
    artifact:
      path: output/product-features.json
      format: json

  - id: write_descriptions
    depends_on: [extract_features]
    with:
      features: $extract_features
    for_each:
      - { platform: "Amazon", style: "SEO-optimized bullet points, keyword-rich title, A+ content" }
      - { platform: "Shopify", style: "storytelling, lifestyle-focused, experience over specs" }
      - { platform: "Product Hunt", style: "concise, tech-savvy, innovation focus" }
      - { platform: "Instagram Shop", style: "emoji-rich, casual, 2-3 punchy sentences" }
    as: target
    concurrency: 4
    infer:
      prompt: |
        Write a product description for {{with.target.platform}}:
        Product: {{with.features}}
        Style: {{with.target.style}} | Price: {{inputs.price}}
      temperature: 0.6
      max_tokens: 800

  - id: compile
    depends_on: [write_descriptions]
    with:
      descriptions: $write_descriptions
    infer:
      prompt: |
        Compile all product descriptions into a reference document:
        {{with.descriptions}}
        Add a comparison table: platform, char count, key messaging angle.
      temperature: 0.3
      max_tokens: 2000
    artifact:
      path: output/product-descriptions.md

  - id: summary
    depends_on: [compile]
    with:
      result: $compile
    exec:
      command: |
        echo "=== PRODUCT DESCRIPTIONS GENERATED ==="
        echo "Product: {{inputs.product}}"
        echo "Price: {{inputs.price}}"
        echo "Platforms: Amazon, Shopify, Product Hunt, Instagram Shop"
        echo "Word count: $(echo '{{with.result}}' | wc -w | tr -d ' ') words"
        echo "Artifact: output/product-descriptions.md"
      shell: true
"##;

// =============================================================================
// 05: STARTUP PITCH GENERATOR
// fetch: market -> infer: market analysis -> infer: problem -> infer: solution -> infer: pitch
// =============================================================================

const STARTUP_PITCH_GENERATOR: &str = r##"# Startup Pitch Generator
# Builds a complete pitch deck narrative through market research,
# problem validation, solution design, and business model analysis.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  idea:
    type: string
    default: "An open-source AI workflow engine that lets developers build LLM pipelines in YAML"
  target_market:
    type: string
    default: "Developer tools, AI/ML infrastructure"

tasks:
  - id: market_research
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://news.ycombinator.com/"
      extract: markdown
      timeout: 20

  - id: market_analysis
    depends_on: [market_research]
    with:
      trends: $market_research
    infer:
      system: "You are a venture capital analyst specializing in developer tools and AI."
      prompt: |
        Analyze the market for: {{inputs.idea}}
        Target: {{inputs.target_market}}
        Trends: {{with.trends | first(2000)}}
        Return JSON: {
          "tam": "...", "sam": "...", "som": "...",
          "growth_rate": "...", "key_trends": ["..."],
          "competitors": [{"name": "...", "positioning": "...", "weakness": "..."}],
          "timing": "Why now?"
        }
      response_format: json
      temperature: 0.4
      max_tokens: 1200
    artifact:
      path: output/market-analysis.json
      format: json

  - id: problem_validation
    depends_on: [market_analysis]
    with:
      market: $market_analysis
    infer:
      prompt: |
        Define and validate the problem for: {{inputs.idea}}
        Market context: {{with.market}}
        Cover: pain points, failed current solutions, cost of the problem,
        customer personas, problem severity score (1-10).
      temperature: 0.4
      max_tokens: 1000

  - id: solution_and_model
    depends_on: [problem_validation, market_analysis]
    with:
      problem: $problem_validation
      market: $market_analysis
    infer:
      prompt: |
        Craft the solution narrative and business model:
        Problem: {{with.problem}}
        Market: {{with.market}}
        Cover: "Aha!" moment, 3-step explanation, differentiators,
        revenue model, pricing tiers, unit economics, milestones.
      temperature: 0.5
      max_tokens: 1500

  - id: pitch_deck
    depends_on: [market_analysis, problem_validation, solution_and_model]
    with:
      market: $market_analysis
      problem: $problem_validation
      solution: $solution_and_model
    infer:
      prompt: |
        Create a 10-slide pitch deck narrative:
        Market: {{with.market}}
        Problem: {{with.problem}}
        Solution & Model: {{with.solution}}

        Each slide: ## Slide N: Title / Key Message / Content / Speaker Notes.
        Order: Hook, Problem, Solution, Market, Business Model,
        Traction, Team, Competition, Financials, Ask.
      temperature: 0.4
      max_tokens: 4000
    artifact:
      path: output/pitch-deck.md
"##;

// =============================================================================
// 06: CODE REVIEW ASSISTANT
// exec: git diff -> infer: analyze -> infer: report
// =============================================================================

const CODE_REVIEW_ASSISTANT: &str = r##"# Code Review Assistant
# Captures the current git diff, analyzes code quality, security, and
# performance, then produces a structured report with recommendations.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

tasks:
  - id: git_diff
    exec:
      command: git diff HEAD~1 --stat && echo "---" && git diff HEAD~1
      shell: true

  - id: git_log
    exec:
      command: git log --oneline -5
      shell: true

  - id: analyze
    depends_on: [git_diff, git_log]
    with:
      diff: $git_diff
      log: $git_log
    infer:
      system: |
        You are a senior code reviewer. Analyze diffs for bugs, security
        vulnerabilities, performance issues, style, and missing error handling.
        Reference line numbers. Be specific.
      prompt: |
        Review this code change:
        Recent commits: {{with.log}}
        Diff: {{with.diff | first(4000)}}

        Return JSON: {
          "summary": "...",
          "risk_level": "low|medium|high|critical",
          "issues": [{"severity": "...", "category": "...", "description": "...", "suggestion": "..."}],
          "approved": true/false
        }
      response_format: json
      temperature: 0.2
      max_tokens: 2000
    artifact:
      path: output/code-review.json
      format: json

  - id: report
    depends_on: [analyze]
    with:
      review: $analyze
    infer:
      prompt: |
        Format this code review as a clean Markdown report:
        {{with.review}}
        Include a summary table of issues sorted by severity.
      temperature: 0.3
      max_tokens: 1500
    artifact:
      path: output/code-review-report.md
"##;

// =============================================================================
// 07: RELEASE NOTES GENERATOR
// exec: git log -> infer: categorize -> infer: release notes
// =============================================================================

const RELEASE_NOTES_GENERATOR: &str = r##"# Release Notes Generator
# Reads recent git history, categorizes commits by type,
# and generates user-friendly release notes with highlights.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

tasks:
  - id: git_history
    exec:
      command: git log --oneline --no-merges -30
      shell: true

  - id: git_stats
    exec:
      command: |
        echo "=== Files Changed ==="
        git diff --stat HEAD~10 HEAD 2>/dev/null || echo "N/A"
        echo ""
        echo "=== Contributors ==="
        git log --format='%aN' -30 | sort | uniq -c | sort -rn 2>/dev/null || echo "N/A"
      shell: true

  - id: categorize
    depends_on: [git_history]
    with:
      log: $git_history
    infer:
      system: |
        You parse git histories into structured changelogs.
        Categories: Features, Bug Fixes, Performance, Breaking Changes,
        Documentation, Internal, Dependencies.
      prompt: |
        Categorize these commits: {{with.log}}
        Return JSON: {
          "version": "suggested semver",
          "categories": {
            "features": [{"commit": "...", "description": "...", "user_impact": "..."}],
            "bug_fixes": [{"commit": "...", "description": "...", "severity": "..."}],
            "performance": [], "breaking_changes": [], "docs": [], "internal": []
          },
          "highlights": ["top 3 most impactful changes"]
        }
      response_format: json
      temperature: 0.2
      max_tokens: 2000
    artifact:
      path: output/changelog-structured.json
      format: json

  - id: release_notes
    depends_on: [categorize, git_stats]
    with:
      changelog: $categorize
      stats: $git_stats
    infer:
      prompt: |
        Write polished release notes:
        Changelog: {{with.changelog}}
        Stats: {{with.stats}}

        Format: # Release vX.Y.Z / > summary / ## Highlights (emoji) /
        ## Features / ## Bug Fixes / ## Breaking Changes (migration guide) /
        ## Contributors / ## Full Changelog
      temperature: 0.4
      max_tokens: 2500
    artifact:
      path: output/release-notes.md
"##;

// =============================================================================
// 08: TECHNICAL RFC WRITER
// infer: problem -> infer: solution -> infer: alternatives -> infer: assemble
// =============================================================================

const TECHNICAL_RFC_WRITER: &str = r##"# Technical RFC Writer
# Guides creation of a complete RFC through four phases:
# problem analysis, solution design, alternatives, and assembly.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  problem:
    type: string
    default: "Our monolithic API is hitting scaling limits at 10k req/sec. Need microservices without downtime."
  constraints:
    type: string
    default: "Zero downtime, 3-month timeline, team of 4, backward compatibility required"

tasks:
  - id: problem_analysis
    infer:
      system: "You are a principal engineer writing RFCs for a high-growth startup."
      prompt: |
        Analyze this problem for an RFC:
        Problem: {{inputs.problem}}
        Constraints: {{inputs.constraints}}
        Return JSON: {
          "title": "RFC-NNN: ...", "status": "Draft",
          "problem_statement": "...",
          "impact": {"users_affected": "...", "revenue_risk": "...", "technical_debt": "..."},
          "success_criteria": ["..."], "non_goals": ["..."]
        }
      response_format: json
      temperature: 0.3
      max_tokens: 1000

  - id: solution_design
    depends_on: [problem_analysis]
    with:
      problem: $problem_analysis
    infer:
      system: "You are a system architect designing scalable distributed systems."
      prompt: |
        Design the proposed solution: {{with.problem}}
        Cover: architecture, migration strategy, data model changes,
        API compatibility layer, rollback plan, timeline with milestones.
      temperature: 0.4
      max_tokens: 2000

  - id: alternatives
    depends_on: [problem_analysis]
    with:
      problem: $problem_analysis
    infer:
      prompt: |
        Propose 3 alternatives to: {{with.problem}}
        For each: description, pros/cons, effort, risk, why not chosen.
        Include "Do Nothing" with its consequences.
      temperature: 0.5
      max_tokens: 1500

  - id: assemble_rfc
    depends_on: [problem_analysis, solution_design, alternatives]
    with:
      problem: $problem_analysis
      solution: $solution_design
      alternatives: $alternatives
    infer:
      prompt: |
        Assemble a complete RFC:
        Problem: {{with.problem}}
        Solution: {{with.solution}}
        Alternatives: {{with.alternatives}}

        Standard format: Title/Status/Author/Date, Summary, Problem,
        Proposed Solution, Alternatives, Migration Plan, Success Metrics,
        Open Questions, References.
      temperature: 0.2
      max_tokens: 4000
    artifact:
      path: output/technical-rfc.md

  - id: summary
    depends_on: [assemble_rfc]
    with:
      rfc: $assemble_rfc
    exec:
      command: |
        echo "=== RFC GENERATED ==="
        echo "Problem: {{inputs.problem}}"
        echo "Constraints: {{inputs.constraints}}"
        echo "Word count: $(echo '{{with.rfc}}' | wc -w | tr -d ' ') words"
        echo "Artifact: output/technical-rfc.md"
      shell: true
"##;

// =============================================================================
// 09: API DOCS GENERATOR
// fetch: OpenAPI spec -> infer: parse -> infer: docs -> infer: quickstart -> infer: assemble
// =============================================================================

const API_DOCS_GENERATOR: &str = r##"# API Documentation Generator
# Fetches an API spec, generates human-friendly docs with examples,
# error guides, and a quickstart tutorial.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

tasks:
  - id: fetch_spec
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://petstore3.swagger.io/api/v3/openapi.json"
      timeout: 15

  - id: parse_endpoints
    depends_on: [fetch_spec]
    with:
      spec: $fetch_spec
    infer:
      prompt: |
        Parse this OpenAPI spec and extract endpoints:
        {{with.spec | first(4000)}}
        Return JSON: {
          "api_name": "...", "base_url": "...", "auth_type": "...",
          "endpoints": [{"method": "...", "path": "...", "summary": "...", "params": ["..."]}]
        }
      response_format: json
      temperature: 0.1
      max_tokens: 2000
    artifact:
      path: output/api-endpoints.json
      format: json

  - id: generate_docs
    depends_on: [parse_endpoints]
    with:
      api: $parse_endpoints
    infer:
      system: "You create clear, developer-friendly API documentation with curl examples."
      prompt: |
        Generate complete API docs from: {{with.api}}
        Include: overview, authentication, each endpoint with examples,
        error codes, rate limiting. Markdown with code blocks.
      temperature: 0.3
      max_tokens: 3000

  - id: quickstart
    depends_on: [parse_endpoints]
    with:
      api: $parse_endpoints
    infer:
      prompt: |
        Write a 5-minute quickstart guide: {{with.api}}
        Include: setup, first request, common patterns, next steps.
        Show curl and JavaScript fetch examples.
      temperature: 0.4
      max_tokens: 1200

  - id: assemble
    depends_on: [generate_docs, quickstart]
    with:
      reference: $generate_docs
      guide: $quickstart
    infer:
      prompt: |
        Combine into one API documentation file:
        # Quickstart
        {{with.guide}}
        # Reference
        {{with.reference}}
        Add a table of contents at the top.
      temperature: 0.2
      max_tokens: 4000
    artifact:
      path: output/api-documentation.md
"##;

// =============================================================================
// 10: VULNERABILITY SCANNER REPORT
// exec: cargo audit -> infer: analyze -> infer: report
// =============================================================================

const VULNERABILITY_SCANNER_REPORT: &str = r##"# Vulnerability Scanner Report
# Runs security audit commands, analyzes and prioritizes findings
# with CVSS-like scoring, generates executive security report.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

tasks:
  - id: cargo_audit
    exec:
      command: cargo audit 2>&1 || echo "Audit complete"
      shell: true

  - id: dependency_tree
    exec:
      command: cargo tree --depth 2 2>&1 | head -50 || echo "No Cargo.toml"
      shell: true

  - id: outdated_check
    exec:
      command: cargo outdated 2>&1 | head -30 || echo "cargo-outdated not installed"
      shell: true

  - id: analyze
    depends_on: [cargo_audit, dependency_tree, outdated_check]
    with:
      audit: $cargo_audit
      deps: $dependency_tree
      outdated: $outdated_check
    infer:
      system: |
        You are a senior security engineer. Analyze dependency audits
        with CVSS-like severity scoring. Focus on exploitability.
      prompt: |
        Analyze these scan results:
        Audit: {{with.audit}}
        Deps: {{with.deps}}
        Outdated: {{with.outdated}}

        Return JSON: {
          "scan_summary": {"total_deps": 0, "vulnerabilities": 0, "outdated": 0},
          "critical": [{"package": "...", "cve": "...", "fix": "..."}],
          "high": [], "medium": [], "low": [],
          "recommendations": [{"priority": 1, "action": "...", "effort": "..."}],
          "supply_chain_risk": "low|medium|high"
        }
      response_format: json
      temperature: 0.1
      max_tokens: 2500
    artifact:
      path: output/vulnerability-analysis.json
      format: json

  - id: report
    depends_on: [analyze]
    with:
      analysis: $analyze
    infer:
      prompt: |
        Create an executive security report:
        {{with.analysis}}
        Include: executive summary, risk dashboard, critical findings,
        remediation timeline, dependency health scorecard.
      temperature: 0.2
      max_tokens: 2500
    artifact:
      path: output/security-report.md
"##;

// =============================================================================
// 11: COMPETITIVE ANALYSIS
// fetch: 3 sites -> for_each: extract profiles -> infer: compare
// =============================================================================

const COMPETITIVE_ANALYSIS: &str = r##"# Competitive Analysis
# Scrapes multiple competitor websites, extracts key info from each,
# then synthesizes a comparison matrix with strategic recommendations.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

tasks:
  - id: scrape_vercel
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://vercel.com"
      extract: metadata
      timeout: 20

  - id: scrape_netlify
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://www.netlify.com"
      extract: metadata
      timeout: 20

  - id: scrape_cloudflare
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://pages.cloudflare.com"
      extract: metadata
      timeout: 20

  - id: extract_profiles
    depends_on: [scrape_vercel, scrape_netlify, scrape_cloudflare]
    with:
      vercel: $scrape_vercel
      netlify: $scrape_netlify
      cloudflare: $scrape_cloudflare
    for_each:
      - { name: "Vercel", data: "{{with.vercel}}" }
      - { name: "Netlify", data: "{{with.netlify}}" }
      - { name: "Cloudflare Pages", data: "{{with.cloudflare}}" }
    as: competitor
    concurrency: 3
    fail_fast: false
    infer:
      prompt: |
        Analyze this competitor's positioning:
        Company: {{with.competitor.name}}
        Data: {{with.competitor.data}}
        Return JSON: {
          "name": "...", "tagline": "...", "target_audience": "...",
          "key_features": ["..."], "pricing_model": "...",
          "strengths": ["..."], "weaknesses": ["..."]
        }
      response_format: json
      temperature: 0.3
      max_tokens: 600

  - id: comparison
    depends_on: [extract_profiles]
    with:
      profiles: $extract_profiles
    infer:
      prompt: |
        Create a strategic competitive analysis:
        {{with.profiles}}
        Include: feature comparison table, market positioning map,
        gaps and opportunities, top 5 strategic recommendations.
      temperature: 0.4
      max_tokens: 2000
    artifact:
      path: output/competitive-analysis.md
"##;

// =============================================================================
// 12: SEO CONTENT OPTIMIZER
// fetch: URL (3 extracts) -> infer: audit -> infer: report
// =============================================================================

const SEO_CONTENT_OPTIMIZER: &str = r##"# SEO Content Optimizer
# Fetches a webpage's metadata, content, and links, then performs
# a comprehensive SEO audit with scores and recommendations.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  url:
    type: string
    default: "https://github.com"

tasks:
  - id: fetch_metadata
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "{{inputs.url}}"
      extract: metadata
      timeout: 20

  - id: fetch_content
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "{{inputs.url}}"
      extract: markdown
      timeout: 20

  - id: fetch_links
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "{{inputs.url}}"
      extract: links
      timeout: 20

  - id: seo_audit
    depends_on: [fetch_metadata, fetch_content, fetch_links]
    with:
      meta: $fetch_metadata
      content: $fetch_content
      links: $fetch_links
    infer:
      system: "You are a senior SEO specialist with 10+ years of experience."
      prompt: |
        Perform a complete SEO audit:
        Metadata: {{with.meta}}
        Content: {{with.content | first(3000)}}
        Links: {{with.links | first(2000)}}

        Return JSON: {
          "scores": {"title": 0-100, "meta_description": 0-100, "headings": 0-100, "content_quality": 0-100, "links": 0-100, "overall": 0-100},
          "critical_issues": [{"issue": "...", "impact": "high|medium|low", "fix": "..."}],
          "quick_wins": [{"action": "...", "effort": "...", "impact": "..."}],
          "keyword_opportunities": ["..."]
        }
      response_format: json
      temperature: 0.2
      max_tokens: 2000
    artifact:
      path: output/seo-audit.json
      format: json

  - id: seo_report
    depends_on: [seo_audit]
    with:
      audit: $seo_audit
    infer:
      prompt: |
        Create a professional SEO audit report:
        {{with.audit}}
        Format as Markdown with score badges, priority tables, 30-day plan.
      temperature: 0.3
      max_tokens: 2000
    artifact:
      path: output/seo-report.md
"##;

// =============================================================================
// 13: DATA ANALYSIS REPORT
// exec: generate data + stats -> infer: analyze -> infer: report
// =============================================================================

const DATA_ANALYSIS_REPORT: &str = r##"# Data Analysis Report
# Generates sample business data, computes statistics via shell,
# then uses an LLM to extract insights and build an executive report.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

tasks:
  - id: generate_data
    exec:
      command: |
        echo "month,revenue,users,churn_rate"
        echo "Jan,45000,1200,0.05"
        echo "Feb,52000,1350,0.04"
        echo "Mar,48000,1100,0.06"
        echo "Apr,61000,1500,0.03"
        echo "May,58000,1450,0.04"
        echo "Jun,72000,1800,0.02"
        echo "Jul,68000,1700,0.03"
        echo "Aug,75000,1900,0.02"
        echo "Sep,82000,2100,0.02"
        echo "Oct,79000,2000,0.03"
        echo "Nov,91000,2400,0.01"
        echo "Dec,95000,2600,0.01"
      shell: true
    artifact:
      path: output/sample-data.csv

  - id: compute_stats
    depends_on: [generate_data]
    exec:
      command: |
        echo "=== Revenue ==="
        echo "Total: 826000 | Avg: 68833 | Min: 45000 (Jan) | Max: 95000 (Dec) | Growth: 111%"
        echo ""
        echo "=== Users ==="
        echo "Peak: 2600 (Dec) | Avg: 1758 | Growth: 117%"
        echo ""
        echo "=== Churn ==="
        echo "Avg: 0.030 | Best: 0.01 (Nov,Dec) | Worst: 0.06 (Mar)"
      shell: true

  - id: analyze
    depends_on: [generate_data, compute_stats]
    with:
      data: $generate_data
      stats: $compute_stats
    infer:
      system: "You are a senior data analyst specializing in SaaS metrics."
      prompt: |
        Analyze this business data:
        Raw: {{with.data}}
        Stats: {{with.stats}}

        Return JSON: {
          "executive_summary": "...",
          "trends": [{"metric": "...", "trend": "up|down|stable", "insight": "..."}],
          "anomalies": [{"month": "...", "metric": "...", "description": "..."}],
          "correlations": ["..."],
          "forecast_q1": {"revenue": 0, "users": 0, "churn": 0.0},
          "recommendations": [{"priority": "high|medium|low", "action": "...", "expected_impact": "..."}]
        }
      response_format: json
      temperature: 0.2
      max_tokens: 2000
    artifact:
      path: output/data-analysis.json
      format: json

  - id: report
    depends_on: [analyze]
    with:
      analysis: $analyze
    infer:
      prompt: |
        Create an executive data analysis report:
        {{with.analysis}}
        Include ASCII charts, trend arrows, clear recommendations.
        Polished Markdown for stakeholder presentation.
      temperature: 0.3
      max_tokens: 2500
    artifact:
      path: output/data-report.md
"##;

// =============================================================================
// 14: CUSTOMER FEEDBACK ANALYZER
// infer: categorize -> infer: aggregate -> infer: report
// =============================================================================

const CUSTOMER_FEEDBACK_ANALYZER: &str = r##"# Customer Feedback Analyzer
# Processes customer feedback, categorizes by sentiment and topic,
# aggregates NPS metrics, generates actionable insights report.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  feedback:
    type: string
    default: |
      1. "Love the new dashboard! Dark mode is chef's kiss." - 5 stars
      2. "App crashes on PDF export. Very frustrating." - 1 star
      3. "Good but pricing is confusing. Too many tiers." - 3 stars
      4. "Support was incredible. Resolved in 5 minutes." - 5 stars
      5. "Mobile app missing half the features." - 2 stars
      6. "API docs are outdated. Wasted 2 hours." - 2 stars
      7. "Best workflow tool. Replaced 3 other tools." - 5 stars
      8. "Onboarding tutorial too long." - 3 stars
      9. "Slack integration broken after update." - 1 star
      10. "Feature request: calendar view for timelines." - 4 stars

tasks:
  - id: categorize
    infer:
      system: "You categorize feedback with NPS methodology: promoters (4-5), passives (3), detractors (1-2)."
      prompt: |
        Categorize each piece of feedback:
        {{inputs.feedback}}
        Return JSON: {
          "items": [{
            "id": 1, "text": "...",
            "sentiment": "positive|negative|neutral", "score": 1-5,
            "nps_category": "promoter|passive|detractor",
            "topics": ["..."], "urgency": "critical|high|medium|low",
            "department": "engineering|design|support|product|billing"
          }]
        }
      response_format: json
      temperature: 0.1
      max_tokens: 2000
    artifact:
      path: output/feedback-categorized.json
      format: json

  - id: aggregate
    depends_on: [categorize]
    with:
      data: $categorize
    infer:
      prompt: |
        Aggregate these feedback items into insights:
        {{with.data}}
        Return JSON: {
          "nps_score": 0-100,
          "sentiment_breakdown": {"positive": 0, "negative": 0, "neutral": 0},
          "top_topics": [{"topic": "...", "count": 0, "avg_sentiment": "..."}],
          "critical_issues": ["..."],
          "department_scores": {"engineering": 0, "design": 0, "support": 0, "product": 0}
        }
      response_format: json
      temperature: 0.2
      max_tokens: 1000
    artifact:
      path: output/feedback-aggregate.json
      format: json

  - id: report
    depends_on: [categorize, aggregate]
    with:
      items: $categorize
      summary: $aggregate
    infer:
      prompt: |
        Create an executive customer feedback report:
        Items: {{with.items}} | Summary: {{with.summary}}
        Include: NPS trend, top issues table, department performance,
        customer quotes, prioritized action plan.
      temperature: 0.3
      max_tokens: 2500
    artifact:
      path: output/feedback-report.md

  - id: summary
    depends_on: [aggregate, report]
    with:
      metrics: $aggregate
    exec:
      command: |
        echo "=== FEEDBACK ANALYSIS COMPLETE ==="
        echo "Feedback items processed: 10"
        echo "Metrics: {{with.metrics}}"
        echo "Artifacts:"
        echo "  - output/feedback-categorized.json"
        echo "  - output/feedback-aggregate.json"
        echo "  - output/feedback-report.md"
      shell: true
"##;

// =============================================================================
// 15: KNOWLEDGE BASE BUILDER
// fetch: docs -> infer: extract QA -> infer: organize -> infer: chatbot data
// =============================================================================

const KNOWLEDGE_BASE_BUILDER: &str = r##"# Knowledge Base Builder
# Scrapes documentation, extracts Q&A pairs, organizes into a
# structured FAQ, and generates chatbot training data.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

tasks:
  - id: scrape_docs
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://doc.rust-lang.org/book/ch01-01-installation.html"
      extract: markdown
      timeout: 20

  - id: scrape_more
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
    fetch:
      url: "https://doc.rust-lang.org/book/ch01-02-hello-world.html"
      extract: markdown
      timeout: 20

  - id: extract_qa
    depends_on: [scrape_docs, scrape_more]
    with:
      docs: $scrape_docs
      faq: $scrape_more
    infer:
      system: "You build knowledge bases by extracting implicit and explicit questions from documentation."
      prompt: |
        Extract Q&A pairs from these docs:
        Doc 1: {{with.docs | first(3000)}}
        Doc 2: {{with.faq | first(3000)}}
        Return JSON: {
          "qa_pairs": [{
            "question": "...", "answer": "...",
            "category": "getting-started|installation|concepts|troubleshooting|advanced",
            "difficulty": "beginner|intermediate|advanced",
            "keywords": ["..."]
          }]
        }
        Extract at least 10 pairs.
      response_format: json
      temperature: 0.2
      max_tokens: 3000
    artifact:
      path: output/qa-pairs.json
      format: json

  - id: organize_kb
    depends_on: [extract_qa]
    with:
      pairs: $extract_qa
    infer:
      prompt: |
        Organize these Q&A pairs into a structured knowledge base:
        {{with.pairs}}
        Create: table of contents, categories with Q&A, cross-references,
        "Still stuck?" section, keyword index.
      temperature: 0.3
      max_tokens: 3000
    artifact:
      path: output/knowledge-base.md

  - id: chatbot_data
    depends_on: [extract_qa]
    with:
      pairs: $extract_qa
    infer:
      prompt: |
        Convert Q&A pairs into chatbot training data:
        {{with.pairs}}
        Return JSON: {
          "intents": [{
            "intent": "...", "examples": ["..."],
            "response": "...", "follow_up": "..."
          }]
        }
      response_format: json
      temperature: 0.3
      max_tokens: 2000
    artifact:
      path: output/chatbot-training.json
      format: json
"##;

// =============================================================================
// 16: MEETING NOTES PROCESSOR
// infer: summarize -> infer: extract actions -> infer: followup email
// =============================================================================

const MEETING_NOTES_PROCESSOR: &str = r##"# Meeting Notes Processor
# Takes raw meeting notes, extracts key decisions and action items,
# then generates a structured follow-up email.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  notes:
    type: string
    default: |
      Project sync - March 2026. Attendees: Alice (PM), Bob (Eng), Carol (Design).
      Alice: Sprint ends Friday. Ship the dashboard redesign.
      Bob: Backend API ready. Need 2 more days for caching layer.
      Carol: Design handoff complete. Found 3 a11y issues in nav.
      Alice: Bob, prioritize caching. Carol, file a11y bugs by EOD.
      Bob: Should we delay? Caching is critical for perf.
      Alice: No delay. Ship without caching, hotfix Monday.
      Decision: Ship Friday without caching. Hotfix caching Monday.
      Carol: I will update design system tokens by Thursday.

tasks:
  - id: summarize
    infer:
      system: "You are an expert meeting note-taker and project coordinator."
      prompt: |
        Summarize these meeting notes into key discussion points:
        {{inputs.notes}}
        Focus on decisions made and disagreements raised.
      temperature: 0.3
      max_tokens: 600

  - id: extract_actions
    depends_on: [summarize]
    with:
      summary: $summarize
    infer:
      prompt: |
        Extract all action items from:
        Summary: {{with.summary}}
        Original: {{inputs.notes}}
        Return JSON: {
          "decisions": [{"decision": "...", "rationale": "..."}],
          "action_items": [{"owner": "...", "task": "...", "deadline": "...", "priority": "high|medium|low"}],
          "risks": [{"risk": "...", "mitigation": "..."}],
          "next_meeting": "suggested date/topic"
        }
      response_format: json
      temperature: 0.2
      max_tokens: 1000
    artifact:
      path: output/action-items.json
      format: json

  - id: followup_email
    depends_on: [summarize, extract_actions]
    with:
      summary: $summarize
      actions: $extract_actions
    infer:
      prompt: |
        Write a professional follow-up email:
        Summary: {{with.summary}}
        Actions: {{with.actions}}
        Include: subject line, bullet action items with owners, next deadline.
      temperature: 0.4
      max_tokens: 800
    artifact:
      path: output/meeting-followup.md

  - id: summary
    depends_on: [extract_actions, followup_email]
    with:
      actions: $extract_actions
    exec:
      command: |
        echo "=== MEETING NOTES PROCESSED ==="
        echo "Actions extracted: {{with.actions}}"
        echo "Artifacts:"
        echo "  - output/action-items.json"
        echo "  - output/meeting-followup.md"
      shell: true
"##;

// =============================================================================
// 17: INTERVIEW QUESTION GENERATOR
// infer: role analysis -> for_each: question categories -> infer: guide
// =============================================================================

const INTERVIEW_QUESTION_GENERATOR: &str = r##"# Interview Question Generator
# Analyzes a job role, generates targeted questions across categories
# in parallel, then assembles a complete interview guide.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  role:
    type: string
    default: "Senior Backend Engineer - Rust/Python, distributed systems, 5+ years"
  company_values:
    type: string
    default: "Open source, technical excellence, autonomy, user empathy"

tasks:
  - id: role_analysis
    infer:
      system: "You are a senior technical recruiter and hiring manager."
      prompt: |
        Analyze this role for interview planning:
        Role: {{inputs.role}}
        Values: {{inputs.company_values}}
        Return JSON: {
          "role_title": "...", "seniority": "...",
          "core_skills": ["..."], "culture_fit_traits": ["..."],
          "question_categories": [
            {"category": "System Design", "weight": 30},
            {"category": "Coding", "weight": 25},
            {"category": "Behavioral", "weight": 20},
            {"category": "Technical Deep-Dive", "weight": 15},
            {"category": "Culture Fit", "weight": 10}
          ]
        }
      response_format: json
      temperature: 0.3
      max_tokens: 800

  - id: generate_questions
    depends_on: [role_analysis]
    with:
      analysis: $role_analysis
    for_each: "$role_analysis.question_categories"
    as: category
    concurrency: 5
    infer:
      prompt: |
        Generate 4 interview questions for: {{inputs.role}}
        Category: {{with.category.category}} (weight: {{with.category.weight}}%)
        Context: {{with.analysis}}

        Each with: the question, what it tests, green flags,
        red flags, 2 follow-up probes. Scale difficulty.
      temperature: 0.5
      max_tokens: 1200

  - id: interview_guide
    depends_on: [role_analysis, generate_questions]
    with:
      analysis: $role_analysis
      questions: $generate_questions
    infer:
      prompt: |
        Create a complete interview guide:
        Role: {{with.analysis}}
        Questions: {{with.questions}}
        Include: pre-interview checklist, timing per section,
        all questions by category, scoring rubric (1-5),
        post-interview evaluation template.
      temperature: 0.3
      max_tokens: 3500
    artifact:
      path: output/interview-guide.md

  - id: summary
    depends_on: [interview_guide]
    with:
      guide: $interview_guide
    exec:
      command: |
        echo "=== INTERVIEW GUIDE GENERATED ==="
        echo "Role: {{inputs.role}}"
        echo "Word count: $(echo '{{with.guide}}' | wc -w | tr -d ' ') words"
        echo "Artifact: output/interview-guide.md"
      shell: true
"##;

// =============================================================================
// 18: SPRINT RETROSPECTIVE
// infer: analyze metrics -> infer: retro analysis -> infer: document
// =============================================================================

const SPRINT_RETROSPECTIVE: &str = r##"# Sprint Retrospective Generator
# Analyzes sprint data, produces structured insights with action items,
# and generates a facilitation-ready retro document.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  sprint_data:
    type: string
    default: |
      Sprint 14 - "Project Phoenix" (2 weeks)
      Planned: 34 pts | Completed: 28 | Carried: 6
      Team: 4 engineers, 1 designer
      Velocity: 25, 30, 32, 28 (last 4 sprints)
      Incidents: 1 P2 (auth service down 2h Tuesday)
      Deploys: 8 ok, 1 rollback (Thursday)
      PR cycle time: avg 18h (target 12h)
      Feedback: "Too many meetings", "Great auth fix collab",
      "Unclear dashboard requirements", "On-call needs review"

tasks:
  - id: analyze_metrics
    infer:
      system: "You are an Agile coach. Data-driven insights, not platitudes."
      prompt: |
        Analyze sprint metrics: {{inputs.sprint_data}}
        Return JSON: {
          "sprint_health": "healthy|warning|critical",
          "velocity_trend": "improving|stable|declining",
          "completion_rate": 0.0,
          "metric_insights": [{"metric": "...", "status": "green|yellow|red", "insight": "..."}],
          "risks": [{"risk": "...", "probability": "high|medium|low", "mitigation": "..."}]
        }
      response_format: json
      temperature: 0.2
      max_tokens: 1000
    artifact:
      path: output/sprint-metrics.json
      format: json

  - id: retro_analysis
    depends_on: [analyze_metrics]
    with:
      metrics: $analyze_metrics
    infer:
      prompt: |
        Generate structured retrospective:
        Data: {{inputs.sprint_data}}
        Metrics: {{with.metrics}}
        Return JSON: {
          "went_well": [{"item": "...", "evidence": "...", "keep_doing": "..."}],
          "needs_improvement": [{"item": "...", "root_cause": "...", "fix": "..."}],
          "action_items": [{"action": "...", "owner": "team|pm|lead", "deadline": "...", "metric": "..."}],
          "experiments": [{"hypothesis": "If X then Y", "duration": "1 sprint", "measure": "..."}]
        }
      response_format: json
      temperature: 0.3
      max_tokens: 1500
    artifact:
      path: output/retro-analysis.json
      format: json

  - id: retro_document
    depends_on: [analyze_metrics, retro_analysis]
    with:
      metrics: $analyze_metrics
      retro: $retro_analysis
    infer:
      prompt: |
        Create sprint retrospective document:
        Metrics: {{with.metrics}} | Analysis: {{with.retro}}
        Include: sprint scorecard, wins, improvements (blameless),
        SMART action items, experiments, facilitation notes.
      temperature: 0.3
      max_tokens: 2500
    artifact:
      path: output/sprint-retro.md

  - id: summary
    depends_on: [retro_document]
    with:
      retro: $retro_document
    exec:
      command: |
        echo "=== SPRINT RETRO GENERATED ==="
        echo "Word count: $(echo '{{with.retro}}' | wc -w | tr -d ' ') words"
        echo "Artifacts:"
        echo "  - output/sprint-metrics.json"
        echo "  - output/retro-analysis.json"
        echo "  - output/sprint-retro.md"
      shell: true
"##;

// =============================================================================
// 19: TRANSLATION PIPELINE
// infer: prepare -> for_each: translate 5 langs -> infer: quality report
// =============================================================================

const TRANSLATION_PIPELINE: &str = r##"# Translation Pipeline
# Prepares source text, translates into 5 languages in parallel
# with cultural adaptation, then runs a quality review.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  content:
    type: string
    default: |
      Introducing Nika: the open-source workflow engine that puts AI to work.
      Build powerful automation pipelines in YAML. Chain LLM calls, HTTP requests,
      shell commands, and tool invocations into reproducible workflows.
      No vendor lock-in. No black boxes. Just clean, declarative power.

tasks:
  - id: prepare_source
    infer:
      prompt: |
        Prepare this content for international translation:
        {{inputs.content}}
        Return JSON: {
          "source_text": "...",
          "key_terms": [{"term": "...", "context": "...", "keep_english": true/false}],
          "tone": "...", "cultural_notes": "..."
        }
      response_format: json
      temperature: 0.2
      max_tokens: 600
    artifact:
      path: output/translation-source.json
      format: json

  - id: translate
    depends_on: [prepare_source]
    with:
      source: $prepare_source
    for_each:
      - { code: "fr-FR", name: "French", notes: "Use vous (formal). Tech terms stay English." }
      - { code: "de-DE", name: "German", notes: "Compound nouns ok. Formal register." }
      - { code: "ja-JP", name: "Japanese", notes: "Desu/masu form. Katakana for tech terms." }
      - { code: "es-ES", name: "Spanish", notes: "Castilian. Ustedes form." }
      - { code: "zh-CN", name: "Chinese", notes: "Mainland conventions. Translate tech terms." }
    as: lang
    concurrency: 5
    fail_fast: false
    infer:
      system: "You are a professional translator. Adapt culturally, not just linguistically."
      prompt: |
        Translate to {{with.lang.name}} ({{with.lang.code}}):
        Source: {{with.source}}
        Notes: {{with.lang.notes}}
        Return translated text then a "Translator Notes" section.
      temperature: 0.3
      max_tokens: 1000
    artifact:
      path: "output/translations/{{with.lang.code}}.md"

  - id: quality_report
    depends_on: [translate]
    with:
      translations: $translate
    infer:
      prompt: |
        Review all translations for consistency:
        {{with.translations}}
        Check: key term consistency, tone match, no lost content.
        Produce a quality scorecard per language.
      temperature: 0.2
      max_tokens: 1000
    artifact:
      path: output/translation-quality-report.md

  - id: summary
    depends_on: [quality_report]
    with:
      report: $quality_report
    exec:
      command: |
        echo "=== TRANSLATION PIPELINE COMPLETE ==="
        echo "Languages: fr-FR, de-DE, ja-JP, es-ES, zh-CN"
        echo "Quality report: {{with.report}}"
        echo "Artifact: output/translation-quality-report.md"
      shell: true
"##;

// =============================================================================
// 20: CONTENT LOCALIZATION
// infer: write copy -> for_each: localize 4 markets -> infer: review matrix
// =============================================================================

const CONTENT_LOCALIZATION: &str = r##"# Content Localization Pipeline
# Creates marketing copy then localizes for 4 international markets,
# adapting language, cultural references, and compliance notes.
schema: "nika/workflow@0.12"
provider: {{PROVIDER}}
model: {{MODEL}}

inputs:
  product_name:
    type: string
    default: "CloudSync Pro"
  description:
    type: string
    default: "Enterprise file sync with end-to-end encryption, real-time collaboration, and compliance built in"

tasks:
  - id: write_source
    infer:
      system: "You are a marketing copywriter for enterprise SaaS products."
      prompt: |
        Write marketing copy for: {{inputs.product_name}}
        Description: {{inputs.description}}
        Create: hero headline (10 words max), subheadline (25 words),
        3 value proposition blocks, CTA button text, social proof line.
      temperature: 0.6
      max_tokens: 800
    artifact:
      path: output/source-copy.md

  - id: localize
    depends_on: [write_source]
    with:
      source: $write_source
    for_each:
      - { market: "Japan", lang: "ja-JP", notes: "Emphasize security. Keigo forms. Replace Western metaphors." }
      - { market: "Germany", lang: "de-DE", notes: "GDPR emphasis. Formal tone. Engineering quality angle." }
      - { market: "Brazil", lang: "pt-BR", notes: "Warm, relationship-focused. LGPD compliance." }
      - { market: "UAE", lang: "ar-AE", notes: "RTL. Formal business Arabic. Local data residency." }
    as: locale
    concurrency: 4
    infer:
      system: "You are a localization specialist. Re-create impact for the local audience."
      prompt: |
        Localize for {{with.locale.market}} ({{with.locale.lang}}):
        Source: {{with.source}}
        Guidance: {{with.locale.notes}}
        Provide: localized copy, cultural notes, legal/compliance notes.
      temperature: 0.4
      max_tokens: 1200
    artifact:
      path: "output/localized/{{with.locale.lang}}.md"

  - id: review_matrix
    depends_on: [localize]
    with:
      versions: $localize
    infer:
      prompt: |
        Create a localization review matrix:
        {{with.versions}}
        Compare: messaging consistency, cultural adaptation quality,
        compliance completeness. Traffic-light ratings table.
      temperature: 0.2
      max_tokens: 1200
    artifact:
      path: output/localization-review.md

  - id: summary
    depends_on: [review_matrix]
    with:
      review: $review_matrix
    exec:
      command: |
        echo "=== LOCALIZATION COMPLETE ==="
        echo "Product: {{inputs.product_name}}"
        echo "Markets: Japan, Germany, Brazil, UAE"
        echo "Review: {{with.review}}"
        echo "Artifact: output/localization-review.md"
      shell: true
"##;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_showcase_llm_count() {
        assert_eq!(
            SHOWCASE_LLM.len(),
            20,
            "Must have exactly 20 LLM showcase workflows"
        );
    }

    #[test]
    fn test_showcase_llm_names_unique() {
        let mut names: Vec<&str> = SHOWCASE_LLM.iter().map(|w| w.name).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), len, "All names must be unique");
    }

    #[test]
    fn test_showcase_llm_all_require_llm() {
        for w in SHOWCASE_LLM {
            assert!(
                w.requires_llm,
                "Workflow '{}' must have requires_llm = true",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_llm_all_have_schema() {
        for w in SHOWCASE_LLM {
            assert!(
                w.content.contains("schema: \"nika/workflow@0.12\""),
                "Workflow '{}' must declare schema",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_llm_all_have_provider() {
        for w in SHOWCASE_LLM {
            assert!(
                w.content.contains("provider: {{PROVIDER}}"),
                "Workflow '{}' must have provider placeholder",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_llm_all_have_model() {
        for w in SHOWCASE_LLM {
            assert!(
                w.content.contains("model: {{MODEL}}"),
                "Workflow '{}' must have model placeholder",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_llm_all_have_tasks() {
        for w in SHOWCASE_LLM {
            assert!(
                w.content.contains("tasks:"),
                "Workflow '{}' must have tasks section",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_llm_all_use_infer() {
        for w in SHOWCASE_LLM {
            assert!(
                w.content.contains("infer:"),
                "Workflow '{}' must use infer: verb",
                w.name
            );
        }
    }

    #[test]
    fn test_showcase_llm_all_multi_verb() {
        let verbs = ["infer:", "exec:", "fetch:", "invoke:", "agent:"];
        for w in SHOWCASE_LLM {
            let verb_count = verbs.iter().filter(|v| w.content.contains(**v)).count();
            assert!(
                verb_count >= 2,
                "Workflow '{}' must use 2+ verbs, found {}",
                w.name,
                verb_count
            );
        }
    }

    #[test]
    fn test_showcase_llm_uses_depends_on() {
        let with_deps = SHOWCASE_LLM
            .iter()
            .filter(|w| w.content.contains("depends_on:"))
            .count();
        assert!(
            with_deps >= 18,
            "At least 18 workflows should use depends_on, found {}",
            with_deps
        );
    }

    #[test]
    fn test_showcase_llm_uses_with_bindings() {
        let with_bindings = SHOWCASE_LLM
            .iter()
            .filter(|w| w.content.contains("with:"))
            .count();
        assert!(
            with_bindings >= 18,
            "At least 18 workflows should use with: bindings, found {}",
            with_bindings
        );
    }

    #[test]
    fn test_showcase_llm_uses_artifact() {
        let with_artifact = SHOWCASE_LLM
            .iter()
            .filter(|w| w.content.contains("artifact:"))
            .count();
        assert!(
            with_artifact >= 15,
            "At least 15 workflows should produce artifacts, found {}",
            with_artifact
        );
    }

    #[test]
    fn test_showcase_llm_uses_structured_output() {
        let with_json = SHOWCASE_LLM
            .iter()
            .filter(|w| w.content.contains("response_format: json"))
            .count();
        assert!(
            with_json >= 10,
            "At least 10 workflows should use structured JSON output, found {}",
            with_json
        );
    }

    #[test]
    fn test_showcase_llm_uses_for_each() {
        let with_foreach = SHOWCASE_LLM
            .iter()
            .filter(|w| w.content.contains("for_each:"))
            .count();
        assert!(
            with_foreach >= 6,
            "At least 6 workflows should use for_each, found {}",
            with_foreach
        );
    }

    #[test]
    fn test_showcase_llm_verb_diversity() {
        let all: String = SHOWCASE_LLM
            .iter()
            .map(|w| w.content)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            all.contains("exec:"),
            "Must include exec: across all workflows"
        );
        assert!(
            all.contains("fetch:"),
            "Must include fetch: across all workflows"
        );
        assert!(
            all.contains("infer:"),
            "Must include infer: across all workflows"
        );
        assert!(all.contains("for_each:"), "Must include for_each: pattern");
        assert!(
            all.contains("response_format: json"),
            "Must include structured output"
        );
    }

    #[test]
    fn test_showcase_llm_categories() {
        let categories: Vec<&str> = SHOWCASE_LLM.iter().map(|w| w.category).collect();
        assert!(
            categories.contains(&"content"),
            "Must have content category"
        );
        assert!(
            categories.contains(&"engineering"),
            "Must have engineering category"
        );
        assert!(
            categories.contains(&"analysis"),
            "Must have analysis category"
        );
        assert!(
            categories.contains(&"operations"),
            "Must have operations category"
        );
    }

    #[test]
    fn test_showcase_llm_content_not_empty() {
        for w in SHOWCASE_LLM {
            assert!(
                w.content.len() > 200,
                "Workflow '{}' content too short ({} bytes)",
                w.name,
                w.content.len()
            );
        }
    }

    #[test]
    fn test_showcase_llm_line_count_range() {
        for w in SHOWCASE_LLM {
            let lines = w.content.lines().count();
            assert!(
                (20..=120).contains(&lines),
                "Workflow '{}' has {} lines (expected 20-120)",
                w.name,
                lines
            );
        }
    }
}
