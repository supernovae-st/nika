//! Tier 2: LLM Required Workflows (04-07)
//!
//! Prerequisites: `nika provider set anthropic` (or openai, mistral, groq, deepseek, gemini)
//!
//! Features covered:
//! - infer: basic LLM prompts with temperature, system, max_tokens
//! - DAG patterns: diamond, fan-out/fan-in
//! - for_each: parallel iteration with concurrency
//! - context: file loading
//! - include: DAG fusion

use super::WorkflowTemplate;

pub const TIER2_DIR: &str = "tier-2-llm";

// =============================================================================
// WORKFLOW 04: Infer Basics
// =============================================================================
pub const WORKFLOW_04_INFER_BASICS: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🧠 WORKFLOW 04: INFER BASICS                                                 ║
# ║  Learn LLM text generation with temperature, system prompts, and max_tokens   ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  PREREQUISITES:                                                               ║
# ║  ┌─────────────────────────────────────────────────────────────────────────┐  ║
# ║  │  # Cloud providers (pick one)                                          │  ║
# ║  │  nika provider set anthropic   # Claude (recommended)                   │  ║
# ║  │  nika provider set openai      # GPT-4                                  │  ║
# ║  │  nika provider set mistral     # Mistral Large                          │  ║
# ║  │  nika provider set groq        # Groq (fast, free tier)                 │  ║
# ║  │  nika provider set deepseek    # DeepSeek                               │  ║
# ║  │  nika provider set gemini      # Google Gemini                          │  ║
# ║  │                                                                         │  ║
# ║  │  # Or local models (no API key needed!)                                │  ║
# ║  │  nika model pull llama3.2:1b   # Then use provider: native              │  ║
# ║  └─────────────────────────────────────────────────────────────────────────┘  ║
# ║                                                                               ║
# ║  DAG FLOW:                                                                    ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │    ┌─────────────┐    ┌──────────────┐    ┌─────────────────┐         │   ║
# ║  │    │ creative    │    │  technical   │    │    balanced     │         │   ║
# ║  │    │ (temp=0.9)  │    │  (temp=0.1)  │    │   (temp=0.5)    │         │   ║
# ║  │    └──────┬──────┘    └──────┬───────┘    └────────┬────────┘         │   ║
# ║  │           │                  │                     │                  │   ║
# ║  │           └──────────────────┼─────────────────────┘                  │   ║
# ║  │                              ▼                                        │   ║
# ║  │                     ┌────────────────┐                                │   ║
# ║  │                     │    combine     │                                │   ║
# ║  │                     │ (synthesizes)  │                                │   ║
# ║  │                     └────────────────┘                                │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ║  WHAT YOU'LL LEARN:                                                           ║
# ║  • temperature: 0.0 (deterministic) → 1.0 (creative)                          ║
# ║  • system: Set the LLM's persona/role                                         ║
# ║  • max_tokens: Limit output length                                            ║
# ║  • Shorthand syntax: infer: "prompt" vs infer: { prompt: "...", ... }        ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: "nika/workflow@0.12"
workflow: infer-basics

# ─────────────────────────────────────────────────────────────────────────────────
# 📝 DESCRIPTION
# ─────────────────────────────────────────────────────────────────────────────────
# This workflow demonstrates the `infer:` verb - the heart of LLM text generation.
# We'll generate three variations of the same prompt with different temperatures,
# then combine them to show how settings affect output.

# ═══════════════════════════════════════════════════════════════════════════════
# TASKS
# ═══════════════════════════════════════════════════════════════════════════════

