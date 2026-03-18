# Binary & Multimodal Content Flow Patterns in AI Workflow Engines

> Research report: How modern frameworks handle binary/multimodal data between workflow steps.
> Date: 2026-03-17 | Agent: claude-opus-4 | Pages analyzed: 18 | Sources: 6 frameworks + MCP spec

---

## Summary

Every major workflow engine has converged on a **two-tier architecture** for binary content: small payloads inline (base64), large payloads via external reference (URI/file ID pointing to object storage). The MCP protocol itself mandates base64 inline for `ImageContent`/`AudioContent` but offers `EmbeddedResource` + `ResourceLink` for reference-based alternatives. The industry consensus is: **never pass raw binary through the orchestration layer -- pass handles**.

---

## 1. MCP Protocol Spec (2025-03-26 + 2025-06-18)

### Content Types in Tool Call Results

The MCP spec defines `CallToolResult.content` as an array of `ContentBlock`, which is a union of:

```
ContentBlock = TextContent | ImageContent | AudioContent | ResourceLink | EmbeddedResource
```

#### ImageContent -- Inline Base64 (Mandatory)

```json
{
  "type": "image",
  "data": "base64-encoded-data",
  "mimeType": "image/png"
}
```

Schema from `schema.json`:
```json
{
  "description": "An image provided to or from an LLM.",
  "properties": {
    "data": {
      "description": "The base64-encoded image data.",
      "format": "byte",
      "type": "string"
    },
    "mimeType": { "type": "string" },
    "type": { "const": "image" },
    "annotations": { "$ref": "#/definitions/Annotations" }
  },
  "required": ["data", "mimeType", "type"]
}
```

**Key observation**: `data` is **required**. There is no `uri` field on `ImageContent`. MCP chose inline-only for images sent to the LLM.

#### AudioContent -- Same Pattern

```json
{
  "type": "audio",
  "data": "base64-encoded-audio-data",
  "mimeType": "audio/wav"
}
```

Also inline-only. `data` is required. No URI option.

#### BlobResourceContents -- Resources with Inline Binary

```json
{
  "uri": "file:///example.png",
  "mimeType": "image/png",
  "blob": "base64-encoded-data"
}
```

Used inside `EmbeddedResource`. Both `blob` AND `uri` are required. The URI provides a stable reference for re-fetching; the blob carries the actual content inline.

#### EmbeddedResource -- Inline Data + Re-fetchable URI

```json
{
  "type": "resource",
  "resource": {
    "uri": "resource://generated-image-123",
    "mimeType": "image/png",
    "blob": "base64-encoded-data"
  }
}
```

The resource wraps either `TextResourceContents` or `BlobResourceContents`. The URI enables the client to subscribe to or re-fetch the resource later.

#### ResourceLink -- NEW in 2025-06-18 (Reference-Only)

```json
{
  "type": "resource_link",
  "uri": "resource://generated-image-123",
  "name": "output.png",
  "mimeType": "image/png",
  "size": 1048576
}
```

**This is the key evolution.** `ResourceLink` was added in the June 2025 spec as a content block type that contains **no inline data** -- just a URI, name, MIME type, and optional size. The client must fetch the actual content via `resources/read`. This enables:
- Large binary references without bloating tool results
- Lazy loading by the client
- Size hints for context window estimation

### MCP Binary Architecture Summary

```
                         Inline data?    URI for re-fetch?    Size hint?
ImageContent              YES (required)  NO                   NO
AudioContent              YES (required)  NO                   NO
EmbeddedResource          YES (blob)      YES (required)       NO
ResourceLink              NO              YES (required)       YES (optional)
```

**Pattern**: MCP provides a spectrum from "send everything inline" (ImageContent) to "send nothing inline" (ResourceLink). The protocol leaves the choice to the server implementation.

Source: https://modelcontextprotocol.io/specification/2025-06-18/server/tools
Source: https://github.com/modelcontextprotocol/modelcontextprotocol (schema.json)

---

## 2. n8n -- Mature Binary Data System

### Architecture: Reference-Based with Pluggable Storage

n8n has the most mature binary handling system among workflow automation tools. The core insight: **binary data is never serialized into the workflow execution data**. Instead, nodes exchange lightweight metadata objects with `id` references.

#### IBinaryData -- What Flows Between Nodes

