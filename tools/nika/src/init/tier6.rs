//! Tier 6: Everyday Magic Workflows (21-30)
//!
//! Non-tech use cases that demonstrate Nika's practical value
//! for daily life. These workflows show how AI automation can
//! simplify everyday tasks - from morning briefings to meal planning.
//!
//! Prerequisites:
//! - nika provider set anthropic (or openai/mistral/groq)
//! - Optional: Various API keys for external services

use super::WorkflowTemplate;

/// Directory for tier 6 workflows
pub const TIER6_DIR: &str = "tier-6-magic";

/// Workflow 21: Morning Briefing
/// Creates a personalized daily summary with weather, news, and calendar
pub const WORKFLOW_21_MORNING_BRIEFING: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# WORKFLOW 21: MORNING BRIEFING ☀️
# ═══════════════════════════════════════════════════════════════════════════════
#
# Your personal AI assistant that creates a morning briefing!
# Gathers weather, top news, and your schedule into one summary.
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                         MORNING BRIEFING PIPELINE                          │
# │                                                                             │
# │    ┌──────────┐    ┌──────────┐    ┌──────────┐                            │
# │    │ weather  │    │   news   │    │ schedule │                            │
# │    │  fetch   │    │  fetch   │    │  fetch   │                            │
# │    └────┬─────┘    └────┬─────┘    └────┬─────┘                            │
# │         │               │               │                                   │
# │         └───────────────┼───────────────┘                                   │
# │                         │                                                   │
# │                         ▼                                                   │
# │              ┌─────────────────────┐                                        │
# │              │   compile_briefing  │ ← Synthesizes all data                 │
# │              └──────────┬──────────┘                                        │
# │                         │                                                   │
# │                         ▼                                                   │
# │              ┌─────────────────────┐                                        │
# │              │   format_output     │ ← Creates readable briefing            │
# │              └──────────┬──────────┘                                        │
# │                         │                                                   │
# │                         ▼                                                   │
# │                  📧 Daily Briefing                                          │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# USAGE:
#   nika run 21-morning-briefing.nika.yaml
#
# OUTPUT:
#   A beautifully formatted morning briefing ready for your day!
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
model: gpt-4o-mini
workflow: morning-briefing
description: "Your personalized morning briefing with weather, news, and schedule"

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────────────
# Customize these for your location and preferences

inputs:
  location: "Paris, France"
  timezone: "Europe/Paris"
  news_topics: ["technology", "business", "science"]

# ─────────────────────────────────────────────────────────────────────────────
# TASKS
# ─────────────────────────────────────────────────────────────────────────────

tasks:
  # ┌─────────────────────────────────────────────────────────────────────────┐
  # │ PARALLEL DATA GATHERING                                                 │
  # │ All three fetches run simultaneously for speed                          │
  # └─────────────────────────────────────────────────────────────────────────┘

  - id: get_weather
    description: "Fetch current weather conditions"
    # In real usage, you'd call a weather API
    # Here we simulate with an LLM for demo purposes
    infer:
      prompt: |
        Generate a realistic weather report for {{inputs.location}} for today.
        Include: temperature, conditions, humidity, and a brief forecast.
        Format as a short, friendly paragraph.
      temperature: 0.3
      max_tokens: 150

  - id: get_news
    description: "Fetch top news headlines"
    infer:
      prompt: |
        Generate 3-5 realistic top news headlines for today covering:
        Topics: {{inputs.news_topics}}

        Format each headline with:
        - 📰 Headline
        - One sentence summary
        - [Source name]

        Make them sound like real current events (but clearly fictional for demo).
      temperature: 0.7
      max_tokens: 400

  - id: get_schedule
    description: "Fetch today's schedule"
    infer:
      prompt: |
        Generate a realistic sample daily schedule for a professional.
        Include 4-5 items with times in {{inputs.timezone}} timezone.

        Format:
        - ⏰ HH:MM - Event name (duration)

        Include a mix of meetings, focus time, and breaks.
      temperature: 0.5
      max_tokens: 200

  # ┌─────────────────────────────────────────────────────────────────────────┐
  # │ SYNTHESIS PHASE                                                         │
  # │ Combine all gathered data into a coherent briefing                      │
  # └─────────────────────────────────────────────────────────────────────────┘

  - id: compile_briefing
    description: "Synthesize all data into a briefing"
    depends_on: [get_weather, get_news, get_schedule]
    with:
      weather: $get_weather
      news: $get_news
      schedule: $get_schedule
    infer:
      prompt: |
        You are a professional executive assistant preparing a morning briefing.

        Create a comprehensive yet concise morning briefing using this data:

        WEATHER:
        {{with.weather}}

        NEWS HEADLINES:
        {{with.news}}

        TODAY'S SCHEDULE:
        {{with.schedule}}

        Structure the briefing with:
        1. 🌤️ Weather at a glance (2-3 sentences)
        2. 📰 News digest (key takeaways, not full headlines)
        3. 📅 Day ahead (schedule overview with prep suggestions)
        4. 💡 Daily insight (a motivational or practical tip)

        Keep it professional but warm. Total length: ~300 words.
      temperature: 0.5
      max_tokens: 500

  # ┌─────────────────────────────────────────────────────────────────────────┐
  # │ OUTPUT FORMATTING                                                       │
  # │ Create the final polished briefing                                      │
  # └─────────────────────────────────────────────────────────────────────────┘

  - id: format_output
    description: "Create beautifully formatted final output"
    depends_on: [compile_briefing]
    with:
      briefing: $compile_briefing
    infer:
      prompt: |
        Format this morning briefing with beautiful ASCII art styling:

        {{with.briefing}}

        Add:
        - A nice header with date and location ({{inputs.location}})
        - Clear section separators
        - Emoji for visual appeal
        - A footer with "Generated by Nika 🦋"

        Make it look like a premium daily briefing email.
      temperature: 0.3
      max_tokens: 800
    # Save the briefing to a file
    artifact:
      path: "./output/morning-briefing-{{date}}.md"
      format: text
"##;

/// Workflow 22: Social Media Planner
/// Creates a week of social media posts in 2 minutes
pub const WORKFLOW_22_SOCIAL_MEDIA: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# WORKFLOW 22: SOCIAL MEDIA PLANNER 📱
# ═══════════════════════════════════════════════════════════════════════════════
#
# Generate a week of engaging social media content in minutes!
# Creates posts optimized for different platforms with hashtags,
# emojis, and optimal posting times.
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                      SOCIAL MEDIA CONTENT FACTORY                          │
# │                                                                             │
# │  ┌──────────────────┐                                                       │
# │  │  content_strategy │ ← Define weekly themes                               │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ▼                                                                 │
# │  ┌────────────────────────────────────────────────────────────────┐        │
# │  │                    FOR EACH: 7 days                             │        │
# │  │  ┌──────────────────────────────────────────────────────────┐  │        │
# │  │  │  generate_posts (Mon, Tue, Wed, Thu, Fri, Sat, Sun)      │  │        │
# │  │  │  Each day gets platform-specific content:                 │  │        │
# │  │  │  📘 LinkedIn | 🐦 Twitter/X | 📸 Instagram                │  │        │
# │  │  └──────────────────────────────────────────────────────────┘  │        │
# │  └────────────────────────────────────────────────────────────────┘        │
# │           │                                                                 │
# │           ▼                                                                 │
# │  ┌──────────────────┐                                                       │
# │  │ compile_calendar │ ← Merge into content calendar                         │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ▼                                                                 │
# │      📅 Weekly Calendar                                                     │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# USAGE:
#   nika run 22-social-media-planner.nika.yaml
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
model: gpt-4o-mini
workflow: social-media-planner
description: "Generate a week of platform-optimized social media content"

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────────────

inputs:
  brand_name: "TechStartup Inc"
  industry: "B2B SaaS"
  tone: "professional yet approachable"
  main_topics:
    - "productivity tips"
    - "industry insights"
    - "product features"
    - "team culture"

# ─────────────────────────────────────────────────────────────────────────────
# TASKS
# ─────────────────────────────────────────────────────────────────────────────

