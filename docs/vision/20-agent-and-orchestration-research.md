# 20 -- Agent & Orchestration Research

> **Research Date**: 2026-03-14 — 2026-03-15
> **Last Updated**: 2026-03-20
> **Scope**: Multi-modal worker architectures, framework routing patterns, agent memory architectures, and Rust implementation patterns
> **Merged from**: Doc 13 (Multimodal Worker Architectures) + Doc 20 (Agent Memory Research)
> **Frameworks Analyzed**: LangGraph, CrewAI, AutoGen 0.4, Semantic Kernel, OpenAI Assistants, Anthropic Messages API, Letta/MemGPT, Google A2A, MCP, Mem0, Cognee
> **Status**: VALIDATED — Approach C with Progressive Disclosure selected

---

# Part 1: Worker & Orchestration Architectures

> How modern frameworks define, configure, and route multi-modal workers.
> **Sources**: 30+ search results, 10 framework documentation sites

---

## Validated Decisions (2026-03-15)

This section summarizes the design decisions validated during the 2026-03-15 brainstorm session with Thibaut.

| Decision | Summary |
|----------|---------|
| **Approach** | **Approach C (Full Capabilities Manifest) + Progressive Disclosure** |
| **Capability Source** | Hybrid: Model modalities + Tool input schemas (auto-inferred) |
| **Routing** | Orchestrator capability-match with LLM fallback for ambiguous cases |
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