tasks:
  # ───────────────────────────────────────────────────────────────────────────────
  # 🎨 TASK 1: Creative Generation (High Temperature)
  # ───────────────────────────────────────────────────────────────────────────────
  # temperature: 0.9 = Very creative, unpredictable, sometimes wild!
  # Great for: brainstorming, poetry, creative writing, unique ideas

  - id: creative_tagline
    infer:
      prompt: |
        Generate a creative, memorable tagline for a futuristic coffee shop
        that serves AI-brewed beverages. Be bold and imaginative!
      temperature: 0.9          # 🔥 High creativity
      system: |
        You are a wildly creative advertising copywriter known for
        unexpected, memorable phrases. Think outside the box!
      max_tokens: 50            # Keep it punchy

  # ───────────────────────────────────────────────────────────────────────────────
  # 🔬 TASK 2: Technical/Precise Generation (Low Temperature)
  # ───────────────────────────────────────────────────────────────────────────────
  # temperature: 0.1 = Very deterministic, consistent, factual
  # Great for: technical docs, code, factual content, structured output

  - id: technical_tagline
    infer:
      prompt: |
        Generate a professional, precise tagline for a futuristic coffee shop
        that serves AI-brewed beverages. Focus on accuracy and clarity.
      temperature: 0.1          # 🧊 Low temperature = consistent
      system: |
        You are a technical writer. Be precise, clear, and professional.
        Avoid hyperbole. Focus on factual benefits.
      max_tokens: 50

  # ───────────────────────────────────────────────────────────────────────────────
  # ⚖️ TASK 3: Balanced Generation (Medium Temperature)
  # ───────────────────────────────────────────────────────────────────────────────
  # temperature: 0.5 = Good balance between creativity and coherence
  # Great for: general content, marketing copy, balanced output

  - id: balanced_tagline
    infer:
      prompt: |
        Generate a balanced, appealing tagline for a futuristic coffee shop
        that serves AI-brewed beverages. Be creative yet professional.
      temperature: 0.5          # ⚖️ Balanced
      system: You are a marketing expert balancing creativity with clarity.
      max_tokens: 50

  # ───────────────────────────────────────────────────────────────────────────────
  # 🔗 TASK 4: Combine Results
  # ───────────────────────────────────────────────────────────────────────────────
  # This task waits for all three to complete, then synthesizes them

  - id: combine_results
    depends_on: [creative_tagline, technical_tagline, balanced_tagline]
    with:
      creative: $creative_tagline    # Bind creative output
      technical: $technical_tagline  # Bind technical output
      balanced: $balanced_tagline    # Bind balanced output
    infer:
      prompt: |
        I generated three taglines for an AI coffee shop with different styles:

        🎨 CREATIVE (temp=0.9): {{with.creative}}

        🔬 TECHNICAL (temp=0.1): {{with.technical}}

        ⚖️ BALANCED (temp=0.5): {{with.balanced}}

        Analyze these three approaches and recommend which would work best
        for different audiences (tech enthusiasts, general public, investors).
        Then create one FINAL tagline that combines the best elements.
      temperature: 0.6
      system: You are a marketing strategist with expertise in audience segmentation.
      max_tokens: 300

# ═══════════════════════════════════════════════════════════════════════════════
# 🎓 LEARNING NOTES
# ═══════════════════════════════════════════════════════════════════════════════
#
# TEMPERATURE GUIDE:
# ┌─────────┬─────────────────────────────────────────────────────────────────┐
# │ 0.0-0.2 │ Very deterministic. Same input = nearly same output. Best for  │
# │         │ code, technical docs, factual content.                          │
# ├─────────┼─────────────────────────────────────────────────────────────────┤
# │ 0.3-0.5 │ Balanced. Some variation while staying coherent. Good default. │
# ├─────────┼─────────────────────────────────────────────────────────────────┤
# │ 0.6-0.8 │ Creative. More variation and unexpected ideas.                  │
# ├─────────┼─────────────────────────────────────────────────────────────────┤
# │ 0.9-1.0 │ Very creative. Can be unpredictable, great for brainstorming.  │
# └─────────┴─────────────────────────────────────────────────────────────────┘
#
# SHORTHAND SYNTAX:
# Full form:    infer: { prompt: "Hello", temperature: 0.7 }
# Shorthand:    infer: "Hello"  (uses default model and temperature)
#
# LOCAL MODELS (No API key needed!):
# Use provider: native with a local GGUF model:
#
#   - id: local_inference
#     infer:
#       provider: native
#       model: ~/.cache/huggingface/models/llama3.2-1b-q4.gguf
#       prompt: "Your prompt here"
#       temperature: 0.7
#
# Download models with: nika model pull llama3.2:1b
#
# RUN THIS WORKFLOW:
# nika run workflows/tier-2-llm/04-infer-basics.nika.yaml
"##;

