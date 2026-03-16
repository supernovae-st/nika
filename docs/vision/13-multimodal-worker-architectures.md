# 13 -- Multi-Modal AI Agent Worker/Satellite Architectures

> **Research Date**: 2026-03-14
> **Last Updated**: 2026-03-15
> **Scope**: How modern frameworks define, configure, and route multi-modal workers
> **Frameworks Analyzed**: LangGraph, CrewAI, AutoGen 0.4, Semantic Kernel, OpenAI Assistants, Anthropic Messages API, Letta/MemGPT, Google A2A Protocol, MCP Protocol
> **Status**: VALIDATED — Approach C with Progressive Disclosure selected

---

## Validated Decisions (2026-03-15)

This section summarizes the design decisions validated during the 2026-03-15 brainstorm session with Thibaut.

| Decision | Summary |
|----------|---------|
| **Approach** | **Approach C (Full Capabilities Manifest) + Progressive Disclosure** |
| **Capability Source** | Hybrid: Model modalities + Tool input schemas (auto-inferred) |
| **Routing** | Shaka capability-match with LLM fallback for ambiguous cases |
| **Image Generation** | MCP tools (image-gen:*) called by satellite LLMs, not dedicated workers |
| **Local Models** | `provider: native` with mistral.rs, optional cloud fallback |
| **Extensibility** | MCP as universal capability extender |

**Key Insight:** Satellites can be defined at three levels of complexity (minimal, medium, full). Nika infers capabilities when not explicitly declared, reducing boilerplate while allowing precision when needed.

---

## Executive Summary

No major framework has a first-class "modality enum" on worker definitions. Instead, multi-modal
capability emerges from the **combination of model selection + tool attachment + routing logic**.
The industry converges on three patterns:

1. **Model-implies-modality** -- pick GPT-4o (vision+text) vs Whisper (audio) vs DALL-E (image gen)
2. **Tools-as-capabilities** -- attach tools that handle specific modalities (code_interpreter, file_search, vision_tool)
3. **Routing-by-classification** -- an orchestrator node/agent classifies input and dispatches to the right worker

The most structured approach is Google's **A2A AgentCard** with explicit `skills[].inputModes` /
`skills[].outputModes` arrays using MIME-like content types. This is the closest thing to a
"capability manifest" for multi-modal workers.

---

## Table of Contents