tasks:
  # ┌─────────────────────────────────────────────────────────────────────────┐
  # │ STRATEGY PHASE                                                          │
  # │ Plan the week's content themes                                          │
  # └─────────────────────────────────────────────────────────────────────────┘

  - id: content_strategy
    description: "Create weekly content strategy and themes"
    infer:
      prompt: |
        Create a 7-day social media content strategy for {{inputs.brand_name}}.

        Industry: {{inputs.industry}}
        Tone: {{inputs.tone}}
        Topics to cover: {{inputs.main_topics}}

        For each day (Monday-Sunday), define:
        1. Daily theme
        2. Key message
        3. Content type (tip, story, question, showcase, etc.)

        Format as JSON array:
        [
          {"day": "Monday", "theme": "...", "message": "...", "type": "..."},
          ...
        ]

        Ensure variety and a good mix of promotional vs value content.
      temperature: 0.7
      max_tokens: 600
    # Validate the strategy structure
    output:
      schema:
        type: array
        items:
          type: object
          properties:
            day: { type: string }
            theme: { type: string }
            message: { type: string }
            type: { type: string }
          required: [day, theme, message, type]

  # ┌─────────────────────────────────────────────────────────────────────────┐
  # │ PARALLEL CONTENT GENERATION                                             │
  # │ Generate posts for all 7 days in parallel                               │
  # └─────────────────────────────────────────────────────────────────────────┘

  - id: generate_posts
    description: "Generate platform-specific posts for each day"
    depends_on: [content_strategy]
    with:
      strategy: $content_strategy
    # Generate content for each day in parallel
    for_each: ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
    as: day
    concurrency: 7  # All days generated simultaneously
    infer:
      prompt: |
        Create social media posts for {{with.day}} based on this strategy:
        {{with.strategy}}

        Brand: {{inputs.brand_name}}
        Tone: {{inputs.tone}}

        Generate posts for 3 platforms:

        1. 📘 LINKEDIN POST (professional)
           - 150-200 words
           - Industry-relevant hashtags
           - Call to action

        2. 🐦 TWITTER/X POST (concise)
           - Max 280 characters
           - 2-3 trending hashtags
           - Engaging hook

        3. 📸 INSTAGRAM CAPTION (visual)
           - 100-150 words
           - Emojis throughout
           - 10-15 relevant hashtags at end

        Output as JSON:
        {
          "day": "{{with.day}}",
          "linkedin": {"post": "...", "hashtags": [...]},
          "twitter": {"post": "...", "hashtags": [...]},
          "instagram": {"caption": "...", "hashtags": [...]}
        }
      temperature: 0.8
      max_tokens: 800

  # ┌─────────────────────────────────────────────────────────────────────────┐
  # │ CALENDAR COMPILATION                                                    │
  # │ Merge all days into a comprehensive content calendar                    │
  # └─────────────────────────────────────────────────────────────────────────┘

  - id: compile_calendar
    description: "Create the final content calendar"
    depends_on: [generate_posts]
    with:
      posts: $generate_posts
    infer:
      prompt: |
        Create a beautiful, actionable content calendar from this data:

        {{with.posts}}

        Format as a markdown document with:

        # 📅 Weekly Social Media Calendar - {{inputs.brand_name}}

        For each day, show:
        - 📘 LinkedIn (post + best time to post)
        - 🐦 Twitter/X (post + best time to post)
        - 📸 Instagram (caption + best time to post)

        Add recommended posting times based on platform best practices.

        Include at the end:
        - 📊 Week summary (total posts, platform distribution)
        - 💡 Engagement tips for the week
        - ⏰ Optimal posting schedule table
      temperature: 0.4
      max_tokens: 2000
    artifact:
      path: "./output/social-calendar-week-{{date}}.md"
      format: text
"##;

/// Workflow 23: Competitor Analysis
/// Automated competitive intelligence gathering
pub const WORKFLOW_23_COMPETITOR_SPY: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# WORKFLOW 23: COMPETITOR ANALYSIS 🔍
# ═══════════════════════════════════════════════════════════════════════════════
#
# Gather competitive intelligence and create a comprehensive
# analysis report. Useful for market research and strategy planning.
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                       COMPETITIVE INTELLIGENCE FLOW                        │
# │                                                                             │
# │  ┌────────────────┐                                                         │
# │  │ define_analysis │ ← What are we analyzing?                               │
# │  └───────┬────────┘                                                         │
# │          │                                                                  │
# │          ▼                                                                  │
# │  ┌───────────────────────────────────────────────────────────────┐         │
# │  │              FOR EACH: competitor                              │         │
# │  │   ┌──────────────────────────────────────────────────────┐    │         │
# │  │   │    analyze_competitor (company A, B, C...)           │    │         │
# │  │   │    Products | Pricing | Positioning | Strengths     │    │         │
# │  │   └──────────────────────────────────────────────────────┘    │         │
# │  └───────────────────────────────────────────────────────────────┘         │
# │          │                                                                  │
# │          ▼                                                                  │
# │  ┌────────────────┐                                                         │
# │  │ swot_comparison │ ← Compare all competitors                              │
# │  └───────┬────────┘                                                         │
# │          │                                                                  │
# │          ▼                                                                  │
# │  ┌────────────────┐                                                         │
# │  │ strategic_recs │ ← Actionable recommendations                            │
# │  └───────┬────────┘                                                         │
# │          │                                                                  │
# │          ▼                                                                  │
# │     📊 Analysis Report                                                      │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
model: gpt-4o-mini
workflow: competitor-analysis
description: "Automated competitive intelligence and analysis"

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────────────

inputs:
  your_company: "Nika AI"
  your_industry: "AI Workflow Automation"
  competitors:
    - name: "Zapier"
      website: "zapier.com"
    - name: "Make (Integromat)"
      website: "make.com"
    - name: "n8n"
      website: "n8n.io"
  analysis_focus:
    - "pricing model"
    - "target audience"
    - "key features"
    - "market positioning"
    - "competitive advantages"

# ─────────────────────────────────────────────────────────────────────────────
# TASKS
# ─────────────────────────────────────────────────────────────────────────────

tasks:
  - id: define_analysis
    description: "Set up the analysis framework"
    infer:
      prompt: |
        Create an analysis framework for competitive research.

        Our company: {{inputs.your_company}}
        Industry: {{inputs.your_industry}}
        Analysis focus areas: {{inputs.analysis_focus}}

        Generate a structured framework that includes:
        1. Key questions to answer for each competitor
        2. Metrics to compare
        3. Evaluation criteria

        Format as JSON:
        {
          "framework": {
            "questions": [...],
            "metrics": [...],
            "criteria": [...]
          }
        }
      temperature: 0.4
      max_tokens: 400

  - id: analyze_competitor
    description: "Deep dive analysis of each competitor"
    depends_on: [define_analysis]
    with:
      framework: $define_analysis
    for_each:
      - name: "Zapier"
        website: "zapier.com"
      - name: "Make (Integromat)"
        website: "make.com"
      - name: "n8n"
        website: "n8n.io"
    as: competitor
    concurrency: 3
    infer:
      prompt: |
        Perform a detailed competitive analysis of:

        Competitor: {{with.competitor.name}}
        Website: {{with.competitor.website}}

        Using this analysis framework:
        {{with.framework}}

        Analyze based on your knowledge (this is a demo - in production
        you would fetch real data):

        Provide:
        1. Company Overview
        2. Product/Service Analysis
        3. Pricing Model
        4. Target Audience
        5. Key Strengths (3-5)
        6. Key Weaknesses (3-5)
        7. Recent News/Developments
        8. Threat Level (Low/Medium/High)

        Be objective and analytical. Format as structured markdown.
      temperature: 0.5
      max_tokens: 800

  - id: swot_comparison
    description: "Create comparative SWOT analysis"
    depends_on: [analyze_competitor]
    with:
      analyses: $analyze_competitor
    infer:
      prompt: |
        Create a comparative SWOT analysis based on:

        {{with.analyses}}

        Our company: {{inputs.your_company}}

        Generate:

        1. **Competitive Positioning Map**
           - Where each competitor sits on price vs features

        2. **Feature Comparison Table**
           - Side-by-side comparison of key capabilities

        3. **SWOT for {{inputs.your_company}} vs Competition**
           - Strengths we have that others lack
           - Weaknesses we need to address
           - Opportunities in the market
           - Threats from competitors

        4. **Competitive Gaps**
           - What competitors offer that we don't
           - What we offer that competitors don't

        Use markdown tables and clear formatting.
      temperature: 0.4
      max_tokens: 1200

  - id: strategic_recs
    description: "Generate strategic recommendations"
    depends_on: [swot_comparison]
    with:
      swot: $swot_comparison
    infer:
      prompt: |
        Based on this competitive analysis:

        {{with.swot}}

        Generate strategic recommendations for {{inputs.your_company}}:

        ## 🎯 STRATEGIC RECOMMENDATIONS

        1. **Immediate Actions** (next 30 days)
           - Quick wins against competitors

        2. **Short-term Strategy** (90 days)
           - Feature priorities
           - Market positioning adjustments

        3. **Long-term Opportunities** (6-12 months)
           - Market gaps to exploit
           - Differentiation strategies

        4. **Risk Mitigation**
           - Competitive threats to monitor
           - Defensive strategies

        Be specific and actionable. Include reasoning for each recommendation.
      temperature: 0.6
      max_tokens: 1000
    artifact:
      path: "./output/competitor-analysis-{{date}}.md"
      format: text
"##;