// =============================================================================
// WORKFLOW 05: DAG Patterns
// =============================================================================
pub const WORKFLOW_05_DAG_PATTERNS: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🔷 WORKFLOW 05: DAG PATTERNS                                                 ║
# ║  Master complex workflow patterns: diamond, fan-out/fan-in, parallel chains   ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  DAG VISUALIZATION:                                                           ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │                         ┌──────────┐                                   │   ║
# ║  │                         │  start   │                                   │   ║
# ║  │                         │ (topic)  │                                   │   ║
# ║  │                         └────┬─────┘                                   │   ║
# ║  │                              │                                         │   ║
# ║  │              ┌───────────────┼───────────────┐                         │   ║
# ║  │              │               │               │                         │   ║
# ║  │              ▼               ▼               ▼                         │   ║
# ║  │       ┌──────────┐   ┌──────────┐   ┌──────────┐    ← FAN-OUT         │   ║
# ║  │       │ research │   │ examples │   │  trends  │                       │   ║
# ║  │       │          │   │          │   │          │                       │   ║
# ║  │       └────┬─────┘   └────┬─────┘   └────┬─────┘                       │   ║
# ║  │            │              │              │                             │   ║
# ║  │            └──────────────┼──────────────┘                             │   ║
# ║  │                           ▼                                            │   ║
# ║  │                    ┌──────────┐             ← DIAMOND MERGE            │   ║
# ║  │                    │ synthesize│                                       │   ║
# ║  │                    │          │                                        │   ║
# ║  │                    └────┬─────┘                                        │   ║
# ║  │                         │                                              │   ║
# ║  │            ┌────────────┴────────────┐                                 │   ║
# ║  │            ▼                         ▼                                 │   ║
# ║  │     ┌──────────┐             ┌──────────┐   ← PARALLEL CHAINS         │   ║
# ║  │     │  blog    │             │  social  │                              │   ║
# ║  │     │  post    │             │  posts   │                              │   ║
# ║  │     └────┬─────┘             └────┬─────┘                              │   ║
# ║  │          │                        │                                    │   ║
# ║  │          └────────────┬───────────┘                                    │   ║
# ║  │                       ▼                                                │   ║
# ║  │                ┌──────────┐              ← FAN-IN                      │   ║
# ║  │                │  final   │                                            │   ║
# ║  │                │ summary  │                                            │   ║
# ║  │                └──────────┘                                            │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ║  PATTERNS DEMONSTRATED:                                                       ║
# ║  • Fan-Out: One task spawns multiple parallel tasks                           ║
# ║  • Diamond: Multiple tasks converge to one (classic merge pattern)            ║
# ║  • Parallel Chains: Independent sequences running simultaneously              ║
# ║  • Fan-In: All branches converge to final result                              ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: "nika/workflow@0.12"
workflow: dag-patterns-masterclass