```typescript
// Conceptual structure (from n8n-workflow package)
interface IBinaryData {
  id?: string;           // Reference to stored binary (e.g., "filesystem-v2:abc123")
  data: string;          // Either base64 content OR storage mode name
  mimeType: string;
  fileName?: string;
  fileSize?: string;     // Human-readable (e.g., "1.5 MB")
  bytes?: number;        // Raw byte count
  fileExtension?: string;
}
```

The critical design: when a storage manager is active, `data` field is **overwritten with the storage mode name** (e.g., `"filesystem-v2"`) and the actual binary is stored externally. The `id` field becomes the retrieval key.

#### BinaryData.Manager Interface -- Pluggable Storage

```typescript
// From packages/core/src/binary-data/types.ts
namespace BinaryData {
  type ServiceMode = 'filesystem-v2' | 's3' | 'default';

  type FileLocation =
    | { type: 'execution'; workflowId: string; executionId: string }
    | { type: 'custom'; pathSegments: string[]; sourceType?: string; sourceId?: string };

  interface Manager {
    init(): Promise<void>;
    store(location: FileLocation, bufferOrStream: Buffer | Readable,
          metadata: PreWriteMetadata): Promise<WriteResult>;
    getPath(fileId: string): string;
    getAsBuffer(fileId: string): Promise<Buffer>;
    getAsStream(fileId: string, chunkSize?: number): Promise<Readable>;
    getMetadata(fileId: string): Promise<Metadata>;
    copyByFileId(targetLocation: FileLocation, sourceFileId: string): Promise<string>;
    copyByFilePath(targetLocation: FileLocation, sourcePath: string,
                   metadata: PreWriteMetadata): Promise<WriteResult>;
    rename(oldFileId: string, newFileId: string): Promise<void>;
  }
}
```

#### Three Storage Modes

| Mode | Storage | When to Use |
|------|---------|-------------|
| `default` | In-memory (base64 in `data` field) | Dev/testing, small payloads |
| `filesystem-v2` | Local disk at `~/.n8n/binary-data/` | Self-hosted single instance |
| `s3` | AWS S3 / compatible object store | Production, multi-instance |

#### Store Flow (from BinaryDataService)

```typescript
async store(location, bufferOrStream, binaryData) {
  const manager = this.managers[this.mode];

  if (!manager) {
    // Default mode: inline base64
    const buffer = await binaryToBuffer(bufferOrStream);
    binaryData.data = buffer.toString(BINARY_ENCODING);
    binaryData.fileSize = prettyBytes(buffer.length);
    return binaryData;
  }

  // External storage mode: store externally, replace data with reference
  const { fileId, fileSize } = await manager.store(location, bufferOrStream, metadata);
  binaryData.id = this.createBinaryDataId(fileId);
  binaryData.data = this.mode;  // <-- KEY: replaces base64 with mode name
  binaryData.fileSize = prettyBytes(fileSize);
  return binaryData;
}
```

#### Key Design Decisions

1. **Stream support**: `store()` accepts `Buffer | Readable` -- enables streaming without full buffering
2. **JWT-signed URLs**: Binary data served via signed tokens for secure client access
3. **Execution-scoped locations**: Binary stored under `{workflowId}/{executionId}` paths
4. **Copy-on-reference**: `copyByFileId` enables sharing without duplication

Source: https://github.com/n8n-io/n8n (packages/core/src/binary-data/)

---

## 3. Dify -- File Variable System

### Architecture: File References with Multiple Transfer Methods

Dify uses a `File` Pydantic model with four transfer methods, abstracting away how the binary was obtained.

#### File Model (from `api/dify_graph/file/models.py`)

```python
class File(BaseModel):
    dify_model_identity: str = FILE_MODEL_IDENTITY
    id: str | None = None          # message file id
    tenant_id: str
    type: FileType                 # IMAGE, DOCUMENT, AUDIO, VIDEO, CUSTOM
    transfer_method: FileTransferMethod
    remote_url: str | None = None  # if transfer_method == REMOTE_URL
    related_id: str | None = None  # if transfer_method in (LOCAL_FILE, TOOL_FILE)
    filename: str | None = None
    extension: str | None = None
    mime_type: str | None = None
    size: int = -1
    _storage_key: str              # private, for internal storage lookup
```

