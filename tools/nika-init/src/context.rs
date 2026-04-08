// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Context Files for Workflows
//!
//! These files are loaded via `context:` in workflows.
//! They provide reusable context that can be referenced
//! via `{{context.files.alias}}` in task prompts.

use super::ContextFile;

/// Directory for context files
pub const CONTEXT_DIR: &str = "context";

/// Brand guidelines context file
pub const CONTEXT_BRAND: &str = r##"# Brand Guidelines

## Company Overview

**Name:** TechStartup Inc
**Tagline:** "Empowering the Future of Work"
**Industry:** B2B SaaS

## Voice & Tone

### Personality Traits
- **Innovative** - We push boundaries
- **Approachable** - We speak human, not jargon
- **Confident** - We know our stuff
- **Helpful** - We're here to solve problems

### Writing Guidelines
1. Use active voice
2. Keep sentences concise (aim for 15-20 words)
3. Lead with benefits, not features
4. Use "you" and "your" to address readers
5. Avoid buzzwords: "synergy", "leverage", "disrupt"

### Do's and Don'ts

**Do:**
- Be conversational but professional
- Use contractions (we're, you'll, it's)
- Include specific examples
- Break up long content with headers

**Don't:**
- Use ALL CAPS (except for acronyms)
- Overuse exclamation marks
- Be overly formal or stiff
- Use passive voice unless necessary

## Visual Identity

### Colors
- Primary: #2563eb (Blue)
- Secondary: #10b981 (Green)
- Accent: #f59e0b (Amber)
- Neutral: #1f2937 (Dark Gray)

### Typography
- Headlines: Inter Bold
- Body: Inter Regular
- Code: JetBrains Mono

## Product References

When referring to our products:
- Full name first mention: "TechStartup Platform"
- Short form after: "the Platform" or "TechStartup"
- Never: "TS", "TSI", or abbreviations

## Contact Information

- Website: https://techstartup.example.com
- Support: support@techstartup.example.com
- Twitter: @TechStartupHQ
"##;

/// Writing style guide
pub const CONTEXT_STYLE_GUIDE: &str = r##"# Writing Style Guide

## Content Structure

### Headers
- Use sentence case for all headers
- Max 3 levels of hierarchy (H1, H2, H3)
- Headers should be descriptive, not clever

### Paragraphs
- One idea per paragraph
- 3-5 sentences maximum
- Use transition words between paragraphs

### Lists
- Use bullets for unordered information
- Use numbers for sequential steps
- Keep list items parallel in structure

## Grammar Rules

### Punctuation
- Oxford comma: always (red, white, and blue)
- Em dashes: use sparingly for emphasis
- Colons: capitalize after if full sentence follows

### Numbers
- Spell out one through nine
- Use numerals for 10 and above
- Always use numerals with units (5 GB, 3 minutes)

### Abbreviations
- Define on first use: "Application Programming Interface (API)"
- Common tech terms OK without definition: URL, HTML, CSS

## Formatting

### Emphasis
- **Bold** for UI elements and key terms
- *Italics* for titles and new concepts
- `Code` for commands, file names, variables

### Code Examples
- Include language identifier in code blocks
- Add comments explaining key lines
- Show expected output when helpful

## SEO Best Practices

### Keywords
- Include primary keyword in first paragraph
- Use variations naturally throughout
- Don't stuff keywords unnaturally

### Meta Content
- Title: 50-60 characters
- Description: 150-160 characters
- Include call-to-action in description
"##;

/// Technical terminology reference
pub const CONTEXT_TERMINOLOGY: &str = r##"# Technical Terminology

## Nika-Specific Terms

| Term | Definition | Usage |
|------|------------|-------|
| Workflow | A YAML file defining a sequence of tasks | "Create a new workflow" |
| Task | A single unit of work in a workflow | "This task runs an LLM inference" |
| Verb | The action type (infer, exec, fetch, invoke, agent) | "The infer verb generates content" |
| DAG | Directed Acyclic Graph - task dependencies | "The DAG ensures correct execution order" |
| Binding | Data passed between tasks via with: | "Use bindings to share data" |
| MCP | Model Context Protocol | "MCP tools extend agent capabilities" |

## AI/ML Terms

| Term | Definition | Plain English |
|------|------------|---------------|
| LLM | Large Language Model | "AI that understands and generates text" |
| Token | A unit of text (word or word piece) | "About 4 characters per token" |
| Context Window | Max input the model can process | "How much the AI can 'remember'" |
| Prompt | Instructions given to the AI | "What you ask the AI to do" |
| Temperature | Randomness in responses (0-1) | "How creative vs deterministic" |
| Inference | Model generating output | "When the AI creates its response" |

## Technical Infrastructure

| Term | Abbreviation | Explanation |
|------|--------------|-------------|
| Application Programming Interface | API | "How systems talk to each other" |
| Continuous Integration | CI | "Automated testing on code changes" |
| Continuous Deployment | CD | "Automated deployment to production" |
| Single Sign-On | SSO | "One login for multiple apps" |
| Software as a Service | SaaS | "Software you rent, not buy" |

## Tone Translations

When explaining technical concepts, translate to accessible language:

- "The API endpoint returned a 429" → "The service asked us to slow down"
- "Memory allocation failed" → "The system ran out of space"
- "Authentication token expired" → "You need to log in again"
- "Query timeout exceeded" → "The search took too long"
"##;

/// Sample persona for content generation
pub const CONTEXT_PERSONA: &str = r##"{
  "persona": {
    "name": "The Helpful Expert",
    "role": "Senior Technical Writer",
    "characteristics": [
      "Patient and thorough in explanations",
      "Uses analogies to clarify complex concepts",
      "Anticipates common questions",
      "Provides actionable next steps"
    ],
    "voice": {
      "tone": "Warm but professional",
      "formality": "Semi-formal (contractions OK)",
      "humor": "Occasional, never at reader's expense"
    },
    "expertise": [
      "Software development",
      "Technical documentation",
      "Developer experience",
      "API design"
    ],
    "communication_style": {
      "preferred": [
        "Start with the 'why' before the 'how'",
        "Use real-world examples",
        "Include code samples when relevant",
        "Summarize key points at the end"
      ],
      "avoid": [
        "Condescension ('simply', 'just', 'obviously')",
        "Unexplained jargon",
        "Walls of text without breaks",
        "Assumptions about reader's knowledge"
      ]
    }
  }
}
"##;