### Part 1: Worker & Orchestration Architectures

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
16. [Orchestrator Routing Algorithm](#16-orchestrator-routing-algorithm)
17. [Image Generation Strategy](#17-image-generation-strategy)
18. [Local Native Models](#18-local-native-models)
19. [MCP as Capability Extender](#19-mcp-as-capability-extender)

### Part 2: Agent Memory Architectures

20. [LangGraph Memory](#20-langgraph-memory)
21. [CrewAI Memory](#21-crewai-memory)
22. [Letta / MemGPT Memory](#22-letta--memgpt-memory)
23. [Mem0](#23-mem0)
24. [Cognee](#24-cognee)
25. [Other Cutting-Edge Frameworks](#25-other-cutting-edge-frameworks)
26. [Academic Papers (2025-2026)](#26-academic-papers-2025-2026)
27. [Cross-Framework Memory Comparison](#27-cross-framework-memory-comparison)
28. [Rust Implementation Patterns](#28-rust-implementation-patterns)

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

### Level 1 -- Minimal (Capabilities Inferred)

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

### Level 2 -- Medium (Explicit accepts/produces)

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

### Level 3 -- Full (Capabilities Array)

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
- `nika:read` accepts file paths -> satellite can handle `text/plain`, `text/markdown`, etc.
- `nika:glob` searches for files -> satellite can discover and process file patterns
- Combined capabilities = model modalities + tool input schemas

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

## 16. Orchestrator Routing Algorithm

The orchestrator uses capability-based routing with LLM fallback for ambiguous cases.

### Routing Flow

```
+----------------------------------------------------------------------------------+
|  ORCHESTRATOR ROUTING ALGORITHM                                                  |
+----------------------------------------------------------------------------------+
|                                                                                  |
|  1. DETECT INPUT MIME TYPE(S)                                                    |
|     +-- Analyze incoming request payload                                         |
|         +-- Image file -> image/png, image/jpeg, etc.                            |
|         +-- Text prompt -> text/plain                                            |
|         +-- JSON data -> application/json                                        |
|         +-- Mixed -> [text/plain, image/png, ...]                                |
|                                                                                  |
|  2. FILTER SATELLITES BY ACCEPTS                                                 |
|     +-- candidates = satellites.filter(s => s.accepts.intersects(input_mime))     |
|                                                                                  |
|  3. ROUTE DECISION                                                               |
|     +-- If candidates.len() == 0 -> fallback satellite                           |
|     +-- If candidates.len() == 1 -> route directly (no LLM call)                 |
|     +-- If candidates.len() > 1  -> LLM selector picks best match               |
|                                                                                  |
|  4. LLM SELECTOR (when multiple candidates)                                      |
|     +-- Small model (e.g., claude-haiku) with context:                           |
|         +-- Input description                                                    |
|         +-- Goal/task context from goal: block                                   |
|         +-- Candidate satellites with descriptions + tags                        |
|         +-- Returns: satellite_id                                                |
|                                                                                  |
|  5. EXECUTE                                                                      |
|     +-- Route request to selected satellite                                      |
|                                                                                  |
+----------------------------------------------------------------------------------+
```

### Configuration

```yaml
goal:
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
+----------------------------------------------------------------------------------+
|  IMAGE GENERATION VIA MCP TOOLS                                                  |
+----------------------------------------------------------------------------------+
|                                                                                  |
|  User Request                                                                    |
|       |                                                                          |
|       v                                                                          |
|  +------------------------------------------------------------------+            |
|  |  Satellite: creative-director                                     |            |
|  |  model: anthropic/claude-sonnet-4-20250514                        |            |
|  |  tools: [image-gen:generate_image, image-gen:edit_image]          |            |
|  |                                                                    |            |
|  |  The LLM DECIDES when to call the tool based on the task.         |            |
|  |  It crafts the prompt, selects parameters, iterates if needed.    |            |
|  +------------------------------------------------------------------+            |
|       |                                                                          |
|       |  tool_call: image-gen:generate_image                                     |
|       |  params: { prompt: "A cyberpunk city at sunset", style: "digital_art" }  |
|       v                                                                          |
|  +------------------------------------------------------------------+            |
|  |  MCP Server: image-gen                                            |            |
|  |  backend: stable-diffusion-xl | dall-e-3 | midjourney-api         |            |
|  |                                                                    |            |
|  |  Handles actual generation, returns image URL or base64.          |            |
|  +------------------------------------------------------------------+            |
|       |                                                                          |
|       v                                                                          |
|  Image returned to satellite -> satellite continues conversation                 |
|                                                                                  |
+----------------------------------------------------------------------------------+
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
+----------------------------------------------------------------------------------+
|  SATELLITE = LLM BRAIN + MCP TOOL HANDS                                          |
+----------------------------------------------------------------------------------+
|                                                                                  |
|  +--------------------------------------+                                        |
|  |          Satellite LLM               |                                        |
|  |    (decides what to do, when)        |                                        |
|  +--------------------------------------+                                        |
|       |         |         |         |                                            |
|       v         v         v         v                                            |
|  +--------+ +--------+ +--------+ +--------+                                    |
|  |novanet | |browser | |image-  | |github  |                                    |
|  |_*      | |:*      | |gen:*   | |:*      |                                    |
|  |        | |        | |        | |        |                                    |
|  |Knowledge| | Web   | | Image  | | Repo   |                                    |
|  |Graph   | |Browser | | Gen    | | Mgmt   |                                    |
|  +--------+ +--------+ +--------+ +--------+                                    |
|                                                                                  |
|  MCP Tools extend capability WITHOUT changing the satellite's core model.        |
|  The LLM's intelligence + tools' capabilities = satellite's total capability.    |
|                                                                                  |
+----------------------------------------------------------------------------------+
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

## Part 1 Sources

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

## Part 1 Methodology

- **Tools used**: Perplexity search (sonar model), cross-referencing official docs
- **Pages analyzed**: 30+ search results, 10 framework documentation sites
- **Time period covered**: 2024-2025 implementations

## Part 1 Confidence Level

**Medium-High** -- Framework APIs and patterns are well-documented for text-based multi-agent
systems. Multi-modal support is still emerging in most frameworks (primarily via model selection
+ tools). The A2A protocol's AgentCard schema is the most mature capability definition but is
not yet widely adopted. Anthropic's content block system is the most mature multi-modal API
for actual content handling.

---

---

# Part 2: Agent Memory Architectures

> Raw architectural details from LangGraph, CrewAI, Letta/MemGPT, Mem0, Cognee, and cutting-edge 2025-2026 frameworks.
> Plus Rust implementation patterns for building an append-only agent memory system in Nika.
> Focus: data formats, compression strategies, cross-run persistence, promotion patterns, Rust crates.
>
> **Sources**: 25+ pages scraped, 8 Perplexity queries, 6 framework docs, 30+ Rust source files

---

## 20. LangGraph Memory

### 20.1 Dual Memory Model

LangGraph separates memory into two fundamentally different systems:

| System | Scope | Purpose | Backend |
|--------|-------|---------|---------|
| **Checkpointer** (short-term) | Thread-scoped | Conversation state per thread | InMemorySaver, SQLite, Postgres, Redis, MongoDB |
| **Store** (long-term) | Cross-thread | User/app data across sessions | BaseStore, InMemoryStore, custom backends |

### 20.2 Checkpointing Architecture

#### Core Data Format

Every checkpoint is a `StateSnapshot` containing:

```python
StateSnapshot(
    values={'foo': 'b', 'bar': ['a', 'b']},      # channel_values: serialized state
    next=(),                                        # next nodes to execute
    config={
        'configurable': {
            'thread_id': '1',
            'checkpoint_ns': '',                    # '' = root, 'node:uuid' = subgraph
            'checkpoint_id': '1ef663ba-28fe-6528-8002-5a559208592c'
        }
    },
    metadata={
        'source': 'loop',                           # 'input' or 'loop'
        'writes': {'node_b': {'foo': 'b', 'bar': ['b']}},
        'step': 2                                   # super-step number
    },
    created_at='2024-08-29T19:19:38.821749+00:00',
    parent_config={                                 # linked list of checkpoints
        'configurable': {
            'thread_id': '1',
            'checkpoint_ns': '',
            'checkpoint_id': '1ef663ba-28f9-6ec4-8001-31981c2c39f8'  # parent
        }
    },
    tasks=()
)
```

#### Internal Checkpoint Structure (Binary)

Serialized as MsgPack (default) with optional encryption:

```python
checkpoint = {
    "channel_values": {
        "messages": serialized_messages,  # MsgPack of AIMessage list
        "custom_state": serialized_dict
    },
    "channel_versions": {
        "messages": "v1.2.3",             # version per channel
        "custom_state": "v4.5"
    },
    "versions_seen": {
        "__start__": {"messages": "v1.2.3"},
        "model_node": {"messages": "v1.2.3", "custom_state": "v4.5"}
    },
    "checkpoint_id": "abc123",
    "parent_checkpoint_id": "def456"       # linked list for time-travel
}
```

#### Super-step Boundaries

Checkpoints are created at every **super-step** boundary. For `START -> A -> B -> END`:
- Checkpoint 0: Empty state + `START` as next
- Checkpoint 1: Input state + `node_a` as next
- Checkpoint 2: After `node_a` + `node_b` as next
- Checkpoint 3: After `node_b` + no next (complete)

Each forms a linked list via `parent_checkpoint_id`, enabling full time-travel.

#### Subgraph Namespacing

```
checkpoint_ns = ""                          # root graph
checkpoint_ns = "node_name:uuid"            # subgraph
checkpoint_ns = "outer:uuid|inner:uuid"     # nested subgraphs (| separator)
```

#### Pending Writes (Fault Tolerance)

When a node fails mid-super-step, LangGraph stores **pending writes** from successful nodes. On resume, successful nodes are NOT re-run.

### 20.3 Checkpointer Backends

| Backend | Package | Use Case | Persistence |
|---------|---------|----------|-------------|
| `InMemorySaver` | built-in | Dev/testing | Ephemeral (lost on restart) |
| `SqliteSaver` | `langgraph-checkpoint-sqlite` | Single-machine | File-based |
| `PostgresSaver` | `langgraph-checkpoint-postgres` | Production multi-machine | PostgreSQL |
| `AsyncPostgresSaver` | same package | Production async | PostgreSQL |
| `MongoDBSaver` | `langgraph-checkpoint-mongodb` | Document-oriented | MongoDB |
| `RedisSaver` | `langgraph-checkpoint-redis` | High-perf caching | Redis |

All implement `BaseCheckpointSaver` with methods:
- `put(config, checkpoint, metadata)` -- save
- `get(config)` -- latest checkpoint for thread
- `list(config)` -- checkpoint history
- `get_tuple(config)` -- checkpoint + metadata

#### Serialization

Default: **MsgPack** binary format. Optional Python-native serialization fallback for custom types. Encryption available via custom serializer wrapper.

### 20.4 Thread Model

```python
config = {"configurable": {"thread_id": "user-123"}}

# Thread isolates ALL state
graph.invoke({"messages": [("user", "Hi, I'm Alice")]}, config)
# Later...
graph.invoke({"messages": [("user", "What's my name?")]}, config)
# -> "Alice" (state persisted in thread)
```

Thread = primary key for checkpoint storage. Without `thread_id`, no persistence.

### 20.5 Long-Term Memory (Store)

The `Store` is a separate system from checkpointing, designed for cross-thread memory.

#### API

```python
from langgraph.store.memory import InMemoryStore

store = InMemoryStore(
    index={
        "dims": 1536,              # embedding dimensions
        "embed": embeddings_model,  # for semantic search
        "fields": ["text"]          # fields to index
    }
)

# Store a memory (namespaced)
store.put(
    namespace=("users", "alice"),
    key="preference-1",
    value={"text": "Prefers dark mode", "importance": 0.8}
)

# Retrieve by key
item = store.get(namespace=("users", "alice"), key="preference-1")

# Semantic search across namespace
results = store.search(
    namespace=("users", "alice"),
    query="UI preferences",
    limit=5
)
```

#### Namespace Model

Namespaces are **tuple-based hierarchies**:
- `("users", "alice")` -- user-specific
- `("teams", "engineering")` -- team-level
- `("global",)` -- application-wide

#### Accessing Store Inside Graph Nodes

```python
from langgraph.graph import StateGraph
from langchain_core.runnables import RunnableConfig

def my_node(state, config: RunnableConfig, *, store):
    # 'store' is injected when graph is compiled with store=
    user_id = config["configurable"]["user_id"]
    memories = store.search(namespace=("users", user_id), query=state["messages"][-1])
    # ... use memories in response
    store.put(namespace=("users", user_id), key="new-fact", value={"text": "..."})
```

#### Memory Types (Cognitive Framework)

LangGraph documents three memory types from the CoALA paper:

| Type | What | Human Analogy | Agent Example |
|------|------|---------------|---------------|
| **Semantic** | Facts/concepts | School knowledge | User preferences, facts |
| **Episodic** | Experiences | Personal events | Past agent actions |
| **Procedural** | Instructions | Motor skills | System prompts, rules |

Semantic memory has two sub-patterns:

- **Profile**: Single continuously-updated JSON document per entity. Risk: overwrites.
- **Collection**: Growing set of individual memory documents. Better recall, harder to manage.

### 20.6 Short-Term Memory Management

Three explicit strategies for managing conversation history:

1. **Trim messages**: Keep only N most recent messages
2. **Delete messages**: Remove specific messages by ID (via `RemoveMessage`)
3. **Summarize messages**: LLM-generated summary replaces old messages

```python
# Summarization pattern
def summarize_conversation(state):
    summary = model.invoke(f"Summarize: {state['messages']}")
    # Delete old messages, keep summary
    delete_messages = [RemoveMessage(id=m.id) for m in state["messages"][:-2]]
    return {"messages": delete_messages + [summary]}
```

### 20.7 Writing Memories: Hot Path vs Background

| Approach | When | Pros | Cons |
|----------|------|------|------|
| **Hot path** | During agent execution | Real-time, consistent | Adds latency |
| **Background** | Async after response | No latency impact | Eventually consistent |

---

## 21. CrewAI Memory

### 21.1 Unified Memory Architecture (v1.10+)

CrewAI has evolved from a 3-type system to a **single unified `Memory` class** that replaces `ShortTermMemory`, `LongTermMemory`, and `EntityMemory`.

#### Core API

```python
from crewai import Memory

memory = Memory()

# 5 cognitive operations:
memory.remember("We decided to use PostgreSQL.")        # encode
matches = memory.recall("What database?", limit=5)      # recall
facts = memory.extract_memories("Long text...")          # extract
memory.forget(scope="/project/old")                      # forget
tree = memory.tree()                                     # consolidate/explore
```

### 21.2 Five Cognitive Operations

| Operation | Purpose | Details |
|-----------|---------|---------|
| **Encode** | Store memory | LLM analyzes content, assigns importance, detects contradictions |
| **Consolidate** | Organize | Self-organizing hierarchical scope tree |
| **Recall** | Retrieve | Adaptive-depth recall with composite scoring |
| **Extract** | Parse | Identifies atomic facts from unstructured text |
| **Forget** | Remove | By age, scope, or relevance threshold |

### 21.3 Composite Scoring System

Recall uses three weighted factors:

```python
memory = Memory(
    recency_weight=0.4,           # how recent
    semantic_weight=0.4,          # how relevant (embedding similarity)
    importance_weight=0.2,        # LLM-assigned importance score
    recency_half_life_days=14,    # decay rate for recency
)

# Score = recency_weight * recency_score
#       + semantic_weight * similarity_score
#       + importance_weight * importance_score
```

### 21.4 Hierarchical Scopes

Memories are organized in a **filesystem-like tree** that grows organically:

```
/
  /company
    /company/engineering
    /company/product
  /project
    /project/alpha
    /project/beta
  /agent
    /agent/researcher
    /agent/writer
```

#### Scope Inference

When `remember()` is called **without** a scope, the LLM analyzes content against the existing tree and suggests placement. New scopes are created automatically.

```python
memory.remember("We chose PostgreSQL.")
# LLM might place under /project/decisions or /engineering/database

memory.remember("Sprint velocity is 42 points", scope="/team/metrics")
# Explicit placement
```

#### MemoryScope (Subtree Views)

```python
agent_memory = memory.scope("/agent/researcher")

# All ops restricted to /agent/researcher subtree
agent_memory.remember("Found three relevant papers.")
agent_memory.recall("relevant papers")

# Narrow further
project_memory = agent_memory.subscope("project-alpha")
# -> /agent/researcher/project-alpha
```

### 21.5 Integration Patterns

```python
# With Crews -- auto extract/inject
crew = Crew(
    agents=[researcher, writer],
    tasks=[research_task, writing_task],
    memory=True,   # or memory=Memory(...)
)
# After each task: extracts discrete facts, stores them
# Before each task: recalls relevant context, injects into prompt

# With Flows -- built-in methods
class ResearchFlow(Flow):
    @start()
    def gather_data(self):
        self.remember(findings, scope="/research/databases")
        return findings

    @listen(gather_data)
    def write_report(self, findings):
        past = self.recall("database benchmarks")
        # ...
```

### 21.6 Underlying Storage

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Vector embeddings | ChromaDB | Semantic search |
| Persistent storage | SQLite3 | Cross-session persistence |
| Scope tree | In-memory + SQLite | Hierarchical organization |

### 21.7 Extract Pattern (Conversation Compression)

```python
raw = """Meeting notes: We decided to migrate from MySQL to PostgreSQL
next quarter. The budget is $50k. Sarah will lead the migration."""

facts = memory.extract_memories(raw)
# -> ["Migration from MySQL to PostgreSQL planned for next quarter",
#     "Database migration budget is $50k",
#     "Sarah will lead the database migration"]

for fact in facts:
    memory.remember(fact)
```

This is CrewAI's approach to **conversation compression**: decompose unstructured text into atomic factual statements, each stored independently with its own scope, importance, and embedding.

---

## 22. Letta / MemGPT Memory

### 22.1 Memory Architecture (OS-Inspired Hierarchy)

Letta implements a **two-tier memory hierarchy** inspired by operating system memory management:

```
+-------------------------------------------+
|           CONTEXT WINDOW (RAM)            |
|                                           |
|  +-------------------------------------+ |
|  | Memory Blocks (Core Memory)          | |
|  | - persona block                      | |
|  | - human block                        | |
|  | - custom blocks (scratchpad, etc.)   | |
|  +-------------------------------------+ |
|  | Message Buffer (recent messages)     | |
|  +-------------------------------------+ |
+-------------------------------------------+
           |                    |
           v                    v
+------------------+  +------------------+
| RECALL MEMORY    |  | ARCHIVAL MEMORY  |
| (Conversation    |  | (Vector DB /     |
|  History Search) |  |  Knowledge Store)|
| Out-of-context   |  | Out-of-context   |
| Auto-persisted   |  | Agent-curated    |
+------------------+  +------------------+
```

### 22.2 Core Memory (Memory Blocks)

Memory blocks are **structured sections of the context window** that persist across all interactions. They are always visible -- no retrieval needed.

#### Block Data Format

Each block consists of:
- `label` (string) -- unique identifier (e.g., "persona", "human", "scratchpad")
- `description` (string) -- describes purpose (critical for agent behavior)
- `value` (string) -- actual content/data
- `limit` (integer) -- max characters (default 5000)
- `read_only` (boolean) -- whether agent can modify

#### How Blocks Appear to the LLM

```xml
<memory_blocks>
  <persona>
    <description>The persona block: Stores details about your current persona...</description>
    <metadata>
      - chars_current=128
      - chars_limit=5000
    </metadata>
    <value>I am a helpful assistant named Sam. I enjoy helping users solve problems.</value>
  </persona>
  <human>
    <description>The human block: Stores key details about the person you are conversing with...</description>
    <metadata>
      - chars_current=84
      - chars_limit=5000
    </metadata>
    <value>The user's name is Alice. She is a software engineer who prefers concise answers.</value>
  </human>
</memory_blocks>
```

#### Block Operations (Agent Tools)

Agents manage their own memory through built-in tools:
- `memory_insert(block_label, content)` -- append new info (concurrent-safe)
- `memory_replace(block_label, old_str, new_str)` -- targeted edit (mostly safe)
- `memory_rethink(block_label, new_value)` -- full rewrite (last-writer-wins)

#### Shared Blocks

Multiple agents can access the **same physical block**. Update once, visible everywhere:

```python
shared_block = client.blocks.create(
    label="organization",
    description="Shared information between all agents.",
    value="Company policies and procedures..."
)

agent1 = client.agents.create(block_ids=[shared_block.id], ...)
agent2 = client.agents.create(block_ids=[shared_block.id], ...)
# Both see the same block in their context windows
```

### 22.3 Recall Memory (Conversation History)

Full conversation history stored in a database, searchable but NOT in context window.

- **Auto-persisted**: Every message (user, assistant, system, tool calls, tool returns) is saved
- **Searchable**: Via `conversation_search` tool
- **Cross-conversation**: All conversations within an agent are pooled together

When the context window fills, messages are **evicted** (compacted) but remain searchable.

### 22.4 Archival Memory (Knowledge Store)

Semantically searchable vector DB for intentional, long-term storage:

```python
# Agent tool call
archival_memory_insert(
    content="Deckard retired six replicants in the off-world colonies",
    tags=["replicant", "history", "retirement"]
)

# Search returns semantically relevant results
results = archival_memory_search(
    query="replicant lifespan",
    tags=["technical"],
    page=0
)
```

Key characteristics:
- **Agent-immutable** (append-only from agent perspective, developers can edit via SDK)
- **Unlimited storage**
- **Semantic search** (meaning-based, not keyword)
- **Tagged organization**

### 22.5 Conversations API (Parallel Threads)

A single agent can have **multiple conversations** running in parallel:

- Each conversation has its **own context window** (messages processed independently)
- All conversations **share memory blocks** (update in one, visible in all)
- All conversations **share searchable message history** (conversation_search spans all)
- Long conversations get **compacted independently**

```python
conversation = client.conversations.create(agent_id="agent-xxx")

# Model override per conversation or per request
conv = client.conversations.create(agent_id="agent-xxx", model="openai/gpt-5-mini")
stream = client.conversations.messages.create(
    conv.id,
    messages=[{"role": "user", "content": "Hello"}],
    override_model="anthropic/claude-haiku-4-5",  # per-request override
)
```

### 22.6 Evolution from MemGPT: Heartbeat Deprecation

The original MemGPT paper used a **heartbeat pattern** where:
- Every action was a tool call (including `send_message` for assistant responses)
- Heartbeats triggered memory management operations
- Memory compression happened inline during conversations

**Letta V1 (`letta_v1_agent`) deprecated this**:
- Assistant messages are now native (no `send_message` tool)
- Heartbeats removed
- Memory management via **sleep-time agents** (async, non-blocking)

### 22.7 Sleep-Time Compute

Instead of inline memory management, Letta now supports **async memory agents**:

- **Non-blocking**: Memory management happens asynchronously
- **Proactive refinement**: Memory blocks reorganized during idle periods
- **Better quality**: Not constrained by real-time response latency
- **Specialized agents**: Dedicated agents for memory curation

### 22.8 Message Eviction & Summarization

When context window reaches capacity:

1. **Evict** ~70% of oldest messages (keep recent for continuity)
2. **Recursive summarization**: Evicted messages summarized alongside existing summaries
3. Older messages have progressively less influence on summary
4. All evicted messages remain searchable via recall memory

### 22.9 Database Backend

Letta stores all state (memory blocks, messages, archival passages) in a persistent database:
- All messages persisted to disk automatically
- Evicted messages still retrievable via API and agent tools
- Server-side persistence (no client-side state management needed)

---

## 23. Mem0

### 23.1 Memory Layer Architecture

Mem0 separates memory into four distinct layers:

```
+-----------------+  +-----------------+  +-----------------+  +-----------------+
| Conversation    |  | Session Memory  |  | User Memory     |  | Org Memory      |
| (single turn)   |  | (minutes-hours) |  | (weeks-forever) |  | (global config) |
+-----------------+  +-----------------+  +-----------------+  +-----------------+
      |                    |                    |                    |
      +--------------------+--------------------+--------------------+
                           |
                    Mem0 Retrieval Layer
                    (merges all layers on query)
```

| Layer | Lifetime | Best For | Trade-offs |
|-------|----------|----------|------------|
| Conversation | Single response | Tool execution detail | Lost after turn |
| Session | Minutes to hours | Multi-step flows | Manual clear |
| User | Weeks to forever | Personalization | Needs consent |
| Org | Global config | Shared knowledge | Needs curation |

### 23.2 Core Data Format

Memories are stored as **atomic facts** extracted from conversations:

```python
from mem0 import Memory

memory = Memory(api_key=os.environ["MEM0_API_KEY"])

# Add memory with scope identifiers
memory.add(
    ["I'm Alex and I prefer boutique hotels."],
    user_id="alex",
    session_id="trip-planning-2025",
)

# Search merges all layers, user memory ranked first
results = memory.search(
    "Any hotel preferences?",
    user_id="alex",
    session_id="trip-planning-2025",
)
```

### 23.3 Memory Lifecycle (Capture-Promote-Retrieve)

1. **Capture**: Messages enter conversation layer during active turn
2. **Promote**: Relevant details persist to session or user memory based on `user_id`, `session_id`, and metadata
3. **Retrieve**: Search pipeline pulls from all layers, ranking: user > session > history

### 23.4 Storage Backend

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Vector store | Qdrant, Chroma (OSS) | Semantic similarity |
| Graph store | Neo4j (Pro tier) | Entity relationships |
| Reranker | Cross-encoder models | Result quality |

### 23.5 Memory Operations

```python
# Add
memory.add(messages, user_id="user-1", session_id="session-1")

# Search (semantic + metadata filtering)
results = memory.search(query, user_id="user-1")

# Update (specific memory by ID)
memory.update(memory_id, data="Updated content")

# Delete
memory.delete(memory_id)
memory.delete_all(user_id="user-1")  # bulk delete
```

### 23.6 Graph Memory (Pro Tier)

Builds entity relationships alongside vector embeddings:
- Entities extracted from conversations
- Relationships tracked (subject-predicate-object triples)
- Traversal queries alongside semantic search
- Handles temporal reasoning

### 23.7 Key Design Choice: Atomic Fact Extraction

Unlike systems that store raw messages, Mem0 **extracts atomic facts**:

```
Input: "I'm Alex, I work at Google, and I prefer boutique hotels"
->
Memory 1: "User's name is Alex"
Memory 2: "User works at Google"
Memory 3: "User prefers boutique hotels"
```

Each fact gets its own embedding, metadata, and version history. This enables:
- Surgical updates (change one fact without touching others)
- Better retrieval precision
- Contradiction detection (new fact vs existing)

---

## 24. Cognee

### 24.1 Extract-Cognify-Load Pipeline

Cognee processes data through three phases:

1. **Extract**: Pull from 30+ data sources (files, APIs, databases)
2. **Cognify**: Transform into structured knowledge:
   - Chunking
   - Embedding generation
   - Graph-based extraction (subject-relation-object triples)
   - Contradiction resolution
3. **Load**: Store in hybrid graph + vector format

### 24.2 Hybrid Architecture: Knowledge Graph + Vector Search

```
+-------------------+     +-------------------+
| Knowledge Graph   |     | Vector Store      |
| (Entities +       |     | (Embeddings +     |
|  Relationships)   |     |  Semantic Search)  |
+-------------------+     +-------------------+
         |                         |
         +------------+------------+
                      |
              Graph-Aware Retrieval
              (multi-hop + semantic)
```

| Component | Backend Options | Purpose |
|-----------|----------------|---------|
| Knowledge Graph | Kuzu, Memgraph | Structured relationships |
| Vector Store | LanceDB, Qdrant | Semantic similarity |
| Persistent DB | PostgreSQL + pgvector | Primary storage |

### 24.3 Memify: Post-Deployment Memory Optimization

**Key innovation**: Memify treats post-processing as a first-class pipeline:

- Clean stale nodes and strengthen associations
- Adjust structure without full rebuilds
- **Feedback loops**: Rated responses aggregate into edge weights
- Reweight important facts for better retrieval

Memory improves over time through real usage, not just initial ingestion.

### 24.4 Associative Memory (2025-2026)

Recent addition: **Associative MCP Memory**:
- Dynamic associations between concepts in real-time
- Contextual learning adapts based on usage patterns
- Cross-domain linking connects info across domains

### 24.5 Key Differentiator

Cognee goes beyond traditional RAG by providing:
- **ACID-style guarantees** for memory operations
- **Contradiction resolution** when new facts conflict with existing
- **Multi-tenancy** with per-user isolation
- **RBAC, API keys, audit logs** for enterprise

---

## 25. Other Cutting-Edge Frameworks

### 25.1 A-MEM (Zettelkasten-Style Graph Memory)

Academic framework that replaces flat memory stores with **Zettelkasten-style graph networks**:
- Typed relationships between memory nodes
- 26% improvement over OpenAI baselines on LLM-as-Judge metrics
- Spreading activation enables associative recall
- Graph outperforms vectors in multi-hop/temporal reasoning

### 25.2 ACC (Adaptive Cognitive Compression)

Mimics brain consolidation:
- Compresses short-term memories into long-term during "sleep-like" processes
- Improves efficiency and stability
- Prevents catastrophic forgetting

### 25.3 Titans Architecture

Learned long-term memory module:
- Modules update at different speeds (fast context, medium consolidation, slow core)
- Prioritizes surprising information
- Enables multi-million token contexts via test-time memorization

### 25.4 Hindsight Framework

Agent-managed memory system:
- Agents self-edit memory blocks
- Agent-driven promotion decisions between working context and archives
- Inspired by OS memory hierarchies

### 25.5 Constitutional Memory Architecture (CMA)

From arXiv:2603.04740v1 (March 2026):
- **Memory-as-Ontology** paradigm: memory as foundation of digital agent existence
- Four-layer governance hierarchy
- Multi-layer semantic storage
- **Digital Citizen Lifecycle** for persistent identity across model changes
- Outperforms Mem0, Letta, Zep in long-term scenarios

### 25.6 Zep (Separate from Mem0)

Dedicated memory framework:
- Persistent storage + retrieval + summarization
- Long-term context with hybrid systems
- Task-oriented but lacks governance layer

### 25.7 OpenAI Agents SDK

Current state management:
- Basic context replay (RAG-style injection)
- Stateful ReAct loops with memory across steps
- Graph/workflow orchestration
- Limited compared to dedicated memory frameworks

### 25.8 Microsoft AutoGen

Memory integration:
- Short/long-term for multi-agent learning
- Event-driven with memory from prior runs
- Azure-integrated persistence
- Learning from interactions across sessions

---

## 26. Academic Papers (2025-2026)

### Key Papers

| Paper | Source | Key Contribution |
|-------|--------|-----------------|
| **Memory in the Age of AI Agents** | arXiv:2512.13564 | Comprehensive survey: taxonomies of memory forms (token-level, parametric, latent), functions (factual, experiential, working), dynamics (formation, evolution, retrieval) |
| **Constitutional Memory Architecture** | arXiv:2603.04740v1 | Memory-as-Ontology paradigm, 4-layer governance, Digital Citizen Lifecycle, outperforms Mem0/Letta/Zep |
| **A-MEM** | 2025 | Zettelkasten graph memory, 26% improvement over baselines, spreading activation |
| **ACC (Adaptive Cognitive Compression)** | 2025 | Brain-like consolidation, sleep-phase compression |
| **Titans Architecture** | 2025 | Multi-speed memory modules, test-time memorization |
| **CoALA** | arXiv:2309.02427 | Foundational: maps human memory types to AI agent memory |

### Research Trends

1. **Graph > Vector**: Knowledge graphs outperform pure vector stores for multi-hop reasoning
2. **Agent-managed memory**: Agents deciding what/when to remember (not just passive storage)
3. **Biologically-inspired**: Consolidation, forgetting, separate systems for different memory types
4. **Governance layers**: Identity continuity and trust, not just retrieval
5. **Async memory management**: Sleep-time compute, background memory agents

---

## 27. Cross-Framework Memory Comparison

### 27.1 Architecture Patterns

```
                    In-Context        Out-of-Context       Cross-Thread
                    (Always visible)  (Retrieved on-demand) (Shared state)
                    +-----------+     +-----------+         +-----------+
LangGraph           | State     |     | Store     |         | Store     |
                    | (channels)|     | (namespace)|        | (namespace)|
                    +-----------+     +-----------+         +-----------+

CrewAI              | (scope    |     | ChromaDB  |         | Shared    |
                    |  tree)    |     | + SQLite  |         | scopes    |
                    +-----------+     +-----------+         +-----------+

Letta               | Memory    |     | Archival  |         | Shared    |
                    | Blocks    |     | + Recall  |         | Blocks    |
                    +-----------+     +-----------+         +-----------+

Mem0                | Conv      |     | Session + |         | Org       |
                    | history   |     | User      |         | memory    |
                    +-----------+     +-----------+         +-----------+

Cognee              | (none -   |     | Graph +   |         | Multi-    |
                    |  stateless)|    | Vector    |         | tenant    |
                    +-----------+     +-----------+         +-----------+
```

### 27.2 Data Format Comparison

| Framework | Short-Term Format | Long-Term Format | Compression |
|-----------|-------------------|------------------|-------------|
| **LangGraph** | MsgPack checkpoints (channel_values/versions) | JSON docs in Store (namespaced) | Trim/delete/summarize messages |
| **CrewAI** | Scope tree + ChromaDB vectors | SQLite records + embeddings | LLM extract_memories() to atomic facts |
| **Letta** | XML memory blocks in prompt + message buffer | Vector passages (archival) + full message history (recall) | Recursive summarization on eviction (~70%) |
| **Mem0** | Raw conversation messages | Atomic facts with embeddings + optional graph triples | Fact extraction from unstructured text |
| **Cognee** | N/A (stateless) | Knowledge graph + vector embeddings | ECL pipeline with contradiction resolution |

### 27.3 Persistence Strategies

| Framework | Thread/Session | Cross-Session | Cross-Agent |
|-----------|---------------|---------------|-------------|
| **LangGraph** | Checkpoint linked list per thread_id | Store with custom namespaces | Store with shared namespace |
| **CrewAI** | Scope subtree per agent/flow | SQLite persisted scopes | Shared scope paths |
| **Letta** | Conversation with own context window | Agent-level memory blocks + archival | Shared blocks (block_ids) |
| **Mem0** | session_id scoping | user_id scoping | org_id scoping |
| **Cognee** | N/A | PostgreSQL + pgvector | Multi-tenancy with RBAC |

### 27.4 Promotion Patterns

| Framework | How Data Moves Between Tiers |
|-----------|------------------------------|
| **LangGraph** | Manual: developer writes to Store in node logic. Background: async memory writing after response. |
| **CrewAI** | Automatic: after each task, crew extracts facts and stores them. Manual: agent calls remember(). Forget: time/scope-based. |
| **Letta** | Agent-driven: agent uses memory tools to self-edit blocks. Sleep-time: async agents curate memory during idle. Eviction: auto-summarize when context full. |
| **Mem0** | Automatic: fact extraction on add(). Scope-based: user_id/session_id determines layer. Search merges all layers. |
| **Cognee** | Pipeline: ECL extracts, cognifies, loads. Memify: post-deployment optimization with feedback loops. |

### 27.5 Compression Strategy Comparison

| Strategy | Used By | Mechanism | Trade-offs |
|----------|---------|-----------|------------|
| **Message trimming** | LangGraph | Keep N most recent messages | Lossy, simple |
| **Message summarization** | LangGraph, Letta | LLM generates summary of old messages | Quality depends on LLM |
| **Recursive summarization** | Letta | Summary of summary (cascading) | Progressive info loss |
| **Atomic fact extraction** | CrewAI, Mem0 | LLM decomposes text into discrete facts | Best precision, more storage |
| **Graph extraction** | Cognee, Mem0 Pro | Subject-predicate-object triples | Rich relationships, complex |
| **Block rewriting** | Letta | Agent rewrites entire memory block | Agent-managed quality |
| **Scope-based forgetting** | CrewAI | Drop memories in old scopes | Coarse-grained |
| **Eviction with archival** | Letta | Move from context to searchable DB | Never truly lost |

### 27.6 Database Backend Comparison

| Framework | Vector DB | Graph DB | Relational DB | Cache |
|-----------|-----------|----------|---------------|-------|
| **LangGraph** | Via Store (custom) | None native | Postgres, SQLite, MongoDB | Redis |
| **CrewAI** | ChromaDB | None | SQLite | None |
| **Letta** | Built-in (archival) | None | Server-managed | None |
| **Mem0** | Qdrant, Chroma | Neo4j (Pro) | None | None |
| **Cognee** | LanceDB, Qdrant | Kuzu, Memgraph | PostgreSQL + pgvector | Pre-computed subgraphs |

---

## 28. Rust Implementation Patterns

> **Scope**: Crates, patterns, and architectural decisions for building an append-only agent memory system in Nika
> **Key finding**: The existing Nika codebase already has strong foundations in `event::TraceWriter` (NDJSON) and `io::atomic` (safe writes) that can be extended rather than replaced.

---

### 28.1 NDJSON Handling in Rust

#### Recommendation: serde_json line-by-line (no additional crate needed)

Nika already implements the optimal pattern in `tools/nika/src/event/trace.rs`. The ecosystem does not have a dominant NDJSON crate -- the existing ones are marginal:

| Crate | Version | Downloads | Notes |
|-------|---------|-----------|-------|
| `ndjson` | 0.2.0 | 2,887 | CLI formatter/colorizer only, not a library |
| `ndjson-stream` | 0.1.0 | 2,142 | Streaming reader, single release, stale |

**The standard Rust approach is to use `serde_json` directly**, which is what Nika already does.

#### Pattern: Synchronous Write (Current Nika)

```rust
// From tools/nika/src/event/trace.rs (lines 67-75)
pub fn write_event(&self, event: &Event) -> Result<()> {
    let json = serde_json::to_string(event)?;
    let mut writer = self.writer.lock();
    writeln!(writer, "{}", json)?;
    writer.flush()?;
    Ok(())
}
```

**Key properties**:
- `BufWriter<File>` for buffered I/O
- `parking_lot::Mutex` for lock performance (2-3x faster than std)
- `flush()` after each event for durability
- `writeln!` ensures newline delimiter

#### Pattern: Async NDJSON Writer (for new memory system)

```rust
use tokio::fs::OpenOptions;
use tokio::io::{AsyncWriteExt, BufWriter};

pub struct AsyncNdjsonWriter {
    writer: tokio::sync::Mutex<BufWriter<tokio::fs::File>>,
    path: PathBuf,
}

impl AsyncNdjsonWriter {
    pub async fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)  // O_APPEND for kernel-level atomicity
            .open(path)
            .await?;
        Ok(Self {
            writer: tokio::sync::Mutex::new(BufWriter::new(file)),
            path: path.to_path_buf(),
        })
    }

    pub async fn append<T: Serialize>(&self, record: &T) -> io::Result<()> {
        let mut json = serde_json::to_vec(record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        json.push(b'\n');

        let mut writer = self.writer.lock().await;
        writer.write_all(&json).await?;
        writer.flush().await?;
        Ok(())
    }
}
```

#### Pattern: NDJSON Reader (async streaming)

```rust
use tokio::io::{AsyncBufReadExt, BufReader};

pub async fn read_ndjson<T: DeserializeOwned>(
    path: &Path,
) -> io::Result<Vec<T>> {
    let file = tokio::fs::File::open(path).await?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut records = Vec::new();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() { continue; }
        match serde_json::from_str(&line) {
            Ok(record) => records.push(record),
            Err(e) => {
                tracing::warn!(
                    line = %line.chars().take(80).collect::<String>(),
                    error = %e,
                    "Skipping malformed NDJSON line"
                );
            }
        }
    }
    Ok(records)
}
```

#### Pattern: Streaming Iterator (memory-efficient, sync)

```rust
pub fn iter_ndjson<T: DeserializeOwned>(
    reader: impl BufRead,
) -> impl Iterator<Item = Result<T, serde_json::Error>> {
    reader.lines()
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(&line))
}
```

---

### 28.2 Append-Only Log Patterns in Rust

#### Existing Crates

| Crate | Version | Downloads | Architecture |
|-------|---------|-----------|-------------|
| `aol` | 0.3.2 | 11,951 | Append-only log with segments, CRC32 checksums |
| `waly` | 0.1.4 | 493 | Simple WAL with `Arc<Mutex<File>>`, JSON entries |
| `walrus-rust` | 0.2.0 | 816 | WAL with page-based storage |

**Recommendation**: None of these are production-ready enough. Build on Nika's existing `io::atomic` module.

#### Nika's Existing Foundation

Nika already has the building blocks in `tools/nika/src/io/atomic.rs`:

```rust
// Atomic write: temp file -> flush -> sync -> rename
pub async fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()>

// Append to file
pub async fn write_append(path: &Path, content: &[u8]) -> io::Result<()>

// Unique filename generation
pub async fn write_unique(path: &Path, content: &[u8]) -> io::Result<PathBuf>

// Fail-if-exists (atomic check)
pub async fn write_fail(path: &Path, content: &[u8]) -> io::Result<()>
```

#### Pattern: Append-Only NDJSON Log with Corruption Prevention

```rust
use std::io::Write;

pub struct AppendLog {
    file: std::fs::File,
    path: PathBuf,
    /// Byte offset of last successful write (for recovery)
    last_good_offset: u64,
}

impl AppendLog {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        let last_good_offset = file.metadata()?.len();

        Ok(Self {
            file,
            path: path.to_path_buf(),
            last_good_offset,
        })
    }

    pub fn append<T: Serialize>(&mut self, record: &T) -> io::Result<()> {
        let mut json = serde_json::to_vec(record)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        json.push(b'\n');

        // Write + fsync for durability
        self.file.write_all(&json)?;
        self.file.sync_data()?;  // fdatasync - faster than sync_all

        self.last_good_offset += json.len() as u64;
        Ok(())
    }

    /// Truncate any partial write after crash recovery
    pub fn recover(&mut self) -> io::Result<()> {
        let actual_len = self.file.metadata()?.len();
        if actual_len > self.last_good_offset {
            self.file.set_len(self.last_good_offset)?;
            tracing::warn!(
                truncated_bytes = actual_len - self.last_good_offset,
                "Recovered from partial write"
            );
        }
        Ok(())
    }
}
```

#### fsync Strategy (Performance vs. Durability)

| Strategy | Method | Durability | Performance |
|----------|--------|-----------|-------------|
| Per-record | `sync_data()` after each write | Highest | ~200-500 records/sec |
| Batched | `sync_data()` every N records or T ms | Good | ~5,000-10,000 records/sec |
| OS-managed | No explicit sync, rely on OS | Lowest | ~50,000+ records/sec |

**Recommendation for agent memory**: Batched sync (every 100ms or 10 records) is the sweet spot. Agent turns are infrequent enough that per-record sync is also acceptable.

#### Pattern: CRC32 Integrity Checking (Optional)

```rust
use crc32fast::Hasher;

#[derive(Serialize, Deserialize)]
pub struct ChecksummedRecord<T> {
    pub data: T,
    pub crc32: u32,
}

impl<T: Serialize> ChecksummedRecord<T> {
    pub fn new(data: T) -> Result<Self, serde_json::Error> {
        let json = serde_json::to_vec(&data)?;
        let mut hasher = Hasher::new();
        hasher.update(&json);
        Ok(Self { data, crc32: hasher.finalize() })
    }
}
```

---

### 28.3 rig-core Conversation History and Agent Memory

#### Current Version: rig-core v0.32.0

**Repository**: https://github.com/0xPlaygrounds/rig (under `rig/rig-core/`)
**Cargo.toml**: `rig-core = "0.32.0"` (published 2026-03-05)
**Key deps**: serde, tokio, reqwest, schemars, rmcp 0.16 (optional)

#### Key Architecture Finding

rig-core has **no built-in persistence for conversation history**. History is passed as `Vec<Message>` at call sites. This is by design -- rig-core is stateless, and the caller (Nika) owns the history.

#### Message Types (`rig/rig-core/src/completion/message.rs`)

```rust
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User { content: OneOrMany<UserContent> },
    Assistant {
        id: Option<String>,
        content: OneOrMany<AssistantContent>,
    },
}

// User can send text, tool results, images, audio, video, documents
pub enum UserContent {
    Text(Text),
    ToolResult(ToolResult),
    Image(Image),
    Audio(Audio),
    Video(Video),
    Document(Document),
}

// Assistant responds with text, tool calls, reasoning, or images
pub enum AssistantContent {
    Text(Text),
    ToolCall(ToolCall),
    Reasoning(Reasoning),  // Extended thinking support
    Image(Image),
}
```

#### Chat Interface

```rust
// High-level chat with history
pub trait Chat {
    async fn chat(
        &self,
        prompt: impl Into<Message>,
        chat_history: Vec<Message>,  // Caller owns history
    ) -> Result<String, PromptError>;
}

// Low-level completion with full control
pub trait Completion<M: CompletionModel> {
    async fn completion(
        &self,
        prompt: impl Into<Message>,
        chat_history: Vec<Message>,
    ) -> Result<CompletionRequestBuilder<M>, CompletionError>;
}
```

#### Agent Loop (Multi-Turn Tool Calling)

The agent loop in `rig/rig-core/src/agent/prompt_request/mod.rs` implements:

1. **Prompt** -- Build `CompletionRequest` with history
2. **Call model** -- Get response (text or tool calls)
3. **If tool calls**: Execute tools, append results to history, loop back
4. **If text**: Return response
5. **Max turns** protection via `max_turns` setting

Key code from the source:

```rust
// From PromptRequest::send() (prompt_request/mod.rs)
let chat_history = if let Some(history) = self.chat_history.as_mut() {
    history.push(self.prompt.to_owned());
    history
} else {
    &mut vec![self.prompt.to_owned()]
};

// ... agent loop runs ...
// On max turns exceeded:
Err(PromptError::MaxTurnsError {
    max_turns,
    chat_history: Box::new(chat_history),
    prompt: Box::new(prompt),
})
```

The loop appends assistant responses (including tool calls) and tool results to history, then sends the full history back to the model on each turn.

#### PromptHook System (for observability)

rig-core v0.32.0 provides `PromptHook` trait for intercepting the agent loop (`agent/prompt_request/hooks.rs`):

```rust
pub trait PromptHook<M: CompletionModel>: Clone + Send + Sync {
    async fn on_completion_call(
        &self, prompt: &Message, history: &[Message]
    ) -> HookAction;

    async fn on_completion_response(
        &self, prompt: &Message, response: &CompletionResponse<M::Response>
    ) -> HookAction;

    async fn on_tool_call(
        &self, tool_name: &str, call_id: Option<String>,
        internal_id: &str, args: &str
    ) -> ToolCallHookAction;

    async fn on_tool_result(
        &self, tool_name: &str, call_id: Option<String>,
        internal_id: &str, args: &str, result: &str
    ) -> HookAction;

    async fn on_text_delta(
        &self, text_delta: &str, aggregated_text: &str
    ) -> HookAction;

    async fn on_tool_call_delta(
        &self, tool_call_id: &str, internal_call_id: &str,
        tool_name: Option<&str>, tool_call_delta: &str
    ) -> HookAction;

    async fn on_stream_completion_response_finish(
        &self, prompt: &Message, response: &M::StreamingResponse
    ) -> HookAction;
}

pub enum HookAction {
    Continue,
    Terminate { reason: String },
}

pub enum ToolCallHookAction {
    Continue,
    Skip { reason: String },
    Terminate { reason: String },
}
```

**This is the integration point for Nika's memory system** -- a `PromptHook` implementation can intercept every turn, tool call, and response to persist to the append-only log.

#### Reasoning Capture

rig-core supports extended thinking via `Reasoning` and `ReasoningContent`:

```rust
pub struct Reasoning {
    pub id: Option<String>,
    pub content: Vec<ReasoningContent>,
}

pub enum ReasoningContent {
    Text { text: String, signature: Option<String> },
    Encrypted(String),
    Redacted { data: String },
    Summary(String),
}
```

#### Extended Response Details

```rust
// Use .extended_details() for token usage and full message history
let response = agent.prompt("Hello")
    .extended_details()
    .await?;

// response.output: String
// response.usage: Usage { input_tokens, output_tokens }
// response.messages: Option<Vec<Message>>  -- full history from the loop
```

#### Dynamic Context (RAG)

```rust
pub struct Agent<M, P> {
    // ...
    pub static_context: Vec<Document>,
    pub dynamic_context: DynamicContextStore,  // Vector store indexes
    // ...
}

type DynamicContextStore = Arc<
    TokioRwLock<Vec<(usize, Box<dyn VectorStoreIndexDyn + Send + Sync>)>>
>;
```

#### Implications for Nika Memory System

1. **History is ephemeral**: rig-core does not persist history -- Nika must own this
2. **PromptHook is the observability API**: Use it to capture turns for the memory log
3. **Message is serde-compatible**: `Message` derives `Serialize`/`Deserialize` -- can be stored directly in NDJSON
4. **Reasoning is captured**: Extended thinking blocks can be persisted alongside responses
5. **Token usage is available**: Via `PromptResponse` with `extended_details()`
6. **MaxTurnsError returns history**: Even on failure, the full chat history is returned for recovery

---

### 28.4 Token Counting in Rust

#### Crate Comparison

| Crate | Version | Downloads | Encodings | Pure Rust? | Maintained? |
|-------|---------|-----------|-----------|-----------|-------------|
| **tiktoken-rs** | 0.9.1 | 4,919,158 | o200k_harmony, o200k, cl100k, p50k, r50k | Yes | Active (Nov 2025) |
| **tokenizers** | 0.22.2 | 12,012,257 | HuggingFace tokenizers (BPE, WordPiece, etc.) | Yes | Active (HuggingFace) |
| **tiktoken** | 3.1.2 | 5,048 | Same as tiktoken-rs | Yes | Active (Mar 2026) |

#### Recommendation: tiktoken-rs v0.9.1

**By far the most popular** with 4.9M downloads. Supports all OpenAI encoding schemes including the latest `o200k_harmony` for GPT-5/o4 models.

```toml
[dependencies]
tiktoken-rs = "0.9"
```

#### Usage: Basic Token Counting

```rust
use tiktoken_rs::o200k_base;

fn count_tokens(text: &str) -> usize {
    let bpe = o200k_base().unwrap();
    bpe.encode_with_special_tokens(text).len()
}
```

#### Usage: Model-Aware Token Counting

```rust
use tiktoken_rs::{get_chat_completion_max_tokens, ChatCompletionRequestMessage};

let messages = vec![
    ChatCompletionRequestMessage {
        content: Some("You are a helpful assistant.".to_string()),
        role: "system".to_string(),
        name: None,
        function_call: None,
    },
    ChatCompletionRequestMessage {
        content: Some("Hello!".to_string()),
        role: "user".to_string(),
        name: None,
        function_call: None,
    },
];
let max_tokens = get_chat_completion_max_tokens("o1-mini", &messages).unwrap();
```

#### Supported Encodings

| Encoding | Models | Tokens/Char (approx) |
|----------|--------|--------------------|
| `o200k_harmony` | GPT-5, gpt-oss-20b/120b | ~0.25 |
| `o200k_base` | GPT-4.1, GPT-4o, o4, o3, o1 | ~0.25 |
| `cl100k_base` | ChatGPT, text-embedding-ada-002 | ~0.25 |
| `p50k_base` | Code models, text-davinci-002/003 | ~0.30 |

#### Alternative: HuggingFace tokenizers v0.22.2

Use this if you need to support non-OpenAI models (Llama, Mistral local models):

```toml
[dependencies]
tokenizers = { version = "0.22", features = ["onig"] }
```

```rust
use tokenizers::Tokenizer;

let tokenizer = Tokenizer::from_pretrained("mistralai/Mistral-7B-v0.1", None)?;
let encoding = tokenizer.encode("Hello world", false)?;
println!("Tokens: {}", encoding.get_ids().len());
```

#### Fast Estimation (No Encoding)

For approximate token counts without loading a full tokenizer:

```rust
/// Approximate token count using character-based heuristic.
/// Accurate to ~10-15% for English text.
pub fn estimate_tokens(text: &str) -> usize {
    // Average ratio: ~4 characters per token for English
    (text.len() as f64 / 4.0).ceil() as usize
}

/// More accurate estimation using word + punctuation counting.
pub fn estimate_tokens_better(text: &str) -> usize {
    let words = text.split_whitespace().count();
    let punct = text.chars().filter(|c| c.is_ascii_punctuation()).count();
    // ~1.3 tokens per word on average, punctuation typically 1 token each
    ((words as f64 * 1.3) + punct as f64).ceil() as usize
}
```

#### Integration Pattern for Memory System

```rust
use tiktoken_rs::o200k_base;
use std::sync::OnceLock;

static BPE: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();

fn get_tokenizer() -> &'static tiktoken_rs::CoreBPE {
    BPE.get_or_init(|| o200k_base().expect("Failed to load tokenizer"))
}

/// Count tokens across a list of rig-core Messages
pub fn count_message_tokens(messages: &[rig::completion::Message]) -> usize {
    let tokenizer = get_tokenizer();
    messages.iter().map(|msg| {
        let text = serde_json::to_string(msg).unwrap_or_default();
        tokenizer.encode_with_special_tokens(&text).len() + 4  // +4 for message overhead
    }).sum()
}
```

---

### 28.5 LLM Compression Patterns (Summarization via rig-core)

#### Pattern: Summarization Agent

Using rig-core's Agent to compress/summarize conversation history:

```rust
use rig::completion::Prompt;

pub struct MemoryCompressor<M: CompletionModel> {
    agent: rig::agent::Agent<M>,
}

impl<M: CompletionModel> MemoryCompressor<M> {
    pub fn new(model: M) -> Self {
        let agent = model
            .agent("claude-sonnet-4-20250514")
            .preamble(
                "You are a conversation memory compressor. \
                 Given a conversation history, produce a concise summary that \
                 preserves all key facts, decisions, tool results, and context. \
                 Output only the summary, no preamble."
            )
            .temperature(0.0)
            .build();
        Self { agent }
    }

    pub async fn compress(
        &self,
        messages: &[Message],
        token_budget: usize,
    ) -> Result<String, PromptError> {
        let history_json = serde_json::to_string_pretty(messages)
            .unwrap_or_default();
        let prompt = format!(
            "Compress this conversation to fit within ~{} tokens. \
             Preserve: tool call results, decisions, key facts, user preferences.\n\n\
             Conversation:\n{}",
            token_budget, history_json
        );
        self.agent.prompt(&prompt).await
    }
}
```

#### Pattern: Hierarchical Compression (Rolling Summary)

```rust
pub struct RollingMemory<M: CompletionModel> {
    /// Compressed summary of older messages
    summary: String,
    /// Recent messages kept in full
    recent: Vec<Message>,
    /// Max tokens for recent window
    recent_token_budget: usize,
    /// Compressor agent
    compressor: MemoryCompressor<M>,
}

impl<M: CompletionModel> RollingMemory<M> {
    pub async fn add_turn(
        &mut self,
        user: Message,
        assistant: Message,
    ) -> Result<(), PromptError> {
        self.recent.push(user);
        self.recent.push(assistant);

        let current_tokens = count_message_tokens(&self.recent);
        if current_tokens > self.recent_token_budget {
            // Move oldest messages to summary
            let to_compress: Vec<_> =
                self.recent.drain(..self.recent.len() / 2).collect();
            let additional_summary = self.compressor
                .compress(&to_compress, self.recent_token_budget / 4)
                .await?;
            self.summary = format!(
                "{}\n\n{}",
                self.summary, additional_summary
            );
        }
        Ok(())
    }

    pub fn build_context(&self) -> Vec<Message> {
        let mut context = vec![];
        if !self.summary.is_empty() {
            context.push(Message::User {
                content: OneOrMany::one(UserContent::Text(Text {
                    text: format!(
                        "[Previous conversation summary]\n{}",
                        self.summary
                    ),
                })),
            });
        }
        context.extend(self.recent.clone());
        context
    }
}
```

#### Pattern: Structured Extraction (Memory Records)

```rust
use schemars::JsonSchema;

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct MemoryExtraction {
    /// Key facts learned during conversation
    pub facts: Vec<String>,
    /// Decisions made
    pub decisions: Vec<String>,
    /// User preferences observed
    pub preferences: Vec<String>,
    /// Tool results worth preserving
    pub tool_results: Vec<ToolResultSummary>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ToolResultSummary {
    pub tool: String,
    pub key_data: String,
}
```

This could be extracted using rig-core's typed prompt (structured output):

```rust
let extraction: MemoryExtraction = agent
    .prompt(&format!("Extract key memories from:\n{}", history_json))
    .prompt_typed()
    .await?;
```

---

### 28.6 Serde Patterns for Evolving Schemas

#### The Problem

When storing NDJSON records over time, the schema will evolve (new fields, renamed fields, removed fields). Older records must remain readable.

#### Crate Comparison

| Crate | Version | Downloads | Approach |
|-------|---------|-----------|----------|
| **magic_migrate** | 2.0.0 | 8,528 | Chain of `TryFrom` conversions, derive macro |
| **serde_flow** | 1.1.1 | 59,818 | Binary versioning with migration functions |
| **pro-serde-versioned** | 1.0.2 | 8,197 | Version byte prepended to serialized data |
| **serde-evolve** | 0.1.0 | 1,293 | Compile-time verified migrations |

#### Recommendation: Built-in serde patterns (no additional crate)

For NDJSON specifically, the built-in serde attributes handle 90% of schema evolution needs without external dependencies.

#### Pattern 1: Forward-Compatible Records (Additive Changes)

This is the simplest and most robust approach for NDJSON:

```rust
#[derive(Serialize, Deserialize)]
pub struct MemoryRecord {
    /// Schema version (always written, defaults to 1 for old records)
    #[serde(default = "default_v1")]
    pub v: u32,

    /// Record type tag
    #[serde(rename = "type")]
    pub record_type: String,

    /// ISO 8601 timestamp
    pub ts: String,

    /// Added in v2 -- Option + default handles old records
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_count: Option<u32>,

    /// Added in v3
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,

    /// Catch-all for unknown future fields (forward compatibility)
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn default_v1() -> u32 { 1 }
```

**Key serde attributes for evolution**:

| Attribute | Purpose | Example |
|-----------|---------|---------|
| `#[serde(default)]` | Missing field gets Default | New optional fields |
| `#[serde(default = "fn")]` | Custom default function | Version field |
| `#[serde(flatten)]` | Catch unknown fields in a map | Future fields |
| `#[serde(skip_serializing_if = "...")]` | Omit None/empty fields | Clean output |
| `#[serde(alias = "old_name")]` | Accept renamed fields | Migrations |
| `#[serde(deny_unknown_fields)]` | Strict mode (use sparingly) | Validation |

#### Pattern 2: Tagged Version Dispatch

For breaking changes that cannot be handled with defaults:

```rust
use serde_json::Value;

#[derive(Serialize, Deserialize)]
#[serde(tag = "v")]
pub enum VersionedRecord {
    #[serde(rename = "1")]
    V1(RecordV1),
    #[serde(rename = "2")]
    V2(RecordV2),
    #[serde(rename = "3")]
    V3(RecordV3),
}

#[derive(Serialize, Deserialize)]
pub struct RecordV1 {
    pub timestamp: String,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
pub struct RecordV2 {
    pub timestamp: String,
    pub content: String,
    pub token_count: u32,
}

#[derive(Serialize, Deserialize)]
pub struct RecordV3 {
    pub timestamp: String,
    pub content: String,
    pub token_count: u32,
    pub model_id: String,
    pub compressed: bool,
}

// Migration chain
impl From<RecordV1> for RecordV3 {
    fn from(v1: RecordV1) -> Self {
        RecordV3 {
            timestamp: v1.timestamp,
            content: v1.content,
            token_count: 0,
            model_id: "unknown".to_string(),
            compressed: false,
        }
    }
}

impl From<RecordV2> for RecordV3 {
    fn from(v2: RecordV2) -> Self {
        RecordV3 {
            timestamp: v2.timestamp,
            content: v2.content,
            token_count: v2.token_count,
            model_id: "unknown".to_string(),
            compressed: false,
        }
    }
}

/// Read any version, migrate to latest
pub fn read_record(line: &str) -> Result<RecordV3, serde_json::Error> {
    let versioned: VersionedRecord = serde_json::from_str(line)?;
    Ok(match versioned {
        VersionedRecord::V1(v1) => v1.into(),
        VersionedRecord::V2(v2) => v2.into(),
        VersionedRecord::V3(v3) => v3,
    })
}
```

#### Pattern 3: Graceful Degradation Reader

```rust
/// Read NDJSON with mixed schema versions, skipping unparseable lines
pub fn read_mixed_ndjson<T: DeserializeOwned>(
    path: &Path,
) -> io::Result<(Vec<T>, Vec<String>)> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut records = Vec::new();
    let mut errors = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        match serde_json::from_str::<T>(&line) {
            Ok(record) => records.push(record),
            Err(e) => {
                errors.push(format!("Line {}: {}", line_num + 1, e));
                tracing::debug!(
                    line_num, error = %e,
                    "Skipping incompatible record"
                );
            }
        }
    }
    Ok((records, errors))
}
```

#### Pattern 4: magic_migrate for Complex Migrations

If migrations involve complex logic (not just additive fields), `magic_migrate` v2.0.0 provides a derive-based chain:

```rust
use magic_migrate::TryMigrate;

#[derive(TryMigrate, Deserialize)]
#[try_migrate(from = None)]
struct RecordV1 { name: String }

#[derive(TryMigrate, Deserialize)]
#[try_migrate(from = RecordV1)]
struct RecordV2 { full_name: String, token_count: u32 }

impl TryFrom<RecordV1> for RecordV2 {
    type Error = std::convert::Infallible;
    fn try_from(v1: RecordV1) -> Result<Self, Self::Error> {
        Ok(RecordV2 {
            full_name: v1.name,
            token_count: 0,
        })
    }
}

// Automatically tries each version in the chain
let record = RecordV2::try_from_str_migrations(json_str);
```

**Note**: magic_migrate defaults to TOML deserialization. For JSON NDJSON, you would need the custom `deserializer` attribute or manual deserialization.

#### Recommended Approach for Nika

**Use Pattern 1 (additive, serde defaults) as the primary approach**, escalating to Pattern 2 (tagged version dispatch) only for breaking changes:

```rust
/// Memory record with forward-compatible schema
#[derive(Serialize, Deserialize)]
pub struct MemoryRecord {
    /// Schema version (always written, defaults to 1 for old records)
    #[serde(default = "default_v1")]
    pub v: u32,

    /// Record type tag (discriminator)
    #[serde(rename = "type")]
    pub record_type: String,

    /// ISO 8601 timestamp
    pub ts: String,

    /// Record payload (varies by type)
    #[serde(flatten)]
    pub data: serde_json::Value,
}
```

---

### 28.7 Architectural Recommendation

#### Proposed Memory Module Structure

```
tools/nika/src/memory/
  mod.rs          -- Module exports
  record.rs       -- MemoryRecord types (versioned, forward-compatible)
  store.rs        -- AppendOnlyStore (NDJSON file writer/reader)
  compressor.rs   -- LLM-based summarization using rig-core Agent
  tokens.rs       -- Token counting utilities (tiktoken-rs wrapper)
  reader.rs       -- NDJSON reader with schema migration
```

#### Key Dependencies

```toml
[dependencies]
# Already in Nika
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["fs", "io-util"] }
parking_lot = "0.12"
rig-core = "0.32"

# New additions
tiktoken-rs = "0.9"           # Token counting (4.9M downloads, actively maintained)
# crc32fast = "1"             # Optional: integrity checking
```

#### Integration Points with Existing Nika Code

| Nika Component | Memory Integration |
|---------------|-------------------|
| `event::TraceWriter` | Extend NDJSON pattern for memory records (same architecture) |
| `io::atomic` | Use `write_append` for durable writes |
| `runtime::rig_agent_loop` | Use rig-core `PromptHook` to capture turns |
| `event::EventLog` | Emit memory events alongside workflow events |
| `event::log::AgentTurnMetadata` | Already captures thinking, tokens, stop_reason |
| `provider/` | Token counting for budget management |

#### How rig-core PromptHook Feeds the Memory Log

```
  +-------------------------------------------------------------+
  |  Nika Runtime                                                |
  |                                                              |
  |  RigAgentLoop                                                |
  |    |                                                         |
  |    +-- Agent.prompt("user message")                          |
  |         |                                                    |
  |         +-- .with_hook(NikaMemoryHook)                       |
  |              |                                               |
  |              on_completion_call() ----> MemoryStore.append()  |
  |              on_tool_call()       ----> MemoryStore.append()  |
  |              on_tool_result()     ----> MemoryStore.append()  |
  |              on_completion_response() -> MemoryStore.append() |
  |                                                              |
  |  MemoryStore                                                 |
  |    |                                                         |
  |    +-- .nika/memory/{session_id}.ndjson                      |
  |    |   (append-only, per-session NDJSON)                     |
  |    |                                                         |
  |    +-- Token counting via tiktoken-rs                        |
  |    +-- Rolling compression when budget exceeded              |
  |                                                              |
  +-------------------------------------------------------------+
```

---

## Part 2 Sources

### Official Documentation (Scraped)

1. LangGraph Persistence: https://docs.langchain.com/oss/python/langgraph/persistence
2. LangGraph Memory: https://docs.langchain.com/oss/python/langgraph/add-memory
3. LangGraph Memory Concepts: https://docs.langchain.com/oss/python/langgraph/memory
4. CrewAI Memory: https://docs.crewai.com/en/concepts/memory
5. Letta Memory Blocks: https://docs.letta.com/guides/core-concepts/memory/memory-blocks
6. Letta Archival Memory: https://docs.letta.com/guides/core-concepts/memory/archival-memory
7. Letta Shared Memory: https://docs.letta.com/guides/core-concepts/memory/shared-memory
8. Letta Conversations: https://docs.letta.com/guides/core-concepts/messages/conversations
9. Letta Stateful Agents: https://docs.letta.com/guides/core-concepts/stateful-agents
10. Mem0 Memory Types: https://docs.mem0.ai/core-concepts/memory-types
11. Mem0 Platform Overview: https://docs.mem0.ai/platform/overview

### Blog Posts & Articles

12. Letta Blog: Agent Memory -- https://www.letta.com/blog/agent-memory
13. CrewAI Blog: Cognitive Memory -- https://crewai.com/blog/how-we-built-cognitive-memory-for-agentic-systems
14. The Checkpoint Ledger Behind LangGraph -- https://zalt.me/blog/2025/12/checkpoint-ledger-langgraph
15. Cognee: From RAG to Graphs -- https://memgraph.com/blog/from-rag-to-graphs-cognee-ai-memory
16. Cognee + LanceDB Case Study -- https://lancedb.com/blog/case-study-cognee/

### Academic Papers

17. Memory in the Age of AI Agents -- arXiv:2512.13564
18. Constitutional Memory Architecture (CMA) -- arXiv:2603.04740v1
19. CoALA: Cognitive Architectures for Language Agents -- arXiv:2309.02427
20. Agent Memory Paper List (GitHub) -- https://github.com/Shichun-Liu/Agent-Memory-Paper-List

### Comparison Articles

21. 6 Best AI Agent Memory Frameworks (2026) -- https://machinelearningmastery.com/the-6-best-ai-agent-memory-frameworks-you-should-try-in-2026/
22. 8 Best AI Agent Memory Systems Compared -- https://vectorize.io/articles/best-ai-agent-memory-systems
23. Top Agentic AI Frameworks 2026 -- https://www.alphamatch.ai/blog/top-agentic-ai-frameworks-2026

### Rust Implementation Sources

24. **rig-core v0.32.0** -- https://github.com/0xPlaygrounds/rig (`rig/rig-core/`)
   - `src/completion/message.rs` -- Message types (User/Assistant/Reasoning)
   - `src/agent/completion.rs` -- Agent struct, Chat trait, build_completion_request
   - `src/agent/prompt_request/mod.rs` -- Multi-turn agent loop with tool calling
   - `src/agent/prompt_request/hooks.rs` -- PromptHook observability API (7 hooks)
   - `src/agent/mod.rs` -- Agent module documentation and examples
   - `src/completion/mod.rs` -- Prompt/Chat/Completion traits
   - `Cargo.toml` -- v0.32.0, schemars, rmcp 0.16
25. **tiktoken-rs v0.9.1** -- https://github.com/zurawiki/tiktoken-rs (4.9M downloads)
   - Supports o200k_harmony (GPT-5), o200k_base (GPT-4o/o3/o1), cl100k_base, p50k, r50k
26. **tokenizers v0.22.2** -- https://github.com/huggingface/tokenizers (12M downloads)
   - HuggingFace tokenizer library, needed for local model tokenization
27. **magic_migrate v2.0.0** -- https://github.com/schneems/magic_migrate (8.5K downloads)
   - Derive-based chain of TryFrom migrations
28. **serde_flow v1.1.1** -- crates.io (59.8K downloads) -- Binary versioning
29. **aol v0.3.2** -- crates.io (12K downloads) -- Append-only log with CRC32
30. **waly v0.1.4** -- crates.io (493 downloads) -- Simple WAL
31. **Nika source** -- `tools/nika/src/event/trace.rs` (existing NDJSON TraceWriter)
32. **Nika source** -- `tools/nika/src/io/atomic.rs` (existing atomic write primitives)
33. **Nika source** -- `tools/nika/src/event/log.rs` (EventKind: 34+ variants, AgentTurnMetadata)
34. **Nika source** -- `tools/nika/src/event/emitter.rs` (EventEmitter trait, NoopEmitter)

---

*Research compiled for Nika Evolution brainstorm series. All data from primary sources (official docs, papers, Rust crate source) cross-referenced where possible.*

---

<div align="center">

[← 16 Multimodal](./16-multimodal-and-tools.md) · [Index](./00-README.md)

</div>