#### FileTransferMethod Enum

```python
class FileTransferMethod(StrEnum):
    REMOTE_URL = "remote_url"       # External URL (e.g., CDN, S3 presigned)
    LOCAL_FILE = "local_file"       # Uploaded to Dify storage
    TOOL_FILE = "tool_file"         # Generated by a tool/plugin
    DATASOURCE_FILE = "datasource_file"  # From a knowledge base
```

#### FileType Enum

```python
class FileType(StrEnum):
    IMAGE = "image"
    DOCUMENT = "document"
    AUDIO = "audio"
    VIDEO = "video"
    CUSTOM = "custom"
```

#### How Files Flow Between Workflow Nodes

1. **No inline base64 between nodes**: Files flow as `File` model instances (metadata + reference)
2. **URL generation on demand**: `generate_url()` creates signed URLs for retrieval
3. **Workflow variable**: Files exposed as `sys.file` or `Array[File]` in the visual builder
4. **Plugin interface**: Tools receive files as `{url, mime_type, filename, size}` dicts
5. **Storage backends**: S3, Azure Blob, Google Cloud Storage, local filesystem

#### Key Design Pattern

```python
def to_plugin_parameter(self) -> dict:
    return {
        "dify_model_identity": FILE_MODEL_IDENTITY,
        "mime_type": self.mime_type,
        "filename": self.filename,
        "extension": self.extension,
        "size": self.size,
        "type": self.type,
        "url": self.generate_url(for_external=False),  # Signed URL
    }
```

Source: https://github.com/langgenius/dify (api/dify_graph/file/)

---

## 4. LangGraph/LangChain -- State Channel Pattern

### Architecture: Base64 in Typed State Channels

LangGraph handles multimodal content through typed state dictionaries where binary data is stored as base64 strings in explicit state channels.

#### Multimodal State Pattern

```python
from typing import TypedDict, Annotated
from langgraph.graph import StateGraph, add_messages

class MultimodalState(TypedDict):
    messages: Annotated[list, add_messages]  # Standard message channel
    image_data: str       # Base64-encoded image (custom channel)
    generated_image: str  # Another base64 channel

def analyze_image(state: MultimodalState):
    msg = HumanMessage(content=[
        {"type": "text", "text": "Identify objects."},
        {"type": "image_url", "image_url": {
            "url": f"data:image/jpeg;base64,{state['image_data']}"
        }}
    ])
    response = llm.invoke([msg])
    return {"messages": [response], "image_data": state["image_data"]}
```

#### Image Generation Output Passing

```python
def generator_node(state):
    result = llm.invoke(state["messages"])
    image_b64 = call_dalle(result)
    return {"generated_image": image_b64}

def consumer_node(state):
    content = [
        {"type": "text", "text": "Describe this image:"},
        {"type": "image_url", "image_url": {
            "url": f"data:image/png;base64,{state['generated_image']}"
        }}
    ]
    return {"messages": [llm.invoke(content)]}
```

#### Limitations of LangGraph's Approach

- **No built-in binary storage**: Everything is base64 in the state dict
- **State serialization**: Full state is serialized at each checkpoint, including all base64 data
- **Memory pressure**: Large images duplicated across checkpoints
- **No streaming**: Full buffer required before passing to next node

This is the **simplest but least scalable** approach among all frameworks surveyed.

Source: https://docs.langchain.com/oss/python/langgraph/

---

## 5. Temporal / Restate -- Durable Execution Patterns

### Temporal: Strict Limits + Codec-Based Offloading

Temporal enforces hard limits that force external storage:
- **4MB per payload** (gRPC limit)
- **50MB total workflow history**
- **50K events per execution**

#### The Payload Codec Pattern (DataDog's temporal-large-payload-codec)

This is a **content-addressed storage** system that transparently intercepts serialization.

**Before encoding (original payload):**
```json
{
  "metadata": { "encoding": "text/plain" },
  "data": "... (large binary, >128KB) ..."
}
```

**After encoding (reference payload):**
```json
{
  "metadata": {
    "encoding": "json/plain",
    "temporal.io/remote-codec": "v2"
  },
  "data": {
    "metadata": { "encoding": "text/plain" },
    "size": 1234567,
    "digest": "sha256:deadbeef",
    "key": "/blobs/default/sha256:deadbeef"
  }
}
```