tasks:
  # ═══════════════════════════════════════════════════════════════════════════════
  # STAGE 1: STARTING POINT
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: define_topic
    infer:
      prompt: |
        Choose an interesting tech topic for a content creation pipeline.
        Just output a single topic in 3-5 words, nothing else.
        Examples: "AI-Powered Code Review", "Quantum Computing Basics"
      temperature: 0.8
      max_tokens: 20

  # ═══════════════════════════════════════════════════════════════════════════════
  # STAGE 2: FAN-OUT (Research Phase)
  # ═══════════════════════════════════════════════════════════════════════════════
  # Three parallel research streams, all starting from the same topic

  - id: research_fundamentals
    depends_on: [define_topic]
    with:
      topic: $define_topic
    infer:
      prompt: |
        Research the fundamentals of: {{with.topic}}

        Provide:
        1. Core concept explanation (2-3 sentences)
        2. Key terminology (3 terms with definitions)
        3. Why it matters (1 sentence)

        Keep it concise and educational.
      temperature: 0.3
      max_tokens: 200
      system: You are a technical educator who explains complex topics simply.

  - id: find_examples
    depends_on: [define_topic]
    with:
      topic: $define_topic
    infer:
      prompt: |
        Find 3 real-world examples or use cases for: {{with.topic}}

        For each example, provide:
        • Company/Project name
        • What they did
        • Result achieved

        Focus on impressive, concrete outcomes.
      temperature: 0.5
      max_tokens: 200
      system: You are a tech journalist with deep industry knowledge.

  - id: analyze_trends
    depends_on: [define_topic]
    with:
      topic: $define_topic
    infer:
      prompt: |
        Analyze current trends and future outlook for: {{with.topic}}

        Include:
        • Current adoption status (emerging/growing/mature)
        • 3 predictions for next 2 years
        • Potential challenges or concerns

        Be specific and forward-thinking.
      temperature: 0.6
      max_tokens: 200
      system: You are a tech industry analyst specializing in emerging technologies.

  # ═══════════════════════════════════════════════════════════════════════════════
  # STAGE 3: DIAMOND MERGE (Synthesis)
  # ═══════════════════════════════════════════════════════════════════════════════
  # All three research streams converge here

  - id: synthesize_research
    depends_on: [research_fundamentals, find_examples, analyze_trends]
    with:
      topic: $define_topic
      fundamentals: $research_fundamentals
      examples: $find_examples
      trends: $analyze_trends
    infer:
      prompt: |
        Synthesize this research on "{{with.topic}}" into a coherent brief:

        📚 FUNDAMENTALS:
        {{with.fundamentals}}

        💡 REAL-WORLD EXAMPLES:
        {{with.examples}}

        📈 TRENDS & OUTLOOK:
        {{with.trends}}

        Create a 3-paragraph executive summary that weaves these together.
        Highlight the most compelling points from each section.
      temperature: 0.4
      max_tokens: 400

  # ═══════════════════════════════════════════════════════════════════════════════
  # STAGE 4: PARALLEL CHAINS (Content Creation)
  # ═══════════════════════════════════════════════════════════════════════════════
  # Two independent content streams running in parallel

  - id: write_blog_post
    depends_on: [synthesize_research]
    with:
      topic: $define_topic
      research: $synthesize_research
    infer:
      prompt: |
        Write a short blog post intro (150 words max) about: {{with.topic}}

        Using this research:
        {{with.research}}

        Structure:
        - Hook/attention grabber
        - Why readers should care
        - What they'll learn

        Style: Engaging, professional, slightly conversational
      temperature: 0.6
      max_tokens: 200
      system: You are a tech blogger with a knack for making complex topics accessible.

  - id: create_social_posts
    depends_on: [synthesize_research]
    with:
      topic: $define_topic
      research: $synthesize_research
    infer:
      prompt: |
        Create 3 social media posts about: {{with.topic}}

        Based on:
        {{with.research}}

        Format:
        1. Twitter/X (280 chars) - punchy, includes hashtags
        2. LinkedIn - professional, thought-leadership angle
        3. Instagram caption - visual-friendly, emoji-enhanced

        Make each platform-appropriate.
      temperature: 0.7
      max_tokens: 400
      system: You are a social media strategist who knows each platform's voice.

  # ═══════════════════════════════════════════════════════════════════════════════
  # STAGE 5: FAN-IN (Final Summary)
  # ═══════════════════════════════════════════════════════════════════════════════
  # All branches converge for the final deliverable

  - id: final_content_package
    depends_on: [write_blog_post, create_social_posts]
    with:
      topic: $define_topic
      blog: $write_blog_post
      social: $create_social_posts
    infer:
      prompt: |
        Create a final content package summary for: "{{with.topic}}"

        📝 BLOG INTRO:
        {{with.blog}}

        📱 SOCIAL POSTS:
        {{with.social}}

        Now provide:
        1. ✅ Content checklist (what's ready to publish)
        2. 🎯 Key message consistency check
        3. 📊 Recommended posting schedule
        4. 💡 One bonus content idea
      temperature: 0.5
      max_tokens: 300

# ═══════════════════════════════════════════════════════════════════════════════
# 🎓 LEARNING NOTES
# ═══════════════════════════════════════════════════════════════════════════════
#
# DAG PATTERNS CHEAT SHEET:
# ┌─────────────────┬───────────────────────────────────────────────────────────┐
# │ Pattern         │ When to Use                                               │
# ├─────────────────┼───────────────────────────────────────────────────────────┤
# │ Fan-Out         │ Same input needs multiple parallel analyses               │
# │ Diamond         │ Multiple paths must converge before continuing            │
# │ Parallel Chains │ Independent workflows that can run simultaneously         │
# │ Fan-In          │ Collecting results from multiple sources                  │
# │ Sequential      │ Each step depends on the previous                         │
# └─────────────────┴───────────────────────────────────────────────────────────┘
#
# BINDING SYNTAX:
# $task_id        → Reference another task's output
# {{with.alias}}   → Template interpolation in prompts
#
# RUN THIS WORKFLOW:
# nika run workflows/tier-2-llm/05-dag-patterns.nika.yaml
"##;

// =============================================================================
// WORKFLOW 06: Parallel For-Each
// =============================================================================
pub const WORKFLOW_06_PARALLEL_FOREACH: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  🔄 WORKFLOW 06: PARALLEL FOR-EACH                                            ║
# ║  Process arrays in parallel with concurrency control and fail-fast            ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  DAG VISUALIZATION:                                                           ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │    ┌─────────────────────────────────────────────────────────────┐     │   ║
# ║  │    │                    FOR-EACH EXPANSION                       │     │   ║
# ║  │    │                                                             │     │   ║
# ║  │    │    Items: ["en-US", "fr-FR", "de-DE", "es-ES", "ja-JP"]    │     │   ║
# ║  │    │                                                             │     │   ║
# ║  │    │           concurrency: 3  (max parallel)                    │     │   ║
# ║  │    │                                                             │     │   ║
# ║  │    │    ┌────────┐  ┌────────┐  ┌────────┐                      │     │   ║
# ║  │    │    │ en-US  │  │ fr-FR  │  │ de-DE  │  ← Batch 1 (3)       │     │   ║
# ║  │    │    └───┬────┘  └───┬────┘  └───┬────┘                      │     │   ║
# ║  │    │        │           │           │                           │     │   ║
# ║  │    │        ▼           ▼           ▼                           │     │   ║
# ║  │    │    ┌────────┐  ┌────────┐                                  │     │   ║
# ║  │    │    │ es-ES  │  │ ja-JP  │      ← Batch 2 (2)               │     │   ║
# ║  │    │    └───┬────┘  └───┬────┘                                  │     │   ║
# ║  │    │        │           │                                       │     │   ║
# ║  │    └────────┼───────────┼───────────────────────────────────────┘     │   ║
# ║  │             │           │                                             │   ║
# ║  │             └─────┬─────┘                                             │   ║
# ║  │                   ▼                                                   │   ║
# ║  │            ┌──────────────┐                                           │   ║
# ║  │            │   COLLECT    │   ← All 5 results aggregated              │   ║
# ║  │            │   RESULTS    │                                           │   ║
# ║  │            └──────────────┘                                           │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ║  KEY CONCEPTS:                                                                ║
# ║  • for_each: Array of items to iterate over                                   ║
# ║  • for_each: $inputs.items - iterate over workflow inputs            ║
# ║  • as: Variable name for current item                                         ║
# ║  • concurrency: Max parallel executions (default: 1)                          ║
# ║  • fail_fast: Stop all on first error (default: true)                         ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: "nika/workflow@0.12"
workflow: parallel-foreach-localization

# ─────────────────────────────────────────────────────────────────────────────────
# 📝 SCENARIO
# ─────────────────────────────────────────────────────────────────────────────────
# We're localizing a product landing page into 5 languages simultaneously.
# Each language gets its own culturally-adapted version.

tasks:
  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 1: Define Source Content
  # ═══════════════════════════════════════════════════════════════════════════════

  - id: source_content
    infer:
      prompt: |
        Create a short product description (50 words) for a smart home device
        that controls lighting, temperature, and security. Make it compelling
        and suitable for localization into multiple languages.

        Output just the description text, nothing else.
      temperature: 0.6
      max_tokens: 100

  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 2: FOR-EACH Parallel Localization
  # ═══════════════════════════════════════════════════════════════════════════════
  # This is where the magic happens! 5 languages, 3 at a time.

  - id: localize_content
    depends_on: [source_content]
    for_each:                         # 🔄 Array of items to process
      - locale: en-US
        name: "American English"
        style: "casual, friendly, uses 'you'"
      - locale: fr-FR
        name: "French (France)"
        style: "formal 'vous', elegant, emphasize design"
      - locale: de-DE
        name: "German"
        style: "precise, technical, compound words OK"
      - locale: es-ES
        name: "Spanish (Spain)"
        style: "warm, use vosotros, emphasize family"
      - locale: ja-JP
        name: "Japanese"
        style: "polite keigo, emphasize harmony with home"
    as: lang                          # 🏷️ Variable name for each item
    concurrency: 3                    # ⚡ Max 3 parallel LLM calls
    fail_fast: true                   # 🛑 Stop all if one fails
    with:
      source: $source_content
    infer:
      prompt: |
        Localize this product description for {{with.lang.name}} ({{with.lang.locale}}):

        SOURCE TEXT:
        {{with.source}}

        LOCALIZATION GUIDELINES:
        • Style: {{with.lang.style}}
        • Keep the same meaning but adapt culturally
        • Use natural {{with.lang.name}} expressions
        • Maintain approximately the same length

        Output ONLY the localized text, nothing else.
      temperature: 0.4
      max_tokens: 150
      system: |
        You are a native {{with.lang.name}} speaker and professional localizer.
        You understand cultural nuances and marketing copy.

  # ═══════════════════════════════════════════════════════════════════════════════
  # TASK 3: Quality Analysis
  # ═══════════════════════════════════════════════════════════════════════════════
  # Analyze all localizations together

  - id: quality_check
    depends_on: [localize_content]
    with:
      source: $source_content
      localizations: $localize_content  # 📦 All 5 results as array
    infer:
      prompt: |
        Review these localizations of our product description:

        📄 ORIGINAL (English):
        {{with.source}}

        🌍 LOCALIZATIONS:
        {{with.localizations}}

        Provide a brief quality assessment:
        1. ✅ Which localizations captured the tone best?
        2. ⚠️ Any that might need cultural adjustment?
        3. 📊 Overall localization quality score (1-10)
        4. 💡 One improvement suggestion

        Be concise but specific.
      temperature: 0.3
      max_tokens: 300

# ═══════════════════════════════════════════════════════════════════════════════
# 🎓 FOR-EACH REFERENCE
# ═══════════════════════════════════════════════════════════════════════════════
#
# BASIC SYNTAX:
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │ for_each: ["a", "b", "c"]     # Simple string array                        │
# │ as: item                       # Access via {{with.item}}                    │
# │ concurrency: 2                 # Run 2 at a time                           │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# WITH OBJECTS:
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │ for_each:                                                                   │
# │   - name: Alice                                                             │
# │     role: engineer                                                          │
# │   - name: Bob                                                               │
# │     role: designer                                                          │
# │ as: person                                                                  │
# │ # Access: {{with.person.name}}, {{with.person.role}}                         │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# FROM BINDING:
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │ for_each: $previous_task_array   # Array from another task                 │
# │ as: item                                                                    │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# FROM WORKFLOW INPUTS:
# ┌─────────────────────────────────────────────────────────────────────────────┐
# │ # Define inputs at workflow level                                          │
# │ inputs:                                                                     │
# │   locales:                                                                  │
# │     type: array                                                             │
# │     default: ["en-US", "fr-FR", "de-DE"]                                   │
# │                                                                             │
# │ tasks:                                                                      │
# │   - id: translate                                                           │
# │     for_each: $inputs.locales                           │
# │     as: locale                                                              │
# │     infer: "Translate to {{with.locale}}"                                   │
# └─────────────────────────────────────────────────────────────────────────────┘
#
# CONCURRENCY GUIDE:
# ┌─────────────┬─────────────────────────────────────────────────────────────┐
# │ concurrency │ Behavior                                                     │
# ├─────────────┼─────────────────────────────────────────────────────────────┤
# │ 1 (default) │ Sequential execution                                         │
# │ 2-5         │ Good for most LLM APIs (rate limits)                        │
# │ 10+         │ Only for local/high-rate-limit APIs                         │
# └─────────────┴─────────────────────────────────────────────────────────────┘
#
# RUN THIS WORKFLOW:
# nika run workflows/tier-2-llm/06-parallel-foreach.nika.yaml
"##;

// =============================================================================
// WORKFLOW 07: Context and Include
// =============================================================================
pub const WORKFLOW_07_CONTEXT_INCLUDE: &str = r##"# ╔═══════════════════════════════════════════════════════════════════════════════╗
# ║  📁 WORKFLOW 07: CONTEXT & INCLUDE                                            ║
# ║  Load external files and compose workflows with DAG fusion                    ║
# ╠═══════════════════════════════════════════════════════════════════════════════╣
# ║                                                                               ║
# ║  DAG VISUALIZATION:                                                           ║
# ║  ┌────────────────────────────────────────────────────────────────────────┐   ║
# ║  │                                                                        │   ║
# ║  │    ┌──────────────────────────────────────────────────────────────┐    │   ║
# ║  │    │                    CONTEXT LOADING                            │    │   ║
# ║  │    │                                                               │    │   ║
# ║  │    │   ./context/brand.md ─────────► {{context.files.brand}}      │    │   ║
# ║  │    │   ./context/persona.json ───► {{context.files.persona}}      │    │   ║
# ║  │    │   ./context/*.md ────────────► {{context.files.guides}}      │    │   ║
# ║  │    │                                                               │    │   ║
# ║  │    └──────────────────────────────────────────────────────────────┘    │   ║
# ║  │                              │                                        │   ║
# ║  │                              ▼                                        │   ║
# ║  │    ┌──────────────────────────────────────────────────────────────┐    │   ║
# ║  │    │                     INCLUDE (DAG FUSION)                      │    │   ║
# ║  │    │                                                               │    │   ║
# ║  │    │   ./partials/validate.nika.yaml ──► [validate_input]         │    │   ║
# ║  │    │   ./partials/format.nika.yaml ────► [format_output]          │    │   ║
# ║  │    │                                                               │    │   ║
# ║  │    └──────────────────────────────────────────────────────────────┘    │   ║
# ║  │                              │                                        │   ║
# ║  │                              ▼                                        │   ║
# ║  │    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐             │   ║
# ║  │    │  validate   │───►│   generate  │───►│   format    │             │   ║
# ║  │    │  (included) │    │   (main)    │    │  (included) │             │   ║
# ║  │    └─────────────┘    └─────────────┘    └─────────────┘             │   ║
# ║  │                                                                        │   ║
# ║  └────────────────────────────────────────────────────────────────────────┘   ║
# ║                                                                               ║
# ║  KEY FEATURES:                                                                ║
# ║  • context.files: Load MD, JSON, YAML, or glob patterns                       ║
# ║  • context.session: Restore previous workflow state                           ║
# ║  • include: Merge tasks from other workflows                                  ║
# ║  • prefix: Namespace included tasks to avoid conflicts                        ║
# ║  • skills: Define reusable prompt templates                                   ║
# ║                                                                               ║
# ╚═══════════════════════════════════════════════════════════════════════════════╝

schema: "nika/workflow@0.12"
workflow: context-include-demo

# ═══════════════════════════════════════════════════════════════════════════════
# CONTEXT - Load external files at workflow start
# ═══════════════════════════════════════════════════════════════════════════════
# Files are loaded BEFORE any tasks run, making them available everywhere.
#
# Supported formats:
# • .md, .txt      → Loaded as raw string
# • .json          → Parsed as JSON object
# • .yaml, .yml    → Parsed as YAML object
# • glob patterns  → Returns array of file contents

context:
  files:
    # 📝 Markdown file - loaded as string
    brand_guidelines: ./context/brand.md

    # 🔧 JSON config - parsed into object (persona for content tone)
    persona: ./context/persona.json

    # 📚 Glob pattern - array of all matching .md files
    guides: ./context/*.md

  # 💾 Session restore (optional) - previous workflow state
  # session: .nika/sessions/previous.json

# ═══════════════════════════════════════════════════════════════════════════════
# SKILLS - Reusable prompt templates (optional)
# ═══════════════════════════════════════════════════════════════════════════════
# Skills are markdown files with prompt patterns that can be referenced.
# Uncomment below if you have skill files in your project:
# skills:
#   code_review: ./.nika/skills/code-review.md

# ═══════════════════════════════════════════════════════════════════════════════
# INCLUDE - DAG Fusion (merge tasks from other workflows)
# ═══════════════════════════════════════════════════════════════════════════════
# Tasks from included workflows are merged into this DAG.
# Use prefix to namespace and avoid task ID conflicts.

# NOTE: For this demo workflow, we'll simulate the include behavior
# In a real project, you'd have these files:
# include:
#   - path: ./partials/validate-input.nika.yaml
#     prefix: val_          # Tasks become: val_check_format, val_check_length
#   - path: ./partials/format-output.nika.yaml
#     prefix: fmt_          # Tasks become: fmt_markdown, fmt_json

# ═══════════════════════════════════════════════════════════════════════════════
# TASKS
# ═══════════════════════════════════════════════════════════════════════════════

tasks:
  # ───────────────────────────────────────────────────────────────────────────────
  # 📥 TASK 1: Validate Input (simulating included task)
  # ───────────────────────────────────────────────────────────────────────────────

  - id: validate_topic
    infer:
      prompt: |
        Validate this topic for content generation:

        TOPIC: "AI-powered productivity tools"

        Check:
        1. Is it specific enough? (not too broad)
        2. Is it appropriate for a blog post?
        3. Is it timely/relevant?

        Output: "VALID" or "INVALID: <reason>"
      temperature: 0.2
      max_tokens: 50

  # ───────────────────────────────────────────────────────────────────────────────
  # 🎨 TASK 2: Generate with Context
  # ───────────────────────────────────────────────────────────────────────────────
  # This task uses files loaded from context

  - id: generate_content
    depends_on: [validate_topic]
    with:
      validation: $validate_topic
    infer:
      prompt: |
        Generate blog content using our brand guidelines and style.

        VALIDATION STATUS: {{with.validation}}

        ═══════════════════════════════════════════════════════════════════
        BRAND GUIDELINES (from context/brand.md):
        ═══════════════════════════════════════════════════════════════════
        {{context.files.brand_guidelines}}

        ═══════════════════════════════════════════════════════════════════
        PERSONA (from context/persona.json):
        ═══════════════════════════════════════════════════════════════════
        {{context.files.persona}}

        ═══════════════════════════════════════════════════════════════════
        STYLE GUIDES (from context/*.md):
        ═══════════════════════════════════════════════════════════════════
        {{context.files.guides}}

        ═══════════════════════════════════════════════════════════════════

        Now generate a 150-word intro paragraph for a blog post about
        "AI-powered productivity tools" following these guidelines.
      temperature: 0.6
      max_tokens: 250
      system: |
        You are a content writer who strictly follows brand guidelines.
        If context files are empty or unavailable, use sensible defaults.

  # ───────────────────────────────────────────────────────────────────────────────
  # 📤 TASK 3: Format Output (simulating included task)
  # ───────────────────────────────────────────────────────────────────────────────

  - id: format_output
    depends_on: [generate_content]
    with:
      content: $generate_content
    infer:
      prompt: |
        Format this content for publishing:

        {{with.content}}

        Add:
        1. A catchy headline
        2. Meta description (150 chars)
        3. 3-5 SEO keywords
        4. Suggested featured image description

        Output as structured Markdown.
      temperature: 0.4
      max_tokens: 300

# ═══════════════════════════════════════════════════════════════════════════════
# 🎓 LEARNING NOTES
# ═══════════════════════════════════════════════════════════════════════════════
#
# CONTEXT FILE LOADING:
# ┌────────────────────────────────────────────────────────────────────────────┐
# │ context:                                                                    │
# │   files:                                                                    │
# │     # String content                                                        │
# │     readme: ./README.md        → "# Title\nContent..."                     │
# │                                                                             │
# │     # Parsed JSON                                                           │
# │     config: ./config.json      → { "key": "value" }                        │
# │                                                                             │
# │     # Glob → Array                                                          │
# │     docs: ./docs/*.md          → ["# Doc1", "# Doc2", ...]                 │
# │                                                                             │
# │   session:                     # Optional: restore previous state           │
# │     .nika/sessions/prev.json                                                │
# └────────────────────────────────────────────────────────────────────────────┘
#
# INCLUDE (DAG FUSION):
# ┌────────────────────────────────────────────────────────────────────────────┐
# │ include:                                                                    │
# │   - path: ./partials/setup.nika.yaml                                       │
# │     prefix: setup_          # setup.nika.yaml's task "init" → "setup_init" │
# │                                                                             │
# │   - path: ./partials/teardown.nika.yaml                                    │
# │     # no prefix → tasks keep original IDs (watch for conflicts!)           │
# └────────────────────────────────────────────────────────────────────────────┘
#
# SKILLS (object format - alias: path):
# ┌────────────────────────────────────────────────────────────────────────────┐
# │ skills:                                                                     │
# │   writer: ./skills/writing.md              # Local file                    │
# │   coder: pkg:@spn/core@1.0.0/skills/coding.md  # From registry             │
# └────────────────────────────────────────────────────────────────────────────┘
#
# SECURITY NOTE:
# • Path traversal is BLOCKED (../../../etc/passwd won't work)
# • All paths are validated against project root
#
# SETUP FOR THIS DEMO:
# Create these files before running:
#
# ./context/brand.md:
#   # Our Brand Voice
#   - Friendly but professional
#   - Use "we" and "you"
#   - Avoid jargon
#
# ./context/style.json:
#   { "tone": "conversational", "maxWords": 200 }
#
# ./context/examples/example1.md:
#   # Example Post Title
#   Example content here...
#
# RUN THIS WORKFLOW:
# nika run workflows/tier-2-llm/07-context-include.nika.yaml
"##;

/// Returns all Tier 2 workflows (04-07)
pub fn get_tier2_workflows() -> Vec<WorkflowTemplate> {
    vec![
        WorkflowTemplate {
            filename: "04-infer-basics.nika.yaml",
            tier_dir: TIER2_DIR,
            content: WORKFLOW_04_INFER_BASICS,
        },
        WorkflowTemplate {
            filename: "05-dag-patterns.nika.yaml",
            tier_dir: TIER2_DIR,
            content: WORKFLOW_05_DAG_PATTERNS,
        },
        WorkflowTemplate {
            filename: "06-parallel-foreach.nika.yaml",
            tier_dir: TIER2_DIR,
            content: WORKFLOW_06_PARALLEL_FOREACH,
        },
        WorkflowTemplate {
            filename: "07-context-include.nika.yaml",
            tier_dir: TIER2_DIR,
            content: WORKFLOW_07_CONTEXT_INCLUDE,
        },
    ]
}