/// Workflow 24: Email Composer
/// Write perfect emails in any tone
pub const WORKFLOW_24_EMAIL_COMPOSER: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# WORKFLOW 24: EMAIL COMPOSER ✉️
# ═══════════════════════════════════════════════════════════════════════════════
#
# Never struggle with email writing again! This workflow helps you
# compose the perfect email for any situation - from professional
# negotiations to friendly follow-ups.
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                         EMAIL COMPOSITION PIPELINE                         │
# │                                                                             │
# │  ┌─────────────────┐                                                        │
# │  │ analyze_context │ ← Understand the situation                             │
# │  └───────┬─────────┘                                                        │
# │          │                                                                  │
# │          ▼                                                                  │
# │  ┌─────────────────┐                                                        │
# │  │  draft_email    │ ← Generate initial draft                               │
# │  └───────┬─────────┘                                                        │
# │          │                                                                  │
# │          ▼                                                                  │
# │  ┌─────────────────┐                                                        │
# │  │  refine_tone    │ ← Perfect the tone & style                             │
# │  └───────┬─────────┘                                                        │
# │          │                                                                  │
# │          ▼                                                                  │
# │  ┌─────────────────┐                                                        │
# │  │ add_variations  │ ← Generate 2 alternate versions                        │
# │  └───────┬─────────┘                                                        │
# │          │                                                                  │
# │          ▼                                                                  │
# │      ✉️ Ready to Send                                                       │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
model: gpt-4o-mini
workflow: email-composer
description: "Compose perfect emails for any situation"

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────────────

inputs:
  email_type: "follow-up"  # follow-up, introduction, request, apology, thank-you, negotiation
  recipient_name: "Marie"
  recipient_role: "Engineering Manager"
  your_name: "Alex"
  context: "Following up on our interview last week for the Senior Engineer position"
  key_points:
    - "Thank them for their time"
    - "Reiterate interest in the role"
    - "Ask about next steps and timeline"
  tone: "professional but warm"
  length: "short"  # short, medium, long

# ─────────────────────────────────────────────────────────────────────────────
# TASKS
# ─────────────────────────────────────────────────────────────────────────────

tasks:
  - id: analyze_context
    description: "Analyze the email context and requirements"
    infer:
      prompt: |
        Analyze this email writing request:

        Type: {{inputs.email_type}}
        Recipient: {{inputs.recipient_name}} ({{inputs.recipient_role}})
        Sender: {{inputs.your_name}}
        Context: {{inputs.context}}
        Key points: {{inputs.key_points}}
        Tone: {{inputs.tone}}
        Length: {{inputs.length}}

        Provide analysis:
        1. What is the primary goal?
        2. What emotional response should this evoke?
        3. What are the do's and don'ts for this email type?
        4. Suggested subject lines (3 options)
        5. Key phrases to include
        6. Things to avoid

        Format as structured analysis.
      temperature: 0.4
      max_tokens: 500

  - id: draft_email
    description: "Create the initial email draft"
    depends_on: [analyze_context]
    with:
      analysis: $analyze_context
    infer:
      prompt: |
        Write an email based on this analysis:

        {{with.analysis}}

        Email details:
        - From: {{inputs.your_name}}
        - To: {{inputs.recipient_name}}
        - Type: {{inputs.email_type}}
        - Context: {{inputs.context}}
        - Points to cover: {{inputs.key_points}}
        - Tone: {{inputs.tone}}
        - Length: {{inputs.length}}

        Write the complete email including:
        - Subject line
        - Greeting
        - Body (covering all key points naturally)
        - Professional closing
        - Signature

        Make it sound natural, not AI-generated.
      temperature: 0.7
      max_tokens: 600

  - id: refine_tone
    description: "Perfect the tone and polish"
    depends_on: [draft_email]
    with:
      draft: $draft_email
    infer:
      prompt: |
        Review and refine this email draft:

        {{with.draft}}

        Desired tone: {{inputs.tone}}

        Improvements to make:
        1. Check that tone is consistent throughout
        2. Ensure clarity and conciseness
        3. Verify all key points are addressed
        4. Check for any awkward phrasing
        5. Ensure professional language
        6. Add personality without being unprofessional

        Return the polished version with improvements noted.
      temperature: 0.3
      max_tokens: 700

  - id: add_variations
    description: "Create alternative versions"
    depends_on: [refine_tone]
    with:
      polished: $refine_tone
    infer:
      prompt: |
        Based on this polished email:

        {{with.polished}}

        Create 2 alternative versions:

        **VERSION A - More Formal:**
        A slightly more formal version for conservative recipients.

        **VERSION B - More Casual:**
        A slightly warmer, more casual version for friendly contexts.

        Format:

        ═══════════════════════════════════════
        📧 ORIGINAL (RECOMMENDED)
        ═══════════════════════════════════════
        [polished email]

        ═══════════════════════════════════════
        📧 VERSION A - FORMAL
        ═══════════════════════════════════════
        [formal version]

        ═══════════════════════════════════════
        📧 VERSION B - CASUAL
        ═══════════════════════════════════════
        [casual version]

        End with quick tips for each version.
      temperature: 0.6
      max_tokens: 1200
    artifact:
      path: "./output/email-{{date}}-{{timestamp}}.md"
      format: text
"##;

/// Workflow 25: Recipe & Meal Planner
/// Weekly menu with grocery list generation
pub const WORKFLOW_25_MEAL_PLANNER: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# WORKFLOW 25: RECIPE & MEAL PLANNER 🍳
# ═══════════════════════════════════════════════════════════════════════════════
#
# Your AI sous-chef! Plans a full week of meals with recipes
# and generates a consolidated grocery list.
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                         MEAL PLANNING PIPELINE                             │
# │                                                                             │
# │  ┌──────────────────┐                                                       │
# │  │ analyze_preferences │ ← Dietary needs & preferences                      │
# │  └─────────┬────────┘                                                       │
# │            │                                                                │
# │            ▼                                                                │
# │  ┌──────────────────┐                                                       │
# │  │   plan_menu      │ ← Create weekly menu                                  │
# │  └─────────┬────────┘                                                       │
# │            │                                                                │
# │            ▼                                                                │
# │  ┌───────────────────────────────────────────────────────────────┐         │
# │  │              FOR EACH: day of the week                         │         │
# │  │   ┌──────────────────────────────────────────────────────┐    │         │
# │  │   │         generate_recipes (7 days parallel)           │    │         │
# │  │   │         Breakfast | Lunch | Dinner                   │    │         │
# │  │   └──────────────────────────────────────────────────────┘    │         │
# │  └───────────────────────────────────────────────────────────────┘         │
# │            │                                                                │
# │            ▼                                                                │
# │  ┌──────────────────┐                                                       │
# │  │ generate_grocery │ ← Consolidated shopping list                          │
# │  └─────────┬────────┘                                                       │
# │            │                                                                │
# │            ▼                                                                │
# │       🛒 Ready to Shop                                                      │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
model: gpt-4o-mini
workflow: meal-planner
description: "Plan a week of meals with recipes and grocery list"

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────────────

inputs:
  servings: 2
  dietary_restrictions: ["vegetarian"]  # vegetarian, vegan, gluten-free, dairy-free, etc.
  cuisine_preferences: ["Mediterranean", "Asian", "Mexican"]
  cooking_skill: "intermediate"  # beginner, intermediate, advanced
  max_prep_time: "45 minutes"
  budget: "moderate"  # budget-friendly, moderate, no-limit

# ─────────────────────────────────────────────────────────────────────────────
# TASKS
# ─────────────────────────────────────────────────────────────────────────────