#### Architecture

```
Temporal Client/Worker          Large Payload Service
+-------------------+          +--------------------+
| Temporal SDK      |          | HTTP API           |
| Large Payload     |--PUT-->  |                    |
|   Codec           |<-key---  | Cloud Storage      |
+-------------------+          | (S3/GCS/Azure)     |
                               +--------------------+
```

#### Content-Addressed Storage

The codec computes `sha256:digest` of the payload data and uses it as the storage key. This provides:
- **Deduplication**: Identical payloads stored once, referenced many times
- **Integrity**: Checksum verified on read
- **Immutability**: Content cannot change once stored

#### Codec Integration (Go)

```go
lpc, _ := largepayloadcodec.New(
    largepayloadcodec.WithURL("http://lps:8577"),
)
opts.DataConverter = converter.NewCodecDataConverter(
    opts.DataConverter, lpc,
)
```

The codec is **transparent** -- workflow code never sees it. The SDK automatically encodes/decodes at the boundary.

### Restate: Compact Journal with External State

Restate stores workflow journals in embedded RocksDB. Lacks explicit large-binary guidance but:
- Journal entries are kept lean (results, not payloads)
- Virtual objects can hold explicit state
- No native codec like Temporal's
- Recommends external blob store for large data

Source: https://github.com/DataDog/temporal-large-payload-codec

---

## 6. Flyte -- Type-Annotated File References

### Architecture: Automatic Upload/Download via Type System

Flyte has the most elegant solution for large files: type annotations that trigger automatic cloud storage operations.

#### FlyteFile Pattern

```python
from flytekit import task, workflow
from flytekit.types.file import FlyteFile

@task
def generate_image(prompt: str) -> FlyteFile:
    # Generate image locally
    path = "/tmp/output.png"
    generate_to_file(prompt, path)
    return FlyteFile(path)  # Auto-uploaded to S3/GCS

@task
def analyze_image(image: FlyteFile) -> str:
    # Auto-downloaded from S3/GCS when accessed
    with open(image) as f:
        return classifier.predict(f.read())

@workflow
def pipeline(prompt: str) -> str:
    image = generate_image(prompt=prompt)
    return analyze_image(image=image)
```

#### What Flows Between Tasks

- **Not the file bytes** -- only a metadata envelope (~100 bytes)
- Contains: S3/GCS URI, MIME type, hash
- Automatic multipart upload for files >5GB
- Lazy download: bytes only fetched when `open()` is called

#### FlyteDirectory for Multi-File Outputs

```python
@task
def batch_generate(prompts: list[str]) -> FlyteDirectory:
    output_dir = "/tmp/batch"
    for i, p in enumerate(prompts):
        generate_to_file(p, f"{output_dir}/{i}.png")
    return FlyteDirectory(output_dir)  # Entire directory uploaded
```

Source: https://docs.flyte.org/

---

## 7. Cross-Framework Pattern Analysis

### The Universal Two-Tier Architecture

Every framework converges on the same pattern:

```
+-----------+     +-----------+     +-----------+
|  Task A   |     | Workflow  |     |  Task B   |
|           |     | Engine    |     |           |
| generate  |---->| stores    |---->| consume   |
| binary    |     | reference |     | binary    |
| data      |     | only      |     | data      |
+-----------+     +-----------+     +-----------+
      |                                   |
      v                                   v
+---------------------------------------------+
|           External Binary Store              |
|   (S3, filesystem, object store, CAS)        |
+---------------------------------------------+
```

### Comparison Matrix

| Framework | Inline Pattern | Reference Pattern | Storage Backends | CAS | Streaming |
|-----------|---------------|-------------------|-----------------|-----|-----------|
| **MCP** | ImageContent (base64, required) | ResourceLink (URI, no data) | Server-defined | No | No |
| **n8n** | Default mode (base64 in `data`) | filesystem-v2/s3 (id reference) | FS, S3 | No | Yes (Readable) |
| **Dify** | None | File model with signed URLs | S3, Azure, GCS, FS | No | No |
| **LangGraph** | Base64 in state channels | None (no built-in) | None | No | No |
| **Temporal** | Small payloads (<128KB inline) | Codec + CAS (sha256 key) | S3, GCS, Azure | **YES** | Yes |
| **Flyte** | None | FlyteFile (S3 URI) | S3, GCS | No | Yes (async) |