1. [Framework Comparison Matrix](#1-framework-comparison-matrix)
2. [LangGraph -- Graph Nodes with Conditional Routing](#2-langgraph)
3. [CrewAI -- Role-Based YAML Agents](#3-crewai)
4. [AutoGen 0.4 -- Actor Model with Group Chat](#4-autogen-04)
5. [Semantic Kernel -- Plugin-Based Agents](#5-semantic-kernel)
6. [OpenAI Assistants API -- Tools + Model Capabilities](#6-openai-assistants)
7. [Anthropic Messages API -- Multi-Modal Content Blocks](#7-anthropic-messages-api)
8. [Letta/MemGPT -- Stateful Agents with Memory](#8-lettamemgpt)
9. [Google A2A Protocol -- AgentCard with Skills](#9-google-a2a-protocol)
10. [MCP Protocol -- Server Capabilities](#10-mcp-protocol)
11. [Capability Definition Patterns](#11-capability-definition-patterns)
12. [Routing Architectures](#12-routing-architectures)
13. [Synthesis for Nika](#13-synthesis-for-nika)

**Validated Design (2026-03-15):**

14. [Progressive Disclosure Design](#14-progressive-disclosure-design)
15. [Capability Inference](#15-capability-inference)
16. [Shaka Routing Algorithm](#16-shaka-routing-algorithm)
17. [Image Generation Strategy](#17-image-generation-strategy)
18. [Local Native Models](#18-local-native-models)
19. [MCP as Capability Extender](#19-mcp-as-capability-extender)

---

## 1. Framework Comparison Matrix

| Framework | Worker Definition | Modality Handling | Routing Mechanism | Config Format |
|-----------|------------------|-------------------|-------------------|---------------|
| **LangGraph** | Python function nodes | Model choice per node | Conditional edges on StateGraph | Python TypedDict |
| **CrewAI** | YAML agent with role/goal/backstory | Tools + LLM per agent | Sequential/Hierarchical Process | YAML (agents.yaml) |
| **AutoGen 0.4** | Agent classes (AssistantAgent, etc.) | model_client + tools per agent | SelectorGroupChat / RoundRobin | Python instantiation |
| **Semantic Kernel** | ChatCompletionAgent + plugins | Multi-model per agent | AgentGroupChat + termination | C#/Python code |
| **OpenAI Assistants** | Assistant with tools array | Model determines modality | Thread-based, manual handoff | JSON API |
| **Anthropic** | Messages with content blocks | Content block types (text/image/tool_use) | Tool routing via LLM | JSON API |
| **Letta/MemGPT** | Agent with memory_blocks + tools | Model + embedding_model | Single agent, tool-based dispatch | REST JSON / Python |
| **A2A Protocol** | AgentCard with skills array | skills[].inputModes/outputModes | Capability negotiation via card | JSON AgentCard |
| **MCP Protocol** | Server with tools/resources/prompts | Tool inputSchema per capability | Client discovers via initialize | JSON-RPC |

---

## 2. LangGraph

### Architecture

LangGraph defines workers as **Python function nodes** in a `StateGraph`. Each node is a function
that reads/writes shared state. Multi-modal capability comes from what model/tools each node uses
internally.

### Worker Definition Pattern

```python
from typing import TypedDict, Literal, Annotated
from langgraph.graph import StateGraph, START, END
import operator

# State schema -- the "contract" between nodes
class MultiModalState(TypedDict):
    messages: Annotated[list, operator.add]
    input_modality: str          # "text", "image", "audio", "code"
    classification: dict         # routing metadata
    output: str

# Worker nodes -- each is a plain function
def text_agent(state: MultiModalState) -> dict:
    """Text-only LLM worker."""
    llm = ChatOpenAI(model="gpt-4o-mini")
    response = llm.invoke(state["messages"])
    return {"messages": [response], "output": response.content}

def vision_agent(state: MultiModalState) -> dict:
    """Vision-capable worker."""
    llm = ChatOpenAI(model="gpt-4o")  # vision model
    msg = [{"type": "image_url", "image_url": state["image"]}]
    response = llm.invoke(msg)
    return {"messages": [response], "output": response.content}

def code_agent(state: MultiModalState) -> dict:
    """Code execution worker."""
    llm = ChatOpenAI(model="gpt-4o-mini")
    response = llm.invoke([f"Write Python for: {state['messages'][-1].content}"])
    return {"messages": [response], "output": response.content}
```

### Routing via Conditional Edges

```python
def route_modality(state: MultiModalState) -> Literal["text_agent", "vision_agent", "code_agent"]:
    """Classifier node -- routes based on input analysis."""
    modality = state.get("input_modality", "text")
    if modality == "image":
        return "vision_agent"
    elif modality == "code":
        return "code_agent"
    return "text_agent"

# Or: LLM-based classification
class ModalityClassification(TypedDict):
    modality: Literal["text", "vision", "code"]
    confidence: float

def classify_and_route(state: MultiModalState) -> Literal[...]:
    structured_llm = llm.with_structured_output(ModalityClassification)
    result = structured_llm.invoke(state["messages"])
    return f"{result['modality']}_agent"

# Build graph
graph = StateGraph(MultiModalState)
graph.add_node("classifier", classify_and_route)
graph.add_node("text_agent", text_agent)
graph.add_node("vision_agent", vision_agent)
graph.add_node("code_agent", code_agent)

graph.add_edge(START, "classifier")
graph.add_conditional_edges("classifier", route_modality, {
    "text_agent": "text_agent",
    "vision_agent": "vision_agent",
    "code_agent": "code_agent"
})
graph.add_edge("text_agent", END)
graph.add_edge("vision_agent", END)
graph.add_edge("code_agent", END)

app = graph.compile()
```

### Key Insight

LangGraph has **no modality field on nodes**. Modality is emergent from:
- Which model the node instantiates
- What tools it binds
- How the conditional edge classifies input

---

## 3. CrewAI

### Architecture

CrewAI uses **role-based agents** defined in YAML. Each agent has a role, goal, backstory, tools,
and optional LLM override. There is no modality field.

### Agent YAML Schema

```yaml
# config/agents.yaml
researcher:
  role: Research Analyst
  goal: Conduct thorough research on assigned topics
  backstory: >
    You are an expert researcher with deep knowledge
    of data analysis and information synthesis.
  tools:
    - SerperDevTool
    - ScrapeWebsiteTool
  llm: openai/gpt-4o
  verbose: true
  allow_delegation: true
  max_iter: 15
  max_rpm: 10

image_analyst:
  role: Visual Content Analyst
  goal: Analyze images and extract meaningful insights
  backstory: >
    You specialize in visual analysis, OCR, and
    image understanding using vision-capable models.
  tools:
    - VisionTool
  llm: openai/gpt-4o     # vision-capable model
  verbose: true

code_developer:
  role: Python Developer
  goal: Write clean, tested Python code
  backstory: >
    You are a senior Python developer who writes
    production-quality code with tests.
  tools:
    - CodeInterpreterTool
  llm: anthropic/claude-sonnet-4-20250514
```

### Task YAML Schema

```yaml
# config/tasks.yaml
research_task:
  description: >
    Conduct research about {topic}.
    Find relevant information and data.
  expected_output: >
    A list of 10 bullet points with key findings.
  agent: researcher

analyze_image_task:
  description: >
    Analyze the provided image and describe its contents.
  expected_output: >
    Detailed description of the image contents.
  agent: image_analyst

generate_code_task:
  description: >
    Write a Python script based on the research findings.
  expected_output: >
    Complete Python script with docstrings and tests.
  agent: code_developer
  context:
    - research_task    # depends on research output
```

### Crew Orchestration

```python
@CrewBase
class MultiModalCrew():
    agents_config = "config/agents.yaml"
    tasks_config = "config/tasks.yaml"

    @crew
    def crew(self) -> Crew:
        return Crew(
            agents=[self.researcher(), self.image_analyst(), self.code_developer()],
            tasks=[self.research_task(), self.analyze_image_task(), self.generate_code_task()],
            process=Process.sequential,  # or Process.hierarchical
            verbose=True
        )
```

### Key Insight

CrewAI routing is **task-to-agent assignment**, not capability-based matching. Each task
explicitly names its agent. The `Process.hierarchical` mode adds a manager agent that can
delegate, but still based on role descriptions, not modality enums.

---

## 4. AutoGen 0.4

### Architecture

AutoGen 0.4 uses the **actor model** with a layered architecture:
- **Core**: Event-driven message passing
- **AgentChat**: High-level agent classes + group chats
- **Extensions**: Model clients, tools, web surfer

### Agent Definition

```python
from autogen_ext.models.openai import OpenAIChatCompletionClient
from autogen_agentchat.agents import AssistantAgent, UserProxyAgent
from autogen_core.tools import FunctionTool

# Model client -- determines modality capability
model_text = OpenAIChatCompletionClient(
    model="gpt-4o-mini",
    api_key=os.environ["OPENAI_API_KEY"]
)
model_vision = OpenAIChatCompletionClient(
    model="gpt-4o",               # vision-capable
    api_key=os.environ["OPENAI_API_KEY"]
)

# Tool definitions
def analyze_image(image_url: str) -> str:
    """Analyze an image and return description."""
    ...

def execute_code(code: str) -> str:
    """Execute Python code in sandbox."""
    ...

# Agent definitions -- 3 required fields: name, model_client, system_message
text_agent = AssistantAgent(
    name="text_analyst",
    model_client=model_text,
    tools=[],
    system_message="You analyze text and provide insights. Say TERMINATE when done."
)

vision_agent = AssistantAgent(
    name="vision_analyst",
    model_client=model_vision,
    tools=[FunctionTool(func=analyze_image)],
    system_message="You analyze images. Describe what you see. Say TERMINATE when done."
)

code_agent = AssistantAgent(
    name="code_executor",
    model_client=model_text,
    tools=[FunctionTool(func=execute_code)],
    system_message="You write and execute code. Say TERMINATE when done."
)
```

### Pre-Built Multi-Modal Agents

```python
from autogen_ext.agents.web_surfer import MultimodalWebSurfer
from autogen_ext.agents.code_executor import CodeExecutorAgent

# MultimodalWebSurfer -- browses web with vision
web_surfer = MultimodalWebSurfer(
    name="web_surfer",
    model_client=model_vision
)

# CodeExecutorAgent -- executes code in sandbox
code_exec = CodeExecutorAgent(
    name="code_exec",
    code_executor=LocalCommandLineCodeExecutor(work_dir="./sandbox")
)
```

### Group Chat Routing

```python
from autogen_agentchat.teams import SelectorGroupChat, RoundRobinGroupChat
from autogen_agentchat.conditions import TextMentionTermination

# SelectorGroupChat -- LLM picks next speaker based on conversation
team = SelectorGroupChat(
    participants=[text_agent, vision_agent, code_agent],
    model_client=model_text,      # selector model
    termination_condition=TextMentionTermination("TERMINATE"),
    selector_prompt="""You are a team coordinator. Based on the conversation,
    select the most appropriate next agent:
    - text_analyst: for text analysis tasks
    - vision_analyst: for image/visual tasks
    - code_executor: for code writing/execution tasks
    Respond with just the agent name."""
)

# RoundRobinGroupChat -- fixed rotation
team_rr = RoundRobinGroupChat(
    participants=[text_agent, vision_agent, code_agent],
    max_turns=6,
    termination_condition=TextMentionTermination("TERMINATE")
)

result = await team.run(task="Analyze this chart image and write code to reproduce it.")
```

### Key Insight

AutoGen's `SelectorGroupChat` is the most **LLM-driven routing** pattern -- the selector model
reads the conversation and picks the next speaker. No explicit modality metadata; the selector
prompt describes capabilities in natural language.

---

## 5. Semantic Kernel

### Architecture

Microsoft Semantic Kernel uses **plugins** (skills) + **kernel** + **agents** in group chats.
Multi-model support lets different agents use different LLMs.

### Agent Definition

```csharp
// C# -- each agent gets its own kernel with specific model
var kernelText = Kernel.CreateBuilder()
    .AddOpenAIChatCompletion("gpt-4o-mini", apiKey)
    .Build();

var kernelVision = Kernel.CreateBuilder()
    .AddOpenAIChatCompletion("gpt-4o", apiKey)  // vision
    .Build();

// Agents with instructions (system prompt)
ChatCompletionAgent textAgent = new()
{
    Name = "TextAnalyst",
    Instructions = "You analyze text documents and extract key information.",
    Kernel = kernelText
};

ChatCompletionAgent visionAgent = new()
{
    Name = "VisionAnalyst",
    Instructions = "You analyze images and visual content.",
    Kernel = kernelVision
};
```

### Multi-Agent Group Chat

```csharp
AgentGroupChat chat = new(textAgent, visionAgent)
{
    ExecutionSettings = new()
    {
        TerminationStrategy = new ApprovalTerminationStrategy()
        {
            Agents = [textAgent],
            MaximumIterations = 10
        }
    }
};

await chat.AddChatMessageAsync(new ChatMessageContent(
    AuthorRole.User,
    "Analyze this document and its embedded images."
));

await foreach (var message in chat.InvokeAsync())
{
    Console.WriteLine($"{message.AuthorName}: {message.Content}");
}
```

### A2A Protocol Integration

Semantic Kernel supports the **A2A (Agent-to-Agent) protocol** for cross-framework agent
communication, enabling capability discovery via AgentCards.

### Key Insight

Semantic Kernel has **no modality field** on agents. Modality is determined by:
- Which model the kernel uses (GPT-4o = vision, GPT-4o-mini = text only)
- Which plugins are registered (image processing, code execution, etc.)
- The agent's Instructions (system prompt)

---

## 6. OpenAI Assistants API

### Architecture

Assistants are stateful agents with persistent threads. Modality is determined by **model choice**
+ **tools** (code_interpreter, file_search, function).

### Assistant Definition (JSON Schema)

```json
{
  "name": "Multimodal Analyst",
  "description": "Analyzes text, images, and code",
  "instructions": "You are a multimodal analyst. Analyze images, run code, search files.",
  "model": "gpt-4o",
  "tools": [
    { "type": "code_interpreter" },
    { "type": "file_search" },
    {
      "type": "function",
      "function": {
        "name": "generate_image",
        "description": "Generate an image from a text prompt",
        "parameters": {
          "type": "object",
          "properties": {
            "prompt": { "type": "string" },
            "size": { "type": "string", "enum": ["1024x1024", "512x512"] }
          },
          "required": ["prompt"]
        }
      }
    }
  ],
  "tool_resources": {
    "file_search": {
      "vector_store_ids": ["vs_abc123"]
    }
  },
  "temperature": 0.2,
  "top_p": 1,
  "response_format": {
    "type": "json_schema",
    "json_schema": { "name": "analysis", "schema": { "..." : "..." } }
  }
}
```

### Modality Capabilities by Model

| Model | Text | Vision | Code | Audio | Image Gen |
|-------|:----:|:------:|:----:|:-----:|:---------:|
| gpt-4o | x | x | x | x (Realtime API) | via function |
| gpt-4o-mini | x | x | x | -- | via function |
| gpt-4-turbo | x | x | x | -- | via function |
| o3-mini | x | -- | x | -- | -- |

### Key Insight

OpenAI has **no modality enum on assistants**. Capabilities are:
- **Implicit** from model (gpt-4o = vision + text + audio)
- **Explicit** from tools array (code_interpreter, file_search, function)
- **Extended** by Realtime API for audio/speech (separate from Assistants)

---

## 7. Anthropic Messages API

### Architecture

Anthropic uses **content blocks** in messages. Each message contains an array of typed blocks.
This is the most explicit multi-modal content typing in any API.

### Content Block Types

```json
// TEXT block
{ "type": "text", "text": "Analyze this image:" }

// IMAGE block (base64)
{
  "type": "image",
  "source": {
    "type": "base64",
    "media_type": "image/png",
    "data": "iVBORw0KGgo..."
  }
}

// IMAGE block (URL)
{
  "type": "image",
  "source": {
    "type": "url",
    "url": "https://example.com/image.png"
  }
}

// TOOL_USE block (LLM requests tool call)
{
  "type": "tool_use",
  "id": "toolu_01A...",
  "name": "get_weather",
  "input": { "location": "Paris" }
}

// TOOL_RESULT block (response to tool call -- supports multi-modal!)
{
  "type": "tool_result",
  "tool_use_id": "toolu_01A...",
  "content": [
    { "type": "text", "text": "Weather data for Paris" },
    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "..." } }
  ]
}
```

### Tool Definition

```json
{
  "name": "analyze_document",
  "description": "Analyze a document with text and images",
  "input_schema": {
    "type": "object",
    "properties": {
      "document_url": { "type": "string" },
      "analysis_type": {
        "type": "string",
        "enum": ["summary", "extraction", "visual_analysis"]
      }
    },
    "required": ["document_url"]
  }
}
```

### Key Insight

Anthropic's **content block** system is the most modality-aware API design:
- Each block has an explicit `type` (text, image, tool_use, tool_result)
- `tool_result` can return **multi-modal content** (text + images together)
- No separate "modality" field -- it is embedded in the content structure itself

---

## 8. Letta/MemGPT

### Architecture

Letta treats each agent as a **stateful service** with persistent memory blocks, tools, and
a specific model. The agent loop is ReAct-style with memory management tools built in.

### Agent Creation Schema

```python
from letta_client import Letta

client = Letta(base_url="http://localhost:8283")

agent = client.agents.create(
    name="multimodal-assistant",
    model="openai/gpt-4o",                           # LLM model
    embedding_model="openai/text-embedding-3-small",  # for memory search
    system_prompt="You are a helpful assistant with persistent memory.",
    tools=[
        {
            "type": "function",
            "function": {
                "name": "analyze_image",
                "description": "Analyze an image from URL",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "image_url": { "type": "string" }
                    },
                    "required": ["image_url"]
                }
            }
        }
    ],
    memory_blocks=[
        { "label": "persona",  "value": "I am a multimodal AI assistant." },
        { "label": "human",    "value": "User preferences and history." },
        { "label": "context",  "value": "Current project context." }
    ],
    metadata={ "version": "1.0", "modalities": ["text", "image"] },
    tags=["multimodal", "production"]
)
```

### REST API Equivalent

```json
POST /agents
{
  "name": "multimodal-assistant",
  "model": "openai/gpt-4o",
  "embedding_model": "openai/text-embedding-3-small",
  "system_prompt": "You are a helpful assistant.",
  "tools": [ ... ],
  "memory_blocks": [
    { "label": "persona", "value": "..." },
    { "label": "human", "value": "..." }
  ],
  "metadata": { "version": "1.0" },
  "tags": ["multimodal"],
  "response_format": {
    "type": "json_schema",
    "json_schema": { "..." : "..." }
  }
}
```

### Key Insight

Letta is **single-agent focused** with memory as the differentiator. Multi-modal handling is
entirely via model choice and tool definitions. The `metadata` and `tags` fields are free-form,
so you could add modality hints, but nothing is enforced.

---

## 9. Google A2A Protocol

### Architecture

The A2A (Agent-to-Agent) protocol defines **AgentCards** -- JSON manifests that describe what an
agent can do. This is the **most structured capability definition** in the ecosystem.

### AgentCard Schema

```json
{
  "name": "Multi-Modal Analyst",
  "description": "Analyzes text, images, audio, and generates reports",
  "url": "https://agent.example.com/a2a",
  "provider": {
    "organization": "Example Corp",
    "url": "https://example.com"
  },
  "version": "1.0.0",
  "documentationUrl": "https://docs.example.com/agent",
  "capabilities": {
    "streaming": true,
    "pushNotifications": false,
    "stateTransitionHistory": true
  },
  "authentication": {
    "schemes": ["bearer"],
    "credentials": "oauth2"
  },
  "defaultInputModes": ["text/plain", "application/json"],
  "defaultOutputModes": ["text/plain", "application/json"],
  "skills": [
    {
      "id": "text-analysis",
      "name": "Text Analysis",
      "description": "Analyze text documents for sentiment, entities, and key themes",
      "tags": ["nlp", "sentiment", "entities"],
      "examples": [
        "Analyze the sentiment of this customer review",
        "Extract named entities from this article"
      ],
      "inputModes": ["text/plain", "application/pdf"],
      "outputModes": ["application/json", "text/markdown"]
    },
    {
      "id": "image-analysis",
      "name": "Image Analysis",
      "description": "Analyze images for objects, text (OCR), and visual content",
      "tags": ["vision", "ocr", "image"],
      "examples": [
        "What objects are in this image?",
        "Extract text from this screenshot"
      ],
      "inputModes": ["image/png", "image/jpeg", "image/webp"],
      "outputModes": ["application/json", "text/plain"]
    },
    {
      "id": "audio-transcription",
      "name": "Audio Transcription",
      "description": "Transcribe audio files to text with speaker diarization",
      "tags": ["audio", "transcription", "speech"],
      "inputModes": ["audio/mpeg", "audio/wav", "audio/webm"],
      "outputModes": ["text/plain", "application/json"]
    },
    {
      "id": "code-generation",
      "name": "Code Generation",
      "description": "Generate and execute code based on natural language instructions",
      "tags": ["code", "python", "execution"],
      "inputModes": ["text/plain"],
      "outputModes": ["text/x-python", "application/json"]
    }
  ]
}
```

### Capability Discovery

```
Client                          Agent Server
  |                                 |
  |  GET /.well-known/agent.json    |
  |-------------------------------->|
  |                                 |
  |  200 OK { AgentCard }           |
  |<--------------------------------|
  |                                 |
  |  (parse skills, match modality) |
  |                                 |
  |  POST /a2a (task request)       |
  |-------------------------------->|
```

### Key Insight

A2A is the **gold standard for capability declaration**:
- `skills[].inputModes` / `outputModes` use **MIME types** (not enums)
- `tags` provide semantic discovery (searchable keywords)
- `examples` help LLM-based routers understand when to use this skill
- `capabilities` describe protocol features (streaming, push notifications)

This is the closest to what Nika would need for multi-modal worker manifests.

---

## 10. MCP Protocol

### Architecture

MCP (Model Context Protocol) defines **server capabilities** discovered during initialization.
Servers expose tools, resources, and prompts.

### ServerCapabilities (initialize response)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2025-11-25",
    "capabilities": {
      "tools": {
        "listChanged": true
      },
      "resources": {
        "subscribe": true,
        "listChanged": true
      },
      "prompts": {
        "listChanged": true
      },
      "logging": {}
    },
    "serverInfo": {
      "name": "novanet-mcp",
      "version": "0.20.0"
    }
  }
}
```

### Tool Definition

```json
{
  "name": "novanet_context",
  "description": "Build LLM context from knowledge graph",
  "inputSchema": {
    "type": "object",
    "properties": {
      "focus_key": { "type": "string" },
      "locale": { "type": "string" },
      "mode": { "type": "string", "enum": ["page", "block", "knowledge", "assemble"] }
    },
    "required": ["focus_key"]
  },
  "annotations": {
    "title": "NovaNet Context Builder",
    "readOnlyHint": true,
    "openWorldHint": false
  }
}
```

### Key Insight

MCP is **tool-level granularity**, not agent-level. Each tool has an `inputSchema` but no
modality metadata. The protocol is about **what operations are available**, not what content
types are supported. For multi-modal, you would need to express modality in the tool's
description and input schema properties.

---

## 11. Capability Definition Patterns

### Pattern A: Modality Enum (Strict)

```yaml
# Hypothetical -- no framework uses this exactly
worker:
  name: vision-worker
  modality: image        # enum: text | image | audio | video | code | embedding
  model: gpt-4o
  tools: [analyze_image, ocr_extract]
```

**Pros**: Simple, type-safe, fast matching
**Cons**: Single modality per worker, rigid, does not capture mixed-modality workers

### Pattern B: Capability List (Structured)

```yaml
# Closest to A2A skills
worker:
  name: multimodal-analyst
  capabilities:
    - id: text-analysis
      input_types: [text/plain, application/pdf]
      output_types: [application/json]
    - id: image-analysis
      input_types: [image/png, image/jpeg]
      output_types: [text/plain, application/json]
  model: gpt-4o
  tools: [analyze_image, extract_text]
```

**Pros**: Rich, discoverable, supports mixed modality
**Cons**: More complex, requires matching logic

### Pattern C: Tags (Flexible)

```yaml
# Closest to CrewAI + Letta
worker:
  name: analyst
  tags: [text, image, nlp, vision, ocr]
  model: gpt-4o
  tools: [analyze_image, extract_text]
```

**Pros**: Simple, extensible, searchable
**Cons**: No structure, ambiguous, hard to validate

### Pattern D: Model-Implies-Modality (Implicit)

```yaml
# How most frameworks actually work
worker:
  name: analyst
  model: gpt-4o          # implies: text + vision
  tools:
    - code_interpreter    # implies: code execution
    - file_search         # implies: document processing
```

**Pros**: Zero configuration overhead
**Cons**: Implicit, not discoverable, changes with model updates

### Recommendation

**VALIDATED (2026-03-15): Approach C + Progressive Disclosure**

We chose a hybrid approach that combines the richness of Pattern B (Capability List) with the
simplicity of Pattern D (Model-Implies-Modality), using progressive disclosure:

- **Level 1 (Minimal):** Model selection implies baseline modalities (Pattern D)
- **Level 2 (Medium):** Explicit `accepts`/`produces` at top level for clarity
- **Level 3 (Full):** Complete `capabilities` array with MIME types, tags, examples (Pattern B)

Key design principles:
- Model selection implies baseline modalities
- Explicit `capabilities` or `skills` array for advanced routing
- Tags for search/discovery
- MIME types for input/output specification (following A2A)
- Capability inference from model + tools when not explicitly declared

See **Section 14-19** for the full validated design.

---

## 12. Routing Architectures

### Architecture 1: Classifier Node (LangGraph pattern)

```
Input --> [Classifier] --text--> [Text Worker]
                       --image-> [Vision Worker]
                       --code--> [Code Worker]
                       --audio-> [Audio Worker]
```

The classifier can be:
- **Rule-based**: Check content type, file extension, keywords
- **LLM-based**: Ask an LLM to classify the input modality
- **Hybrid**: Rules first, LLM fallback

### Architecture 2: LLM Selector (AutoGen pattern)

```
Input --> [Selector LLM] --> picks agent from pool
              |
              v
    "Based on the conversation, which agent
     should handle this? Options:
     - text_analyst (text tasks)
     - vision_analyst (image tasks)
     - code_executor (code tasks)"
```

### Architecture 3: Task Assignment (CrewAI pattern)

```
Workflow definition:
  Task 1 --> Agent A (researcher)
  Task 2 --> Agent B (image_analyst)
  Task 3 --> Agent C (code_developer)
```

No runtime routing -- assignment is static at workflow definition time.

### Architecture 4: Capability Negotiation (A2A pattern)

```
Orchestrator                     Worker Registry
    |                                |
    |  "I need image/png analysis"   |
    |------------------------------->|
    |                                |  (match skills[].inputModes)
    |  [Agent A: score 0.95]         |
    |  [Agent B: score 0.72]         |
    |<-------------------------------|
    |                                |
    |  POST /a2a to Agent A          |
    |------------------------------->|
```

### Architecture 5: Tool-Based Dispatch (Letta/Anthropic pattern)

```
Single Agent with multi-modal tools:
  Agent --> [tool: analyze_image] for images
        --> [tool: transcribe_audio] for audio
        --> [tool: execute_code] for code
        --> direct LLM for text
```

The LLM itself decides which tool to call based on the input.

### Comparison

| Architecture | Determinism | Flexibility | Overhead | Best For |
|-------------|:-----------:|:-----------:|:--------:|----------|
| Classifier Node | High | Medium | Low | Known modality set |
| LLM Selector | Low | High | High (LLM call) | Dynamic teams |
| Task Assignment | Highest | Lowest | Zero | Static workflows |
| Capability Negotiation | High | High | Medium | Federated agents |
| Tool-Based Dispatch | Medium | High | Low | Single agent, many modalities |

---

## 13. Synthesis for Nika

### What Nika Can Learn from Each Framework

| Framework | Takeaway for Nika |
|-----------|-------------------|
| **LangGraph** | TypedDict state with `input_modality` field; conditional edges for routing |
| **CrewAI** | YAML-first agent definitions with role/goal/backstory; task-to-agent binding |
| **AutoGen 0.4** | `SelectorGroupChat` with selector prompt for LLM-based routing |
| **Semantic Kernel** | Multi-model agents where kernel determines capability |
| **OpenAI Assistants** | Tools array as capability declaration (code_interpreter, file_search, function) |
| **Anthropic** | Content block types (text/image/tool_use/tool_result) for multi-modal payloads |
| **Letta** | Memory blocks + tags + metadata for agent state |
| **A2A Protocol** | `skills[].inputModes/outputModes` with MIME types -- best capability schema |
| **MCP** | Tool-level inputSchema for structured capability discovery |

### Proposed Nika Worker Definition

> **NOTE (2026-03-15):** This section shows the original research proposal. The **validated design**
> with progressive disclosure is documented in **Sections 14-19**. The core structure remains the
> same, but the validated design adds three levels of complexity and capability inference.

Drawing from A2A (capability manifest) + CrewAI (YAML-first) + Anthropic (content blocks):

```yaml
# Hypothetical Nika satellite/worker definition
satellites:
  - id: text-analyst
    name: Text Analyst
    role: Analyze text documents and extract insights
    model:
      provider: anthropic
      name: claude-sonnet-4-20250514
    system: |
      You are a text analysis specialist. Extract key themes,
      entities, and sentiment from documents.
    capabilities:
      - id: text-analysis
        input_modes: [text/plain, text/markdown, application/pdf]
        output_modes: [application/json, text/markdown]
        tags: [nlp, sentiment, entities, summarization]
      - id: translation
        input_modes: [text/plain]
        output_modes: [text/plain]
        tags: [i18n, locale]
    tools:
      - novanet_context
      - novanet_search
    max_tokens: 4096
    temperature: 0.3

  - id: vision-analyst
    name: Vision Analyst
    role: Analyze images and visual content
    model:
      provider: openai
      name: gpt-4o
    system: |
      You specialize in visual analysis, OCR, and
      image understanding.
    capabilities:
      - id: image-analysis
        input_modes: [image/png, image/jpeg, image/webp]
        output_modes: [application/json, text/plain]
        tags: [vision, ocr, object-detection]
    tools:
      - nika:read        # read image files
      - nika:glob        # find image files
    max_tokens: 2048

  - id: code-worker
    name: Code Worker
    role: Generate and execute code
    model:
      provider: anthropic
      name: claude-sonnet-4-20250514
    system: |
      You write clean, tested Python code.
    capabilities:
      - id: code-generation
        input_modes: [text/plain]
        output_modes: [text/x-python, text/x-rust, application/json]
        tags: [code, python, rust, execution]
      - id: code-review
        input_modes: [text/x-python, text/x-rust]
        output_modes: [text/markdown]
        tags: [review, analysis]
    tools:
      - nika:read
      - nika:write
      - nika:edit
    max_tokens: 8192

  - id: embedding-worker
    name: Embedding Worker
    role: Generate and search embeddings
    model:
      provider: native
      name: nomic-embed-text-v1.5
    capabilities:
      - id: embedding-generation
        input_modes: [text/plain]
        output_modes: [application/x-embedding]
        tags: [embedding, vector, similarity]
    # no tools -- pure inference

# Routing configuration
routing:
  strategy: classifier          # classifier | round-robin | static | capability-match
  classifier:
    model: anthropic/claude-haiku  # cheap model for classification
    prompt: |
      Given the input, classify which satellite should handle it.
      Available satellites: {satellites}
      Respond with the satellite id.
  fallback: text-analyst         # default if classification fails
```

### Key Design Decisions for Nika

1. **Capability = skills array with MIME-typed inputModes/outputModes** (from A2A)
2. **Tags for fuzzy matching** (from Letta/CrewAI)
3. **YAML-first definitions** (consistent with Nika's existing workflow format)
4. **Routing as a configurable strategy** (classifier/round-robin/static/capability-match)
5. **Model-per-worker** not model-per-task (each satellite owns its model config)
6. **Tools-as-capabilities** (tools a worker can use define what it can do)

---

## 14. Progressive Disclosure Design

Following the validated decision, Nika satellites support three levels of definition complexity.
This allows simple cases to remain simple while enabling full capability specification when needed.

### Level 1 — Minimal (Capabilities Inferred)

When only `id` and `model` are provided, Nika infers capabilities from the model's known modalities:

```yaml
satellites:
  - id: vision
    model: openai/gpt-4o
```

**What Nika infers:**
- `accepts: [text/plain, text/markdown, image/png, image/jpeg, image/webp]` (from gpt-4o's vision capability)
- `produces: [text/plain, text/markdown, application/json]` (default LLM outputs)
- No tools (pure inference)

**Use case:** Quick prototyping, simple single-modality tasks.

### Level 2 — Medium (Explicit accepts/produces)

When you need to constrain or clarify what a satellite handles, declare `accepts`, `produces`, and
`tools` at the top level:

```yaml
satellites:
  - id: vision
    model: openai/gpt-4o
    system: "You analyze images for visual content and text extraction."
    accepts: [image/png, image/jpeg, image/webp]
    produces: [text/markdown, application/json]
    tools: [nika:read, nika:glob]
```

**When to use:**
- You want to restrict a vision-capable model to only handle images (not text)
- You need specific tools attached
- You want explicit documentation of capabilities

### Level 3 — Full (Capabilities Array)

For complex satellites with multiple distinct skills, use the full `capabilities` array inspired
by A2A AgentCard:

```yaml
satellites:
  - id: multimodal-analyst
    model: openai/gpt-4o
    system: "You are a senior analyst specializing in document and visual analysis."
    capabilities:
      - id: image-analysis
        description: "Analyze images for objects, text (OCR), and visual content"
        accepts: [image/png, image/jpeg, image/webp]
        produces: [application/json, text/plain]
        tags: [vision, ocr, object-detection]
        examples:
          - "What's in this image?"
          - "Extract text from this screenshot"
          - "Identify the main objects in this photo"
      - id: document-review
        description: "Review and summarize text documents"
        accepts: [text/plain, text/markdown, application/pdf]
        produces: [text/markdown, application/json]
        tags: [nlp, summary, review, extraction]
        examples:
          - "Summarize this document"
          - "Extract key points from this report"
    tools: [nika:read, nika:glob, novanet_context]
    max_tokens: 4096
    temperature: 0.3
```

**When to use:**
- Satellite handles multiple distinct types of tasks
- You need fine-grained routing based on specific capabilities
- You want rich metadata for LLM-based selector prompts
- Documentation and discoverability are important

### Schema Summary

| Level | Fields Required | Capabilities | Routing |
|-------|----------------|--------------|---------|
| **1 (Minimal)** | `id`, `model` | Auto-inferred from model | By model modality |
| **2 (Medium)** | + `accepts`, `produces`, `tools` | Explicit top-level | By declared MIME types |
| **3 (Full)** | + `capabilities[]` array | Per-capability granularity | By capability match + tags |

---

## 15. Capability Inference

When a satellite is defined minimally (Level 1), Nika automatically infers its capabilities from
two sources:

### Source 1: Model Modalities

Each known model has associated modalities based on provider documentation:

| Model | Modalities | Inferred accepts |
|-------|------------|------------------|
| `openai/gpt-4o` | text, vision | `text/*, image/png, image/jpeg, image/webp` |
| `openai/gpt-4o-mini` | text, vision | `text/*, image/png, image/jpeg, image/webp` |
| `anthropic/claude-sonnet-4-20250514` | text | `text/*, application/pdf` |
| `anthropic/claude-3-opus` | text, vision | `text/*, image/*` |
| `native/qwen2-vl-*` | text, vision | `text/*, image/*` |
| `native/llama3*` | text | `text/*` |
| `google/gemini-2.0-flash` | text, vision, audio | `text/*, image/*, audio/*` |

### Source 2: Tool Input Schemas

When tools are attached, their input schemas contribute to capability inference:

```yaml
satellites:
  - id: file-processor
    model: anthropic/claude-sonnet-4-20250514
    tools: [nika:read, nika:glob]
```

**Inference logic:**
- `nika:read` accepts file paths → satellite can handle `text/plain`, `text/markdown`, etc.
- `nika:glob` searches for files → satellite can discover and process file patterns
- Combined capabilities = model modalities ∪ tool input schemas

### Inference Algorithm

```
function infer_capabilities(satellite):
    base_modalities = MODEL_REGISTRY[satellite.model].modalities
    tool_modalities = []

    for tool in satellite.tools:
        schema = get_tool_input_schema(tool)
        tool_modalities.extend(extract_mime_types(schema))

    return deduplicate(base_modalities + tool_modalities)
```

### Override Behavior

Explicit declaration always wins over inference:

```yaml
satellites:
  - id: text-only-gpt4o
    model: openai/gpt-4o           # Has vision capability
    accepts: [text/plain]          # But we restrict to text only
```

---

## 16. Shaka Routing Algorithm

The Shaka orchestrator uses capability-based routing with LLM fallback for ambiguous cases.

### Routing Flow

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  SHAKA ROUTING ALGORITHM                                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  1. DETECT INPUT MIME TYPE(S)                                                   │
│     └── Analyze incoming request payload                                        │
│         ├── Image file → image/png, image/jpeg, etc.                            │
│         ├── Text prompt → text/plain                                            │
│         ├── JSON data → application/json                                        │
│         └── Mixed → [text/plain, image/png, ...]                                │
│                                                                                 │
│  2. FILTER SATELLITES BY ACCEPTS                                                │
│     └── candidates = satellites.filter(s => s.accepts.intersects(input_mime))   │
│                                                                                 │
│  3. ROUTE DECISION                                                              │
│     ├── If candidates.len() == 0 → fallback satellite                           │
│     ├── If candidates.len() == 1 → route directly (no LLM call)                 │
│     └── If candidates.len() > 1  → LLM selector picks best match                │
│                                                                                 │
│  4. LLM SELECTOR (when multiple candidates)                                     │
│     └── Small model (e.g., claude-haiku) with context:                          │
│         ├── Input description                                                   │
│         ├── Goal/task context from shaka: block                                 │
│         ├── Candidate satellites with descriptions + tags                       │
│         └── Returns: satellite_id                                               │
│                                                                                 │
│  5. EXECUTE                                                                     │
│     └── Route request to selected satellite                                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Configuration

```yaml
shaka:
  routing:
    strategy: capability-match     # capability-match | round-robin | static
    selector:
      model: anthropic/claude-haiku-3-5-20241022  # cheap model for selection
      prompt: |
        Given the input and available satellites, select the most appropriate one.
        Consider:
        - Input modality ({{input_mime_types}})
        - Task context ({{goal}})
        - Satellite capabilities and tags

        Available satellites:
        {{#each candidates}}
        - {{id}}: {{description}} [tags: {{tags}}]
        {{/each}}

        Respond with just the satellite id.
    fallback: general-assistant    # default if no match or selection fails

satellites:
  - id: vision-analyst
    model: openai/gpt-4o
    accepts: [image/png, image/jpeg, image/webp]
    produces: [text/markdown, application/json]
    tags: [vision, ocr, analysis]

  - id: text-processor
    model: anthropic/claude-sonnet-4-20250514
    accepts: [text/plain, text/markdown, application/pdf]
    produces: [text/markdown, application/json]
    tags: [nlp, summary, extraction]

  - id: general-assistant
    model: anthropic/claude-sonnet-4-20250514
    accepts: [text/plain]
    produces: [text/plain, text/markdown]
    tags: [general, fallback]
```

### Routing Examples

| Input | MIME Detection | Candidates | Decision |
|-------|----------------|------------|----------|
| "Analyze this photo" + image.png | `[text/plain, image/png]` | `[vision-analyst]` | Direct route |
| "Summarize this report" + report.pdf | `[text/plain, application/pdf]` | `[text-processor]` | Direct route |
| "What is this?" + image.png | `[text/plain, image/png]` | `[vision-analyst, general-assistant]` | LLM selects vision-analyst |
| "Hello world" | `[text/plain]` | `[text-processor, general-assistant]` | LLM selects based on goal |
| "Process this video" + video.mp4 | `[video/mp4]` | `[]` | Fallback to general-assistant |

---

## 17. Image Generation Strategy

Rather than creating dedicated "image generation satellites", the recommended approach is to use
MCP tools that satellites can call when they need to generate images.

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  IMAGE GENERATION VIA MCP TOOLS                                                 │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  User Request                                                                   │
│       │                                                                         │
│       ▼                                                                         │
│  ┌─────────────────────────────────────────────────────────────────────┐       │
│  │  Satellite: creative-director                                        │       │
│  │  model: anthropic/claude-sonnet-4-20250514                           │       │
│  │  tools: [image-gen:generate_image, image-gen:edit_image]             │       │
│  │                                                                       │       │
│  │  The LLM DECIDES when to call the tool based on the task.            │       │
│  │  It crafts the prompt, selects parameters, iterates if needed.       │       │
│  └─────────────────────────────────────────────────────────────────────┘       │
│       │                                                                         │
│       │  tool_call: image-gen:generate_image                                    │
│       │  params: { prompt: "A cyberpunk city at sunset", style: "digital_art" } │
│       ▼                                                                         │
│  ┌─────────────────────────────────────────────────────────────────────┐       │
│  │  MCP Server: image-gen                                               │       │
│  │  backend: stable-diffusion-xl | dall-e-3 | midjourney-api            │       │
│  │                                                                       │       │
│  │  Handles actual generation, returns image URL or base64.             │       │
│  └─────────────────────────────────────────────────────────────────────┘       │
│       │                                                                         │
│       ▼                                                                         │
│  Image returned to satellite → satellite continues conversation                 │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Satellite Configuration

```yaml
satellites:
  - id: creative-director
    model: anthropic/claude-sonnet-4-20250514
    system: |
      You are a creative director specializing in visual content.
      When asked to create images, use the generate_image tool.
      When asked to modify images, use the edit_image tool.

      Always consider:
      - Composition and visual hierarchy
      - Color theory and mood
      - Target audience and context

      Iterate on generations if the result doesn't match requirements.
    tools:
      - image-gen:generate_image
      - image-gen:edit_image
      - image-gen:upscale_image
    accepts: [text/plain, text/markdown]
    produces: [image/png, text/markdown]
    tags: [creative, image-gen, design, visual]
    max_tokens: 4096
```

### MCP Server Definition

```yaml
mcp:
  servers:
    image-gen:
      command: "node"
      args: ["./mcp-servers/image-gen/dist/index.js"]
      env:
        OPENAI_API_KEY: "${OPENAI_API_KEY}"        # For DALL-E
        STABILITY_API_KEY: "${STABILITY_API_KEY}"  # For Stable Diffusion
```

### Why MCP Tools Over Dedicated Workers?

| Approach | Pros | Cons |
|----------|------|------|
| **MCP Tools (Recommended)** | LLM controls when/how to generate; iterative refinement; works with any LLM satellite | Requires MCP server setup |
| Dedicated image-gen satellite | Simpler routing | No decision-making; hard to iterate; requires separate routing logic |

**Key Insight:** Image generation is an action the satellite takes, not a modality the satellite
processes. The satellite (with its LLM brain) decides when to generate, what prompt to use, and
whether to iterate -- just like a human creative director would.

---

## 18. Local Native Models

Satellites can use local GGUF models via Nika's native inference runtime (mistral.rs) introduced
in v0.26.0 (ADR-008).

### Basic Native Configuration

```yaml
satellites:
  - id: local-assistant
    model:
      provider: native
      name: llama3.2-3b-q4.gguf
    system: "You are a helpful local assistant."
    accepts: [text/plain]
    produces: [text/markdown]
```

### Native Vision Models

```yaml
satellites:
  - id: local-vision
    model:
      provider: native
      name: qwen2-vl-2b-q4.gguf
    system: "You analyze images locally."
    accepts: [image/png, image/jpeg, text/plain]
    produces: [text/markdown]
    tags: [vision, local, privacy]
```

### Fallback Configuration

For production scenarios where local model quality may not suffice, configure a cloud fallback:

```yaml
satellites:
  - id: hybrid-analyst
    model:
      provider: native
      name: qwen2-vl-2b-q4.gguf
      fallback:
        provider: openai
        name: gpt-4o
    system: "Analyze visual content with high accuracy."
    accepts: [image/png, image/jpeg, image/webp]
    produces: [text/markdown, application/json]
    tags: [vision, hybrid]
```

**Fallback triggers:**
- Local model returns low-confidence response
- Local model fails (OOM, timeout)
- Task complexity exceeds local model capability (configurable threshold)

### Native Model Benefits

| Benefit | Description |
|---------|-------------|
| **Privacy** | Data never leaves the machine |
| **Cost** | No API charges |
| **Latency** | No network round-trip (after model load) |
| **Offline** | Works without internet |
| **Development** | Free iteration during development |

### Native Model Limitations

| Limitation | Mitigation |
|------------|------------|
| Model size | Use quantized models (q4, q5) |
| Quality | Use fallback for production |
| Memory | Metal/CUDA acceleration, batching |
| First load | Keep daemon running, preload |

### Model Registry

Nika's `KNOWN_MODELS` registry (v0.27.0) includes curated native models:

```
llama3.2:1b, llama3.2:3b, llama3.1:8b
qwen3:1.7b, qwen3:4b, qwen3:8b
phi4:14b, mistral:7b, gemma3:4b
qwen2.5-coder:7b
```

---

## 19. MCP as Capability Extender

MCP tools act as "hands" for satellites, extending what they can do beyond pure inference.

### Capability Extension Pattern

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  SATELLITE = LLM BRAIN + MCP TOOL HANDS                                         │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  ┌──────────────────────────────────────┐                                       │
│  │          Satellite LLM               │                                       │
│  │    (decides what to do, when)        │                                       │
│  └──────────────────────────────────────┘                                       │
│       │         │         │         │                                           │
│       ▼         ▼         ▼         ▼                                           │
│  ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐                                   │
│  │novanet │ │browser │ │image-  │ │github  │                                   │
│  │_*      │ │:*      │ │gen:*   │ │:*      │                                   │
│  │        │ │        │ │        │ │        │                                   │
│  │Knowledge│ │ Web   │ │ Image  │ │ Repo   │                                   │
│  │Graph   │ │Browser │ │ Gen    │ │ Mgmt   │                                   │
│  └────────┘ └────────┘ └────────┘ └────────┘                                   │
│                                                                                 │
│  MCP Tools extend capability WITHOUT changing the satellite's core model.       │
│  The LLM's intelligence + tools' capabilities = satellite's total capability.   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Common MCP Tool Categories

| Category | Tools | Capability Added |
|----------|-------|------------------|
| **Knowledge** | `novanet_context`, `novanet_search` | Entity knowledge, SEO data, locale context |
| **Files** | `nika:read`, `nika:write`, `nika:edit`, `nika:glob` | File system access |
| **Web** | `browser:navigate`, `browser:screenshot`, `browser:extract` | Web browsing, scraping |
| **Image** | `image-gen:generate`, `image-gen:edit`, `image-gen:upscale` | Image creation/modification |
| **Code** | `github:search_repos`, `github:create_pr`, `github:review` | Repository management |
| **Communication** | `slack:send_message`, `email:send` | External messaging |

### Full Satellite Example

```yaml
satellites:
  - id: research-analyst
    model: anthropic/claude-sonnet-4-20250514
    system: |
      You are a senior research analyst with access to multiple tools.

      Use novanet_* tools to access the knowledge graph.
      Use browser tools to research the web when needed.
      Use nika:* tools to read/write local files.

      Always cite your sources and cross-reference information.
    capabilities:
      - id: entity-research
        description: "Research entities in the knowledge graph"
        accepts: [text/plain]
        produces: [text/markdown, application/json]
        tags: [research, knowledge-graph, entities]
      - id: web-research
        description: "Research topics on the web"
        accepts: [text/plain]
        produces: [text/markdown]
        tags: [research, web, scraping]
      - id: report-generation
        description: "Generate comprehensive reports"
        accepts: [text/plain, application/json]
        produces: [text/markdown, application/pdf]
        tags: [reports, writing, synthesis]
    tools:
      # Knowledge Graph (NovaNet)
      - novanet_context
      - novanet_search
      # Web Browsing
      - browser:navigate
      - browser:screenshot
      - browser:extract_text
      # Local Files
      - nika:read
      - nika:write
      - nika:glob
    max_tokens: 8192
    temperature: 0.2
```

### Tool Discovery

Nika discovers available tools from configured MCP servers:

```yaml
mcp:
  servers:
    novanet:
      command: "cargo run --manifest-path ../novanet/Cargo.toml -- mcp"
    browser:
      command: "npx @anthropic/browser-mcp"
    image-gen:
      command: "node ./mcp-servers/image-gen/dist/index.js"

# Tools are discovered at runtime via MCP initialize
# Satellites reference them by server:tool_name pattern
```

---

## Sources

1. LangGraph Documentation -- https://docs.langchain.com/oss/python/langgraph/thinking-in-langgraph
2. CrewAI Documentation -- https://docs.crewai.com/en/concepts/agents
3. AutoGen v0.4 Blog -- https://devblogs.microsoft.com/autogen/autogen-reimagined-launching-autogen-0-4/
4. Semantic Kernel Agent Framework -- https://learn.microsoft.com/en-us/semantic-kernel/frameworks/agent/
5. Semantic Kernel Multi-Model Blog -- https://devblogs.microsoft.com/semantic-kernel/guest-blog-building-multi-agent-systems-with-multi-models-in-semantic-kernel-part-1/
6. OpenAI Assistants API v2 -- https://help.openai.com/en/articles/8550641-assistants-api-v2-faq
7. Anthropic MCP Specification -- https://modelcontextprotocol.io/specification/2025-11-25
8. Letta Agent API -- https://docs.letta.com/api/resources/agents/methods/create/
9. Google A2A Protocol -- https://github.com/google/A2A
10. AutoGen Architecture Preview -- https://microsoft.github.io/autogen/0.2/blog/2024/10/02/new-autogen-architecture-preview/

---

## Methodology

- **Tools used**: Perplexity search (sonar model), cross-referencing official docs
- **Pages analyzed**: 30+ search results, 10 framework documentation sites
- **Time period covered**: 2024-2025 implementations

## Confidence Level

**Medium-High** -- Framework APIs and patterns are well-documented for text-based multi-agent
systems. Multi-modal support is still emerging in most frameworks (primarily via model selection
+ tools). The A2A protocol's AgentCard schema is the most mature capability definition but is
not yet widely adopted. Anthropic's content block system is the most mature multi-modal API
for actual content handling.