tasks:
  - id: analyze_preferences
    description: "Analyze dietary needs and preferences"
    infer:
      prompt: |
        Analyze these meal planning requirements:

        - Servings: {{inputs.servings}} people
        - Dietary restrictions: {{inputs.dietary_restrictions}}
        - Cuisine preferences: {{inputs.cuisine_preferences}}
        - Cooking skill level: {{inputs.cooking_skill}}
        - Max prep time: {{inputs.max_prep_time}}
        - Budget: {{inputs.budget}}

        Generate guidelines for the meal plan:
        1. What ingredients to emphasize
        2. What to avoid
        3. Protein sources to use
        4. Complexity appropriate for skill level
        5. Budget-appropriate ingredient choices
        6. Variety across the week
      temperature: 0.4
      max_tokens: 400

  - id: plan_menu
    description: "Create the weekly menu overview"
    depends_on: [analyze_preferences]
    with:
      guidelines: $analyze_preferences
    infer:
      prompt: |
        Create a weekly meal plan following these guidelines:

        {{with.guidelines}}

        For each day (Monday-Sunday), suggest:
        - 🌅 Breakfast (quick, 10-15 min)
        - ☀️ Lunch (can be leftovers or quick)
        - 🌙 Dinner (main meal, up to {{inputs.max_prep_time}})

        Cuisines to include: {{inputs.cuisine_preferences}}

        Return as JSON array:
        [
          {
            "day": "Monday",
            "breakfast": {"name": "...", "description": "..."},
            "lunch": {"name": "...", "description": "..."},
            "dinner": {"name": "...", "description": "..."}
          },
          ...
        ]

        Ensure variety and plan for using leftovers smartly.
      temperature: 0.7
      max_tokens: 800
    output:
      schema:
        type: array
        minItems: 7
        maxItems: 7

  - id: generate_recipes
    description: "Generate detailed recipes for each day"
    depends_on: [plan_menu]
    with:
      menu: $plan_menu
    for_each: ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
    as: day
    concurrency: 7
    infer:
      prompt: |
        Generate detailed recipes for {{with.day}} based on this menu:

        {{with.menu}}

        Find {{with.day}}'s meals and create recipes with:

        For EACH meal (breakfast, lunch, dinner):
        - 🍽️ Recipe name
        - ⏱️ Prep time + Cook time
        - 📝 Ingredients list with exact quantities (for {{inputs.servings}} servings)
        - 📋 Step-by-step instructions
        - 💡 Tips & variations
        - 🥗 Nutritional highlights

        Cooking skill level: {{inputs.cooking_skill}}

        Format as clear, easy-to-follow recipes.
      temperature: 0.6
      max_tokens: 1200

  - id: generate_grocery
    description: "Create consolidated grocery list"
    depends_on: [generate_recipes]
    with:
      recipes: $generate_recipes
    infer:
      prompt: |
        Create a consolidated grocery list from these recipes:

        {{with.recipes}}

        Servings: {{inputs.servings}}

        Organize the list by store section:

        🥬 PRODUCE
        - Item (quantity)

        🥛 DAIRY & EGGS
        - Item (quantity)

        🥫 PANTRY
        - Item (quantity)

        ❄️ FROZEN
        - Item (quantity)

        🌿 HERBS & SPICES
        - Item (quantity)

        🍞 BAKERY
        - Item (quantity)

        For each item:
        - Consolidate quantities across all recipes
        - Note which recipes use it
        - Flag items you might already have (common pantry staples)

        End with:
        - 💰 Estimated total cost
        - 🏪 Shopping tips
        - 📦 Meal prep suggestions for the week
      temperature: 0.3
      max_tokens: 1500
    artifact:
      path: "./output/meal-plan-{{date}}.md"
      format: text
"##;

/// Workflow 26: Travel Planner
/// Complete trip itinerary generation
pub const WORKFLOW_26_TRAVEL_PLANNER: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# WORKFLOW 26: TRAVEL PLANNER ✈️
# ═══════════════════════════════════════════════════════════════════════════════
#
# Plan your perfect trip! This workflow creates a comprehensive
# travel itinerary with activities, logistics, and local tips.
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                          TRAVEL PLANNING FLOW                               │
# │                                                                             │
# │  ┌────────────────┐                                                         │
# │  │ analyze_trip   │ ← Trip type, interests, constraints                     │
# │  └───────┬────────┘                                                         │
# │          │                                                                  │
# │          ▼                                                                  │
# │  ┌────────────────┐    ┌────────────────┐                                   │
# │  │research_dest   │    │ local_insights │                                   │
# │  │  (destination  │    │ (culture, tips │                                   │
# │  │   overview)    │    │   customs)     │                                   │
# │  └───────┬────────┘    └───────┬────────┘                                   │
# │          │                     │                                            │
# │          └──────────┬──────────┘                                            │
# │                     │                                                       │
# │                     ▼                                                       │
# │          ┌────────────────┐                                                 │
# │          │ create_itinerary│ ← Day-by-day plan                              │
# │          └───────┬────────┘                                                 │
# │                  │                                                          │
# │                  ▼                                                          │
# │          ┌────────────────┐                                                 │
# │          │ add_logistics  │ ← Packing, budget, checklist                    │
# │          └───────┬────────┘                                                 │
# │                  │                                                          │
# │                  ▼                                                          │
# │              🗺️ Trip Ready                                                  │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
model: gpt-4o-mini
workflow: travel-planner
description: "Create a comprehensive travel itinerary"

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────────────

inputs:
  destination: "Tokyo, Japan"
  trip_duration: "7 days"
  trip_type: "cultural exploration"  # adventure, relaxation, cultural, foodie, romantic
  travelers: 2
  travel_style: "mid-range"  # budget, mid-range, luxury
  interests:
    - "history and temples"
    - "local cuisine"
    - "anime and pop culture"
    - "nature and gardens"
  mobility: "good walking ability"
  departure_city: "Paris"

# ─────────────────────────────────────────────────────────────────────────────
# TASKS
# ─────────────────────────────────────────────────────────────────────────────

tasks:
  - id: analyze_trip
    description: "Analyze trip requirements"
    infer:
      prompt: |
        Analyze this trip request:

        - Destination: {{inputs.destination}}
        - Duration: {{inputs.trip_duration}}
        - Trip type: {{inputs.trip_type}}
        - Travelers: {{inputs.travelers}}
        - Style: {{inputs.travel_style}}
        - Interests: {{inputs.interests}}
        - Mobility: {{inputs.mobility}}
        - Departing from: {{inputs.departure_city}}

        Provide:
        1. Best time to do specific activities
        2. Key areas/neighborhoods to focus on
        3. Must-see vs nice-to-see prioritization
        4. Pace recommendations (relaxed vs packed)
        5. Budget considerations
      temperature: 0.5
      max_tokens: 500

  # Parallel research phase
  - id: research_dest
    description: "Research destination highlights"
    depends_on: [analyze_trip]
    with:
      analysis: $analyze_trip
    infer:
      prompt: |
        Research {{inputs.destination}} for a {{inputs.trip_duration}} trip.

        Trip focus:
        {{with.analysis}}

        Cover:

        🏛️ TOP ATTRACTIONS
        - Must-see spots for {{inputs.interests}}
        - Best time to visit each
        - Crowd levels

        🍜 FOOD SCENE
        - Local specialties to try
        - Recommended restaurants/areas
        - Food experiences (markets, tours)

        🚇 TRANSPORTATION
        - Getting around efficiently
        - Passes and tickets to buy
        - Airport transfer options

        🏨 NEIGHBORHOODS
        - Best areas to stay
        - Pros/cons of each
        - Budget considerations
      temperature: 0.6
      max_tokens: 1000

  - id: local_insights
    description: "Gather cultural tips and local knowledge"
    depends_on: [analyze_trip]
    with:
      analysis: $analyze_trip
    infer:
      prompt: |
        Provide local insights for {{inputs.destination}}:

        🎎 CULTURAL ETIQUETTE
        - Important customs to know
        - Do's and don'ts
        - Common mistakes tourists make

        💬 ESSENTIAL PHRASES
        - Key phrases in local language
        - Numbers and basic communication

        💴 MONEY MATTERS
        - Currency tips
        - Tipping customs
        - Payment methods accepted

        🌡️ WEATHER & SEASONS
        - What to expect
        - How to dress

        📱 PRACTICAL TIPS
        - Apps to download
        - SIM/internet options
        - Safety notes

        🎁 UNIQUE EXPERIENCES
        - Hidden gems locals know
        - Off-the-beaten-path suggestions
      temperature: 0.6
      max_tokens: 800

  - id: create_itinerary
    description: "Build day-by-day itinerary"
    depends_on: [research_dest, local_insights]
    with:
      research: $research_dest
      insights: $local_insights
    infer:
      prompt: |
        Create a detailed {{inputs.trip_duration}} itinerary for {{inputs.destination}}.

        Research:
        {{with.research}}

        Local Insights:
        {{with.insights}}

        Focus on: {{inputs.interests}}
        Travel style: {{inputs.travel_style}}

        For EACH day, provide:

        📅 DAY [N]: [Theme/Area]

        Morning:
        - Activity + location
        - Time: X:XX - X:XX
        - Tips & logistics

        Lunch:
        - Restaurant recommendation
        - Cuisine type
        - Price range

        Afternoon:
        - Activity + location
        - Time: X:XX - X:XX
        - Tips & logistics

        Evening:
        - Dinner spot
        - Activity (if applicable)
        - Nightlife suggestions (optional)

        🚇 Transportation notes for the day
        💰 Estimated daily budget

        Balance busy and relaxed days. Account for jet lag on first days.
      temperature: 0.7
      max_tokens: 2000

  - id: add_logistics
    description: "Add packing list, budget, and checklists"
    depends_on: [create_itinerary, local_insights]
    with:
      itinerary: $create_itinerary
      insights: $local_insights
    infer:
      prompt: |
        Complete the travel planning with logistics:

        Itinerary:
        {{with.itinerary}}

        Local insights:
        {{with.insights}}

        Generate:

        🧳 PACKING LIST
        - Essentials
        - Electronics
        - Clothing (weather-appropriate)
        - Documents
        - Toiletries
        - Nice-to-haves

        💰 BUDGET BREAKDOWN
        - Flights (estimate)
        - Accommodation ({{inputs.trip_duration}})
        - Food (daily average)
        - Activities & attractions
        - Transportation
        - Shopping/souvenirs
        - Emergency fund
        - TOTAL ESTIMATE

        ✅ PRE-TRIP CHECKLIST
        - [ ] Documents/visas
        - [ ] Bookings confirmation
        - [ ] Currency/cards
        - [ ] Insurance
        - [ ] Medications
        - [ ] Copies of documents

        📱 RECOMMENDED APPS
        - Maps/navigation
        - Translation
        - Transport
        - Reservations

        Compile everything into a printable travel guide format.
      temperature: 0.4
      max_tokens: 1500
    artifact:
      path: "./output/trip-{{date}}-tokyo.md"
      format: text