### Size Thresholds

| Framework | Inline Threshold | Max Payload | Reference Mechanism |
|-----------|-----------------|-------------|-------------------|
| **MCP** | No explicit limit | No limit stated | URI in ResourceLink |
| **n8n** | Configurable (default: all inline) | No limit | `binaryData.id` |
| **Temporal** | 128KB (default, configurable) | 4MB (hard gRPC limit) | `sha256:digest` key |
| **Flyte** | ~1MB (FlyteSchema) | Unlimited | S3/GCS URI |

---

## 8. Key Implementation Patterns

### Pattern 1: Handle/Reference Architecture

**The most common pattern.** Store binary externally, pass a lightweight handle through the workflow.

```
Handle = {
  id: string           // Unique identifier (UUID, hash, path)
  uri: string          // Retrieval URI (s3://, file://, http://)
  mime_type: string    // Content type
  size: number         // Byte count
  checksum?: string    // Integrity verification
}
```

Used by: n8n, Dify, Flyte, Temporal (via codec)

### Pattern 2: Content-Addressed Storage (CAS)

**Hash the content, use hash as key.** Enables deduplication and integrity verification.

```
store(data) -> sha256:digest
retrieve(sha256:digest) -> data

key = sha256(data)
storage[key] = data
handle = { digest: key, size: len(data) }
```

Used by: Temporal (DataDog codec), Bazel remote cache, Docker layer cache, Nix store

Benefits:
- Identical outputs from different tasks stored once
- Integrity verification built-in
- Immutable -- no accidental overwrites
- Natural deduplication across workflow runs

### Pattern 3: Pluggable Storage Manager

**Abstract the storage backend behind an interface.**

```typescript
interface BinaryManager {
  store(data: Buffer | Stream, metadata: Meta): Promise<Handle>;
  retrieve(handle: Handle): Promise<Buffer>;
  stream(handle: Handle): Promise<ReadableStream>;
  delete(handle: Handle): Promise<void>;
}

// Implementations:
class FilesystemManager implements BinaryManager { ... }
class S3Manager implements BinaryManager { ... }
class InMemoryManager implements BinaryManager { ... }
```

Used by: n8n (BinaryData.Manager), Dify (storage backends), Temporal (storage drivers)

### Pattern 4: Transparent Codec / Interceptor

**Intercept serialization to offload large payloads without changing workflow code.**

```
Workflow code writes: output = large_binary_data
Codec intercepts:     if sizeof(output) > threshold:
                        key = store_externally(output)
                        output = { ref: key, size: N, digest: sha256 }
Next step reads:      Codec intercepts: fetch(ref) -> original data
```

Used by: Temporal (PayloadCodec), n8n (BinaryDataService.store)

### Pattern 5: Type-Annotated Automatic Transfer

**Use the type system to trigger upload/download at task boundaries.**

```python
@task
def produce() -> FileRef:       # Type annotation triggers upload
    return FileRef("output.png")

@task
def consume(f: FileRef) -> str: # Type annotation triggers download
    with open(f) as data:
        return process(data)
```

Used by: Flyte (FlyteFile/FlyteDirectory)

### Pattern 6: Lazy/On-Demand Loading

**Pass reference immediately, download bytes only when accessed.**

```python
class LazyFile:
    def __init__(self, uri, size, mime_type):
        self.uri = uri
        self.size = size
        self.mime_type = mime_type
        self._data = None

    def read(self) -> bytes:
        if self._data is None:
            self._data = fetch(self.uri)
        return self._data
```

Used by: Flyte (FlyteFile.open()), MCP (ResourceLink + resources/read)

---

## 9. Implications for Nika

### Current State

Nika currently has no binary content flow mechanism. Task outputs are text/JSON. The `infer:` verb returns text. `exec:` returns stdout (text). `fetch:` returns HTTP responses (text/JSON). `invoke:` returns MCP tool results which CAN contain `ImageContent` (base64) but Nika has no way to route that binary data to the next task.

### Recommended Architecture for Nika

Based on this research, the recommended approach combines patterns from n8n, Temporal, and MCP:

#### Tier 1: Artifact Store (Core)