/// Code review checklist
pub const CONTEXT_CODE_REVIEW: &str = r##"# Code Review Checklist

## Security

- [ ] No hardcoded secrets or credentials
- [ ] Input validation on all user data
- [ ] SQL/NoSQL injection prevention
- [ ] XSS prevention (output encoding)
- [ ] CSRF tokens where needed
- [ ] Proper authentication/authorization
- [ ] Sensitive data encrypted
- [ ] No dangerous functions (eval, exec, etc.)

## Performance

- [ ] No N+1 query problems
- [ ] Proper indexing for queries
- [ ] Pagination for large datasets
- [ ] Caching where appropriate
- [ ] No memory leaks
- [ ] Efficient algorithms (correct Big O)
- [ ] Lazy loading for expensive operations

## Code Quality

- [ ] Single responsibility principle
- [ ] DRY (Don't Repeat Yourself)
- [ ] Clear naming conventions
- [ ] Appropriate error handling
- [ ] No dead code
- [ ] Consistent formatting
- [ ] Limited cyclomatic complexity

## Testing

- [ ] Unit tests for new functionality
- [ ] Edge cases covered
- [ ] Integration tests if needed
- [ ] Tests actually verify behavior
- [ ] No flaky tests introduced
- [ ] Good test names (describe what's tested)

## Documentation

- [ ] Public APIs documented
- [ ] Complex logic has comments
- [ ] README updated if needed
- [ ] Changelog entry added
- [ ] Migration guide if breaking changes

## Best Practices

- [ ] No TODO without ticket number
- [ ] Logging at appropriate levels
- [ ] Feature flags for risky changes
- [ ] Backwards compatible if possible
- [ ] Graceful degradation

## Before Merge

- [ ] All CI checks passing
- [ ] Code review approved
- [ ] No unresolved conversations
- [ ] Commit messages are clear
- [ ] Squashed if needed
"##;

/// Returns all context files
pub fn get_context_files() -> Vec<ContextFile> {
    vec![
        ContextFile {
            filename: "brand.md",
            dir: CONTEXT_DIR,
            content: CONTEXT_BRAND,
        },
        ContextFile {
            filename: "style-guide.md",
            dir: CONTEXT_DIR,
            content: CONTEXT_STYLE_GUIDE,
        },
        ContextFile {
            filename: "terminology.md",
            dir: CONTEXT_DIR,
            content: CONTEXT_TERMINOLOGY,
        },
        ContextFile {
            filename: "persona.json",
            dir: CONTEXT_DIR,
            content: CONTEXT_PERSONA,
        },
        ContextFile {
            filename: "code-review-checklist.md",
            dir: CONTEXT_DIR,
            content: CONTEXT_CODE_REVIEW,
        },
    ]
}