"##;

/// Workflow 27: Birthday Party Planner
/// Complete party planning kit
pub const WORKFLOW_27_PARTY_PLANNER: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# WORKFLOW 27: BIRTHDAY PARTY PLANNER 🎂
# ═══════════════════════════════════════════════════════════════════════════════
#
# Plan the perfect party! From theme ideas to shopping lists,
# this workflow handles all the party planning details.
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                         PARTY PLANNING PIPELINE                            │
# │                                                                             │
# │  ┌──────────────────┐                                                       │
# │  │  analyze_party   │ ← Who, what, where, when                              │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ▼                                                                 │
# │  ┌──────────────────┐                                                       │
# │  │  generate_theme  │ ← Theme ideas and styling                             │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ├─────────────────┬─────────────────┐                             │
# │           ▼                 ▼                 ▼                             │
# │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                       │
# │  │ plan_food    │  │plan_activities│ │plan_decor    │                       │
# │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘                       │
# │         │                 │                 │                               │
# │         └─────────────────┼─────────────────┘                               │
# │                           │                                                 │
# │                           ▼                                                 │
# │                ┌──────────────────┐                                         │
# │                │ compile_party_kit │ ← Everything organized                  │
# │                └──────────┬───────┘                                         │
# │                           │                                                 │
# │                           ▼                                                 │
# │                       🎉 Party Kit                                          │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
model: gpt-4o-mini
workflow: party-planner
description: "Plan a complete birthday party"

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────────────

inputs:
  honoree_name: "Emma"
  age: 8
  guest_count: 15
  venue_type: "backyard"  # home, backyard, park, venue
  duration_hours: 3
  budget: 200
  dietary_considerations: ["nut-free"]
  season: "spring"
  special_interests: ["unicorns", "arts and crafts", "dancing"]

# ─────────────────────────────────────────────────────────────────────────────
# TASKS
# ─────────────────────────────────────────────────────────────────────────────

tasks:
  - id: analyze_party
    description: "Analyze party requirements"
    infer:
      prompt: |
        Analyze this birthday party request:

        🎂 PARTY DETAILS
        - Honoree: {{inputs.honoree_name}}, turning {{inputs.age}}
        - Guests: {{inputs.guest_count}} kids
        - Venue: {{inputs.venue_type}}
        - Duration: {{inputs.duration_hours}} hours
        - Budget: ${{inputs.budget}}
        - Season: {{inputs.season}}
        - Dietary: {{inputs.dietary_considerations}}
        - Interests: {{inputs.special_interests}}

        Provide planning considerations:
        1. Age-appropriate activity level
        2. Time structure (games, food, cake)
        3. Budget allocation recommendations
        4. Weather/venue considerations
        5. Safety notes for age group
      temperature: 0.5
      max_tokens: 400

  - id: generate_theme
    description: "Create theme ideas"
    depends_on: [analyze_party]
    with:
      analysis: $analyze_party
    infer:
      prompt: |
        Generate party theme ideas for:
        {{with.analysis}}

        Interests: {{inputs.special_interests}}

        Create 3 theme options:

        For each theme:
        🎨 THEME: [Name]

        - Color palette
        - Key visual elements
        - Invitation style
        - Outfit suggestions (optional)
        - How it incorporates interests

        Rank by feasibility for budget/venue.

        RECOMMENDED THEME:
        [Pick the best one and explain why]
      temperature: 0.8
      max_tokens: 600

  # Parallel planning phase
  - id: plan_food
    description: "Plan food and treats"
    depends_on: [generate_theme]
    with:
      theme: $generate_theme
    infer:
      prompt: |
        Plan party food matching this theme:
        {{with.theme}}

        Requirements:
        - Guests: {{inputs.guest_count}} kids
        - Dietary: {{inputs.dietary_considerations}}
        - Budget allocation: ~30% of ${{inputs.budget}}

        Plan:

        🍕 MAIN FOOD
        - Options (simple, kid-friendly)
        - Quantities needed

        🎂 BIRTHDAY CAKE
        - Theme-matching design
        - DIY or order recommendations

        🍬 TREATS & SNACKS
        - Theme-coordinated options
        - Healthy options mixed in

        🥤 DRINKS
        - Options and quantities

        🛒 SHOPPING LIST
        - Itemized with estimates

        💰 FOOD BUDGET: $XX / ${{inputs.budget}}
      temperature: 0.6
      max_tokens: 700

  - id: plan_activities
    description: "Plan games and activities"
    depends_on: [generate_theme]
    with:
      theme: $generate_theme
    infer:
      prompt: |
        Plan party activities for:
        {{with.theme}}

        Details:
        - Age: {{inputs.age}} year olds
        - Guests: {{inputs.guest_count}}
        - Duration: {{inputs.duration_hours}} hours
        - Venue: {{inputs.venue_type}}

        Create timeline:

        📋 PARTY SCHEDULE

        [TIME] Arrival & Free Play (15 min)
        - Activity while guests arrive

        [TIME] Game 1 (20 min)
        - Instructions
        - Materials needed

        [TIME] Game 2 (20 min)
        - Instructions
        - Materials needed

        [TIME] Craft Activity (30 min)
        - Theme-matching craft
        - Take-home item

        [TIME] Food Time (30 min)

        [TIME] Cake & Presents (30 min)

        [TIME] Final Game (15 min)
        - Calm-down activity

        [TIME] Goody Bags & Goodbye

        🎁 GOODY BAG IDEAS
        - Theme-matching items
        - Budget-friendly options
      temperature: 0.7
      max_tokens: 800

  - id: plan_decor
    description: "Plan decorations"
    depends_on: [generate_theme]
    with:
      theme: $generate_theme
    infer:
      prompt: |
        Plan decorations for:
        {{with.theme}}

        Venue: {{inputs.venue_type}}
        Budget allocation: ~25% of ${{inputs.budget}}

        🎈 DECORATIONS

        MUST-HAVES:
        - Balloons (colors, quantities)
        - Banner/signage
        - Table decorations

        NICE-TO-HAVES:
        - Backdrop for photos
        - Themed centerpieces
        - Entrance decoration

        🎉 PARTY SUPPLIES
        - Plates, cups, napkins (themed?)
        - Tablecloths
        - Utensils

        DIY vs BUY:
        - What to make (save money)
        - What to buy (save time)

        🛒 DECORATION SHOPPING LIST
        - Items with prices
        - Where to buy

        💰 DECOR BUDGET: $XX / ${{inputs.budget}}
      temperature: 0.6
      max_tokens: 600

  - id: compile_party_kit
    description: "Compile complete party planning kit"
    depends_on: [generate_theme, plan_food, plan_activities, plan_decor]
    with:
      theme: $generate_theme
      food: $plan_food
      activities: $plan_activities
      decor: $plan_decor
    infer:
      prompt: |
        Compile the complete party planning kit:

        THEME: {{with.theme}}
        FOOD: {{with.food}}
        ACTIVITIES: {{with.activities}}
        DECORATIONS: {{with.decor}}

        Create:

        # 🎂 {{inputs.honoree_name}}'s {{inputs.age}}th Birthday Party Kit

        ## 📋 MASTER CHECKLIST

        ### 2 Weeks Before
        - [ ] Send invitations
        - [ ] Order cake
        - [ ] Plan menu

        ### 1 Week Before
        - [ ] Buy decorations
        - [ ] Gather activity supplies
        - [ ] Confirm RSVPs

        ### 2 Days Before
        - [ ] Grocery shopping
        - [ ] Prepare goody bags
        - [ ] Prep any crafts

        ### Day Before
        - [ ] Decorate venue
        - [ ] Set up activity stations
        - [ ] Charge cameras!

        ### Day Of
        - [ ] Final food prep
        - [ ] Set up food table
        - [ ] Pick up cake
        - [ ] HAVE FUN! 🎉

        ## 🛒 COMPLETE SHOPPING LIST
        [Consolidated from all plans]

        ## 💰 BUDGET SUMMARY
        - Food: $XX
        - Decorations: $XX
        - Activities: $XX
        - Goody bags: $XX
        - Contingency: $XX
        - TOTAL: $XX / ${{inputs.budget}}

        ## 📞 IMPORTANT NOTES
        - Emergency contacts
        - Allergy management
        - Backup plan for weather
      temperature: 0.4
      max_tokens: 1500
    artifact:
      path: "./output/party-plan-{{date}}.md"
      format: text