```yaml
# Conceptual design
artifact:
  id: "sha256:a1b2c3..."       # Content-addressed (Pattern 2)
  uri: "artifact://sha256:a1b2c3"
  mime_type: "image/png"
  size: 1048576
  created_by: "task-generate-image"
  storage: "filesystem"         # or "s3", "memory"
```

#### Tier 2: Transparent Flow via Bindings

```yaml
tasks:
  - name: generate-image
    infer:
      model: gpt-image-1
      prompt: "A sunset over mountains"
    # Output includes ImageContent -> auto-stored as artifact

  - name: analyze-image
    infer:
      model: gpt-4o
      prompt: "Describe this image"
      images:
        - "{{with.generate-image.artifacts[0]}}"  # Reference, not bytes
    depends_on: [generate-image]
```

#### Tier 3: Pluggable Storage Backend

```rust
// Conceptual Rust interface
trait ArtifactStore: Send + Sync {
    async fn store(&self, data: &[u8], meta: ArtifactMeta) -> Result<ArtifactHandle>;
    async fn retrieve(&self, handle: &ArtifactHandle) -> Result<Vec<u8>>;
    async fn stream(&self, handle: &ArtifactHandle) -> Result<impl AsyncRead>;
    async fn exists(&self, handle: &ArtifactHandle) -> Result<bool>;
}

struct FilesystemStore { base_path: PathBuf }
struct S3Store { bucket: String, client: S3Client }
struct MemoryStore { cache: DashMap<String, Bytes> }
```

---

## Sources

1. **MCP Specification** (2025-03-26 + 2025-06-18) - https://modelcontextprotocol.io/specification/
   - JSON Schema: https://github.com/modelcontextprotocol/modelcontextprotocol
   - Provided: Exact type definitions for ImageContent, AudioContent, BlobResourceContents, EmbeddedResource, ResourceLink, CallToolResult

2. **n8n Source Code** - https://github.com/n8n-io/n8n
   - `packages/core/src/binary-data/binary-data.service.ts` - BinaryDataService implementation
   - `packages/core/src/binary-data/types.ts` - BinaryData.Manager interface
   - Provided: Pluggable storage manager pattern, reference-based binary flow

3. **Dify Source Code** - https://github.com/langgenius/dify
   - `api/dify_graph/file/models.py` - File model with transfer methods
   - `api/dify_graph/file/enums.py` - FileType, FileTransferMethod enums
   - Provided: File variable system with signed URL generation

4. **LangGraph/LangChain** - https://docs.langchain.com/oss/python/langgraph/
   - Provided: Base64 state channel pattern, multimodal message format

5. **Temporal Large Payload Codec** (DataDog) - https://github.com/DataDog/temporal-large-payload-codec
   - Provided: Content-addressed storage, transparent codec pattern, SHA256 deduplication

6. **Flyte** - https://docs.flyte.org/
   - Provided: FlyteFile type annotation pattern, automatic S3 upload/download

## Methodology

- Tools used: Perplexity search (7 queries), Firecrawl scrape (4 pages), GitHub API (3 repos), raw file fetch (6 files)
- Schema directly parsed: MCP JSON schema (2025-03-26 and 2025-06-18 versions)
- Source code directly read: n8n BinaryDataService, Dify File model, Temporal codec README

## Confidence Level

**High** -- All claims are backed by either spec documents or source code. The MCP types are directly from the JSON schema. The n8n and Dify patterns are from actual production source code. The Temporal codec architecture is from DataDog's open-source implementation.

## Further Research Suggestions

- **MCP Streamable HTTP transport**: How does the new (2025-06-18) Streamable HTTP transport affect binary data transfer? Does it enable chunked binary streaming?
- **Nika artifact store sizing**: What are realistic payload sizes for AI workflow outputs? Image generation (1-10MB), audio generation (10-100MB), video generation (100MB-1GB)?
- **Content-addressed vs UUID-based storage**: For Nika's use case, is CAS (sha256) worth the hashing cost, or is UUID + separate checksum simpler?
- **Memory-mapped files in Rust**: Can `mmap` be used for zero-copy binary passing between Nika tasks running in the same process?
- **MCP binary streaming proposal**: Is there an open SEP (Spec Enhancement Proposal) for streaming binary content in MCP?