"##;

/// Workflow 28: Podcast Show Notes
/// Convert transcripts to full content packages
pub const WORKFLOW_28_PODCAST_NOTES: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# WORKFLOW 28: PODCAST SHOW NOTES 🎙️
# ═══════════════════════════════════════════════════════════════════════════════
#
# Transform podcast transcripts into comprehensive show notes,
# timestamps, social clips, and SEO content.
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                       PODCAST CONTENT PIPELINE                              │
# │                                                                             │
# │  ┌──────────────────┐                                                       │
# │  │ analyze_episode  │ ← Structure and key topics                            │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ├─────────────────┬─────────────────┐                             │
# │           ▼                 ▼                 ▼                             │
# │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐                       │
# │  │create_notes  │  │ find_clips   │  │generate_seo  │                       │
# │  │ (timestamps, │  │ (quotable    │  │ (description │                       │
# │  │  summary)    │  │  moments)    │  │  keywords)   │                       │
# │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘                       │
# │         │                 │                 │                               │
# │         └─────────────────┼─────────────────┘                               │
# │                           │                                                 │
# │                           ▼                                                 │
# │                ┌──────────────────┐                                         │
# │                │ generate_social  │ ← Social posts + threads                │
# │                └──────────┬───────┘                                         │
# │                           │                                                 │
# │                           ▼                                                 │
# │                ┌──────────────────┐                                         │
# │                │  compile_package │ ← Full content package                   │
# │                └──────────────────┘                                         │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
model: gpt-4o-mini
workflow: podcast-show-notes
description: "Convert podcast transcript to full content package"

# ─────────────────────────────────────────────────────────────────────────────
# CONTEXT - Load transcript from file
# ─────────────────────────────────────────────────────────────────────────────

context:
  files:
    transcript: "./podcast-transcript.txt"  # Your transcript file

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────────────

inputs:
  show_name: "Tech Talks Weekly"
  episode_number: 42
  host_name: "Alex Chen"
  guest_name: "Dr. Sarah Johnson"
  guest_title: "AI Research Lead at TechCorp"
  episode_date: "2024-03-15"
  platforms: ["YouTube", "Spotify", "Apple Podcasts"]

# ─────────────────────────────────────────────────────────────────────────────
# TASKS
# ─────────────────────────────────────────────────────────────────────────────

tasks:
  - id: analyze_episode
    description: "Analyze transcript structure and topics"
    infer:
      prompt: |
        Analyze this podcast transcript:

        Show: {{inputs.show_name}} - Episode {{inputs.episode_number}}
        Host: {{inputs.host_name}}
        Guest: {{inputs.guest_name}} ({{inputs.guest_title}})

        Transcript:
        {{context.files.transcript}}

        Identify:
        1. Main topics discussed (in order)
        2. Key insights and takeaways
        3. Memorable quotes
        4. Discussion segments with approximate timestamps
        5. Questions answered
        6. Resources/books/people mentioned
        7. Call-to-actions mentioned

        Format as structured analysis.
      temperature: 0.4
      max_tokens: 800

  # Parallel content generation
  - id: create_notes
    description: "Create show notes with timestamps"
    depends_on: [analyze_episode]
    with:
      analysis: $analyze_episode
    infer:
      prompt: |
        Create detailed show notes from:
        {{with.analysis}}

        Format:

        # Episode {{inputs.episode_number}}: [Generated Title]

        ## 🎯 Episode Summary
        (2-3 sentences capturing the essence)

        ## 👤 Guest Bio
        {{inputs.guest_name}} - {{inputs.guest_title}}
        (Expanded bio from conversation context)

        ## ⏱️ Timestamps
        [00:00] Introduction
        [MM:SS] Topic 1
        [MM:SS] Topic 2
        ... (all key moments)

        ## 📝 Key Takeaways
        1. ...
        2. ...
        3. ...

        ## 💡 Notable Quotes
        > "Quote 1" - Speaker
        > "Quote 2" - Speaker

        ## 📚 Resources Mentioned
        - Books
        - Tools
        - Links

        ## 🔗 Connect
        - Guest social links (placeholder)
        - Show links
      temperature: 0.4
      max_tokens: 1000

  - id: find_clips
    description: "Identify quotable moments for clips"
    depends_on: [analyze_episode]
    with:
      analysis: $analyze_episode
    infer:
      prompt: |
        From this analysis:
        {{with.analysis}}

        Find the 5 best moments for short video/audio clips:

        For each clip:

        🎬 CLIP [N]: "[Hook Title]"
        - Timestamp: [MM:SS - MM:SS]
        - Quote: "The exact words..."
        - Why it works: [Emotional? Surprising? Actionable?]
        - Best platform: [TikTok/Reels/LinkedIn/Twitter]
        - Suggested caption: [One-liner for social]

        Prioritize:
        1. Emotional peaks
        2. Surprising insights
        3. Actionable advice
        4. Controversial takes
        5. Inspirational moments
      temperature: 0.6
      max_tokens: 800

  - id: generate_seo
    description: "Generate SEO content"
    depends_on: [analyze_episode]
    with:
      analysis: $analyze_episode
    infer:
      prompt: |
        Generate SEO content for:
        {{with.analysis}}

        Show: {{inputs.show_name}}
        Episode: {{inputs.episode_number}}
        Guest: {{inputs.guest_name}}

        Create:

        📺 YOUTUBE TITLE OPTIONS (3)
        - Curiosity-driven
        - Keyword-rich
        - Number-focused

        📝 YOUTUBE DESCRIPTION
        (First 150 chars are crucial - front-load keywords)
        Full description with timestamps, links, hashtags.

        🏷️ TAGS/KEYWORDS
        - Primary keywords (5)
        - Long-tail keywords (10)
        - Related topics (5)

        🎵 SPOTIFY/APPLE DESCRIPTION
        (Optimized for podcast apps)

        🔍 BLOG POST SEO TITLE
        (For website embed)

        📊 META DESCRIPTION
        (155 characters max)
      temperature: 0.5
      max_tokens: 800

  - id: generate_social
    description: "Create social media content"
    depends_on: [find_clips, create_notes]
    with:
      clips: $find_clips
      notes: $create_notes
    infer:
      prompt: |
        Create social media content:

        Clips identified:
        {{with.clips}}

        Show notes:
        {{with.notes}}

        Generate:

        🐦 TWITTER/X THREAD (5-7 tweets)
        1/ Hook tweet (engagement driver)
        2-6/ Key insights
        7/ CTA + link

        📘 LINKEDIN POST
        - Professional angle
        - Tag guest
        - Insight summary
        - CTA

        📸 INSTAGRAM
        - Carousel concept (5-7 slides)
        - Caption
        - Hashtags (20)

        📺 YOUTUBE COMMUNITY POST
        - Engagement question
        - Key quote image text

        🎵 TIKTOK/REELS SCRIPTS
        (For each clip, 15-30 second scripts)
      temperature: 0.7
      max_tokens: 1200

  - id: compile_package
    description: "Compile complete content package"
    depends_on: [create_notes, find_clips, generate_seo, generate_social]
    with:
      notes: $create_notes
      clips: $find_clips
      seo: $generate_seo
      social: $generate_social
    infer:
      prompt: |
        Compile the complete podcast content package:

        # 🎙️ {{inputs.show_name}} - Episode {{inputs.episode_number}}
        # Content Package

        Date: {{inputs.episode_date}}
        Guest: {{inputs.guest_name}}

        ---

        ## 📋 SHOW NOTES
        {{with.notes}}

        ---

        ## 🎬 VIDEO CLIPS
        {{with.clips}}

        ---

        ## 🔍 SEO CONTENT
        {{with.seo}}

        ---

        ## 📱 SOCIAL MEDIA
        {{with.social}}

        ---

        ## ✅ PUBLISHING CHECKLIST

        ### Upload
        - [ ] YouTube (use title option, description, tags)
        - [ ] Spotify (description, keywords)
        - [ ] Apple Podcasts (description)
        - [ ] Website embed + blog post

        ### Social
        - [ ] Post Twitter thread
        - [ ] Post LinkedIn
        - [ ] Schedule Instagram carousel
        - [ ] YouTube community post
        - [ ] Newsletter mention

        ### Clips
        - [ ] Create clip 1 (platform: X)
        - [ ] Create clip 2 (platform: X)
        ...

        ### Follow-up
        - [ ] Thank guest
        - [ ] Share analytics after 1 week
        - [ ] Repurpose best performer

        ---

        Generated by Nika 🦋
      temperature: 0.3
      max_tokens: 2000
    artifact:
      path: "./output/podcast-ep{{inputs.episode_number}}-{{date}}.md"
      format: text
"##;

/// Workflow 29: Product Review Analyzer
/// "Should I buy this?" decision helper
pub const WORKFLOW_29_PRODUCT_REVIEW: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# WORKFLOW 29: PRODUCT REVIEW ANALYZER 🛒
# ═══════════════════════════════════════════════════════════════════════════════
#
# Make better buying decisions! This workflow analyzes product
# reviews to give you an unbiased verdict.
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                      PRODUCT REVIEW ANALYSIS FLOW                          │
# │                                                                             │
# │  ┌──────────────────┐                                                       │
# │  │ gather_context   │ ← Product details and use case                        │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ▼                                                                 │
# │  ┌────────────────────────────────────────────────────────────────┐        │
# │  │              FOR EACH: review_source                            │        │
# │  │   ┌──────────────────────────────────────────────────────┐     │        │
# │  │   │         analyze_reviews (Amazon, Reddit, YouTube)    │     │        │
# │  │   └──────────────────────────────────────────────────────┘     │        │
# │  └────────────────────────────────────────────────────────────────┘        │
# │           │                                                                 │
# │           ▼                                                                 │
# │  ┌──────────────────┐                                                       │
# │  │ synthesize_verdict│ ← Weighted analysis                                  │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ▼                                                                 │
# │  ┌──────────────────┐                                                       │
# │  │find_alternatives │ ← Better options?                                     │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ▼                                                                 │
# │      ✅ Buy / ❌ Skip                                                        │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
model: gpt-4o-mini
workflow: product-review-analyzer
description: "Analyze reviews to make better buying decisions"

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────────────

inputs:
  product_name: "Sony WH-1000XM5 Headphones"
  product_category: "wireless noise-canceling headphones"
  price: 400
  your_priorities:
    - "noise cancellation quality"
    - "comfort for long wear"
    - "sound quality"
    - "battery life"
  deal_breakers:
    - "connection issues"
    - "durability problems"
  use_case: "Daily commute and home office focus work"

# ─────────────────────────────────────────────────────────────────────────────
# TASKS
# ─────────────────────────────────────────────────────────────────────────────

tasks:
  - id: gather_context
    description: "Understand product and your needs"
    infer:
      prompt: |
        Analyze this purchase consideration:

        Product: {{inputs.product_name}}
        Category: {{inputs.product_category}}
        Price: ${{inputs.price}}

        Your priorities (in order):
        {{inputs.your_priorities}}

        Deal-breakers:
        {{inputs.deal_breakers}}

        Use case: {{inputs.use_case}}

        Generate:
        1. Key questions to answer from reviews
        2. Features to verify
        3. Red flags to watch for
        4. Comparison criteria
        5. Value assessment framework
      temperature: 0.4
      max_tokens: 500

  - id: analyze_reviews
    description: "Analyze reviews from multiple sources"
    depends_on: [gather_context]
    with:
      context: $gather_context
    for_each: ["Amazon Reviews", "Reddit Discussions", "YouTube Reviews"]
    as: source
    concurrency: 3
    infer:
      prompt: |
        Simulate analyzing {{inputs.product_name}} reviews from {{with.source}}.

        Analysis context:
        {{with.context}}

        Note: In production, this would fetch real reviews via API.
        For demo, simulate a realistic analysis based on your knowledge.

        For {{with.source}}, provide:

        📊 OVERVIEW
        - Typical sentiment (positive/mixed/negative)
        - Sample size quality
        - Reviewer credibility

        ✅ COMMONLY PRAISED
        - Feature 1: [praise + frequency]
        - Feature 2: [praise + frequency]
        - Feature 3: [praise + frequency]

        ❌ COMMON COMPLAINTS
        - Issue 1: [complaint + severity + frequency]
        - Issue 2: [complaint + severity + frequency]

        💡 UNIQUE INSIGHTS
        - Things only {{with.source}} reveals

        🎯 RELEVANCE TO YOUR PRIORITIES
        - How well it addresses {{inputs.your_priorities}}
        - Any deal-breaker hits? {{inputs.deal_breakers}}

        📈 CONFIDENCE SCORE: X/10
      temperature: 0.6
      max_tokens: 700

  - id: synthesize_verdict
    description: "Create weighted verdict"
    depends_on: [analyze_reviews]
    with:
      analyses: $analyze_reviews
    infer:
      prompt: |
        Synthesize all review analyses into a verdict:

        {{with.analyses}}

        Your priorities: {{inputs.your_priorities}}
        Your deal-breakers: {{inputs.deal_breakers}}
        Price: ${{inputs.price}}
        Use case: {{inputs.use_case}}

        Create:

        # 📊 REVIEW SYNTHESIS: {{inputs.product_name}}

        ## Consensus Summary
        - What most reviewers agree on
        - Where opinions diverge

        ## Priority Analysis
        For each of your priorities, score 1-10:

        | Priority | Score | Evidence |
        |----------|-------|----------|
        | Priority 1 | X/10 | Why |
        ...

        ## Deal-Breaker Check
        ✅ or ❌ for each deal-breaker

        ## Value Assessment
        - Price vs competitors
        - Cost per use calculation
        - Longevity expectations

        ## Confidence Level
        How confident are we in this analysis? (High/Medium/Low)
        Based on: review volume, consistency, recency
      temperature: 0.4
      max_tokens: 800

  - id: find_alternatives
    description: "Identify alternatives to consider"
    depends_on: [synthesize_verdict]
    with:
      verdict: $synthesize_verdict
    infer:
      prompt: |
        Based on this analysis:
        {{with.verdict}}

        Priorities: {{inputs.your_priorities}}
        Budget: ${{inputs.price}} (willing to go ±30%)

        Suggest alternatives:

        ## 🔄 ALTERNATIVES TO CONSIDER

        ### If budget allows more ($+30%):
        - Product: [name]
        - Why: [how it addresses priorities better]
        - Price: $X

        ### Similar price point:
        - Product: [name]
        - Why: [trade-offs vs {{inputs.product_name}}]
        - Price: $X

        ### Budget-friendly ($-30%):
        - Product: [name]
        - Why: [what you sacrifice, what you keep]
        - Price: $X

        ## Quick Comparison Table
        | Feature | {{inputs.product_name}} | Alt 1 | Alt 2 |
        |---------|------------------------|-------|-------|
        | Priority 1 | X/10 | X/10 | X/10 |
        ...
      temperature: 0.5
      max_tokens: 700

  - id: final_recommendation
    description: "Generate final buy/skip recommendation"
    depends_on: [synthesize_verdict, find_alternatives]
    with:
      verdict: $synthesize_verdict
      alternatives: $find_alternatives
    infer:
      prompt: |
        Generate final recommendation:

        Verdict:
        {{with.verdict}}

        Alternatives:
        {{with.alternatives}}

        Use case: {{inputs.use_case}}

        # 🎯 FINAL VERDICT: {{inputs.product_name}}

        ## The Bottom Line

        ### 🟢 BUY IF:
        - Condition 1
        - Condition 2
        - Condition 3

        ### 🔴 SKIP IF:
        - Condition 1
        - Condition 2
        - Condition 3

        ### 🟡 WAIT IF:
        - Condition 1 (e.g., sale expected)

        ## For YOUR Use Case ({{inputs.use_case}}):

        **RECOMMENDATION:** [BUY ✅ / SKIP ❌ / CONSIDER ALTERNATIVE ⚠️]

        **Confidence:** [High/Medium/Low]

        **Reasoning:**
        [2-3 sentences explaining why this is right for YOU]

        **Action:**
        If buying: [where to buy, what to look for]
        If skipping: [which alternative and why]

        ---

        *Analysis generated by Nika 🦋*
        *Always verify with current reviews before purchasing*
      temperature: 0.3
      max_tokens: 600
    artifact:
      path: "./output/product-review-{{date}}.md"
      format: text
"##;

/// Workflow 30: Newsletter Curator
/// Auto-curate topic newsletters
pub const WORKFLOW_30_NEWSLETTER: &str = r##"# ═══════════════════════════════════════════════════════════════════════════════
# WORKFLOW 30: NEWSLETTER CURATOR 📰
# ═══════════════════════════════════════════════════════════════════════════════
#
# Automatically curate and write newsletters on any topic.
# Perfect for weekly digests, industry roundups, or personal
# knowledge newsletters.
#
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │                       NEWSLETTER CURATION FLOW                              │
# │                                                                             │
# │  ┌──────────────────┐                                                       │
# │  │ define_edition   │ ← Theme and focus                                     │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ▼                                                                 │
# │  ┌────────────────────────────────────────────────────────────────┐        │
# │  │          FOR EACH: content_category                             │        │
# │  │   ┌──────────────────────────────────────────────────────────┐ │        │
# │  │   │      curate_content (News, Tools, Insights, Picks)       │ │        │
# │  │   └──────────────────────────────────────────────────────────┘ │        │
# │  └────────────────────────────────────────────────────────────────┘        │
# │           │                                                                 │
# │           ▼                                                                 │
# │  ┌──────────────────┐                                                       │
# │  │  write_intro     │ ← Engaging opening                                    │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ▼                                                                 │
# │  ┌──────────────────┐                                                       │
# │  │compile_newsletter│ ← Final formatted edition                             │
# │  └────────┬─────────┘                                                       │
# │           │                                                                 │
# │           ▼                                                                 │
# │       📧 Ready to Send                                                      │
# │                                                                             │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# ═══════════════════════════════════════════════════════════════════════════════

schema: "nika/workflow@0.12"
model: gpt-4o-mini
workflow: newsletter-curator
description: "Auto-curate topic newsletters"

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURATION
# ─────────────────────────────────────────────────────────────────────────────

inputs:
  newsletter_name: "AI Weekly Digest"
  edition_number: 42
  topic: "artificial intelligence and machine learning"
  audience: "tech professionals and AI enthusiasts"
  tone: "informative but accessible"
  sections:
    - name: "Top News"
      description: "Major AI announcements and breakthroughs"
      items: 3
    - name: "Tool Spotlight"
      description: "Useful AI tools and platforms"
      items: 2
    - name: "Research Corner"
      description: "Notable papers and findings"
      items: 2
    - name: "Industry Insights"
      description: "Trends and analysis"
      items: 1
    - name: "Resource of the Week"
      description: "Tutorial, course, or guide"
      items: 1

# ─────────────────────────────────────────────────────────────────────────────
# TASKS
# ─────────────────────────────────────────────────────────────────────────────

tasks:
  - id: define_edition
    description: "Define this edition's focus"
    infer:
      prompt: |
        Plan newsletter edition #{{inputs.edition_number}}:

        Newsletter: {{inputs.newsletter_name}}
        Topic: {{inputs.topic}}
        Audience: {{inputs.audience}}
        Tone: {{inputs.tone}}

        Sections:
        {{inputs.sections}}

        For this edition, define:

        1. **Theme/Hook**
           What's the unifying thread this week?

        2. **Key Topics to Cover**
           Based on recent developments in {{inputs.topic}}

        3. **Engagement Elements**
           - Question to ask readers
           - Poll idea
           - CTA for replies

        4. **Cross-Section Connections**
           How sections can reference each other

        Make it timely and relevant.
      temperature: 0.6
      max_tokens: 500

  - id: curate_content
    description: "Curate content for each section"
    depends_on: [define_edition]
    with:
      theme: $define_edition
    for_each:
      - name: "Top News"
        description: "Major AI announcements and breakthroughs"
        items: 3
      - name: "Tool Spotlight"
        description: "Useful AI tools and platforms"
        items: 2
      - name: "Research Corner"
        description: "Notable papers and findings"
        items: 2
      - name: "Industry Insights"
        description: "Trends and analysis"
        items: 1
      - name: "Resource of the Week"
        description: "Tutorial, course, or guide"
        items: 1
    as: section
    concurrency: 5
    infer:
      prompt: |
        Curate content for newsletter section:

        Section: {{with.section.name}}
        Description: {{with.section.description}}
        Items needed: {{with.section.items}}

        Edition theme:
        {{with.theme}}

        Topic: {{inputs.topic}}
        Audience: {{inputs.audience}}

        Note: In production, this would use web search/APIs.
        For demo, generate realistic simulated content.

        For each item, provide:

        ### [Item Title]

        **Source:** [Publication/Author]
        **Link:** [URL placeholder]

        **Summary:**
        2-3 sentences explaining what it is and why it matters.

        **Key Takeaway:**
        One-liner insight for busy readers.

        **Our Take:**
        Brief editorial perspective (optional).

        ---

        Make each item genuinely interesting and relevant.
        Vary the sources for credibility.
      temperature: 0.7
      max_tokens: 800

  - id: write_intro
    description: "Write engaging introduction"
    depends_on: [define_edition, curate_content]
    with:
      theme: $define_edition
      sections: $curate_content
    infer:
      prompt: |
        Write the newsletter introduction:

        Theme:
        {{with.theme}}

        Curated sections preview:
        {{with.sections}}

        Newsletter: {{inputs.newsletter_name}} #{{inputs.edition_number}}
        Audience: {{inputs.audience}}
        Tone: {{inputs.tone}}

        Write:

        **Subject Line Options (3):**
        - Curiosity-driven
        - Benefit-focused
        - News-hook

        **Preview Text:**
        (What shows in email client - 50-100 chars)

        **Opening Paragraph:**
        - Warm greeting
        - Hook that teases best content
        - Brief context on theme
        - 3-4 sentences max

        **What's Inside:**
        Quick bulleted preview of highlights

        **Transition to first section:**
        Smooth lead-in

        Make it feel personal and worth reading.
      temperature: 0.7
      max_tokens: 500

  - id: compile_newsletter
    description: "Compile final newsletter"
    depends_on: [write_intro, curate_content, define_edition]
    with:
      intro: $write_intro
      sections: $curate_content
      theme: $define_edition
    infer:
      prompt: |
        Compile the complete newsletter:

        Introduction:
        {{with.intro}}

        Curated sections:
        {{with.sections}}

        Theme guidance:
        {{with.theme}}

        Format the complete newsletter:

        ═══════════════════════════════════════════════════════════════
        📰 {{inputs.newsletter_name}} | Edition #{{inputs.edition_number}}
        ═══════════════════════════════════════════════════════════════

        [Introduction]

        ═══════════════════════════════════════════════════════════════
        📢 TOP NEWS
        ═══════════════════════════════════════════════════════════════

        [Section content]

        ═══════════════════════════════════════════════════════════════
        🛠️ TOOL SPOTLIGHT
        ═══════════════════════════════════════════════════════════════

        [Section content]

        ... [Continue for all sections] ...

        ═══════════════════════════════════════════════════════════════
        💬 COMMUNITY CORNER
        ═══════════════════════════════════════════════════════════════

        **This Week's Question:**
        [Engagement question]

        **Reply to share your thoughts!**

        ═══════════════════════════════════════════════════════════════
        📌 UNTIL NEXT TIME
        ═══════════════════════════════════════════════════════════════

        [Closing paragraph]

        ---

        Forward to a friend | Unsubscribe | View online

        © {{inputs.newsletter_name}}
        Curated with AI, edited with love.

        ---

        Keep section headers consistent.
        Add emoji for visual scanning.
        Ensure smooth transitions between sections.
      temperature: 0.4
      max_tokens: 2500
    artifact:
      path: "./output/newsletter-{{inputs.edition_number}}-{{date}}.md"
      format: text
"##;

/// Returns all tier 6 workflows (everyday magic)
pub fn get_tier6_workflows() -> Vec<WorkflowTemplate> {
    vec![
        WorkflowTemplate {
            filename: "21-morning-briefing.nika.yaml",
            tier_dir: TIER6_DIR,
            content: WORKFLOW_21_MORNING_BRIEFING,
        },
        WorkflowTemplate {
            filename: "22-social-media-planner.nika.yaml",
            tier_dir: TIER6_DIR,
            content: WORKFLOW_22_SOCIAL_MEDIA,
        },
        WorkflowTemplate {
            filename: "23-competitor-analysis.nika.yaml",
            tier_dir: TIER6_DIR,
            content: WORKFLOW_23_COMPETITOR_SPY,
        },
        WorkflowTemplate {
            filename: "24-email-composer.nika.yaml",
            tier_dir: TIER6_DIR,
            content: WORKFLOW_24_EMAIL_COMPOSER,
        },
        WorkflowTemplate {
            filename: "25-meal-planner.nika.yaml",
            tier_dir: TIER6_DIR,
            content: WORKFLOW_25_MEAL_PLANNER,
        },
        WorkflowTemplate {
            filename: "26-travel-planner.nika.yaml",
            tier_dir: TIER6_DIR,
            content: WORKFLOW_26_TRAVEL_PLANNER,
        },
        WorkflowTemplate {
            filename: "27-party-planner.nika.yaml",
            tier_dir: TIER6_DIR,
            content: WORKFLOW_27_PARTY_PLANNER,
        },
        WorkflowTemplate {
            filename: "28-podcast-show-notes.nika.yaml",
            tier_dir: TIER6_DIR,
            content: WORKFLOW_28_PODCAST_NOTES,
        },
        WorkflowTemplate {
            filename: "29-product-review-analyzer.nika.yaml",
            tier_dir: TIER6_DIR,
            content: WORKFLOW_29_PRODUCT_REVIEW,
        },
        WorkflowTemplate {
            filename: "30-newsletter-curator.nika.yaml",
            tier_dir: TIER6_DIR,
            content: WORKFLOW_30_NEWSLETTER,
        },
    ]
}
