# Research Report: VS Code DAG/Graph Webview Visualization

> Date: 2026-04-06
> Scope: Technical patterns for interactive workflow DAG visualization in VS Code extension webviews
> Use case: Nika workflow engine -- visualizing `.nika.yaml` DAG with colored verb-type nodes and live execution status

---

## Executive Summary

For a VS Code extension rendering Nika workflow DAGs, the recommended stack is **ELK.js for layout + D3.js for SVG rendering**, bundled with **esbuild** as a separate browser target. For richer interactivity (drag, zoom, minimap), **React Flow (@xyflow/react) with dagre layout** is a strong alternative but requires React/Preact. Communication uses the standard `postMessage` bidirectional API. Real-time execution status updates are best implemented as extension-push via `postMessage` with throttling.

---

## 1. JavaScript Libraries for DAG Visualization in VS Code Webviews

### Comparison Matrix

| Library | DAG Layout | Bundle Size | Interactivity | VS Code Fit | Maintenance |
|---------|-----------|-------------|---------------|-------------|-------------|
| **ELK.js** | Excellent (Sugiyama layered, mrtree) | ~150KB | Good (via D3 integration) | Excellent | Active (Eclipse Foundation) |
| **D3.js + d3-dag** | Excellent (Sugiyama, Zherebko) | ~250KB (d3 core + d3-dag) | Excellent (full event control) | Excellent | d3-dag active (erikbrinkman) |
| **dagre / @dagrejs/dagre** | Good (Sugiyama layered) | ~30KB | Basic (needs D3/SVG wrapper) | Good | Original abandoned; @dagrejs fork maintained |
| **React Flow (@xyflow/react)** | Good (via dagre/ELK plugins) | ~150KB + React (~42KB) | Outstanding (built-in zoom/pan/minimap) | Good (needs React or Preact) | Very active |
| **Cytoscape.js** | Good (breadthfirst, dagre extension) | ~500KB | Outstanding (100+ extensions) | Moderate (heavy) | Active |
| **vis.js (vis-network)** | Moderate (hierarchical, no native layered) | ~200KB | Excellent (physics, zoom/pan) | Good | Community maintained |
| **Mermaid** | Basic (flowchart) | ~100KB | Basic (static SVG) | Good for previews | Active |

### Recommendations

**Option A -- Lightweight custom (RECOMMENDED for Nika):**
- **ELK.js** for layout computation (best Sugiyama layered algorithm, configurable direction/spacing)
- **D3.js** for SVG rendering (full control over node shapes, colors, click handlers)
- Total bundle: ~400KB, zero framework dependency
- Best for: Custom verb-colored nodes, edge routing, surgical control over rendering

**Option B -- Rich interactive (if drag-and-drop or minimap needed):**
- **React Flow (@xyflow/react)** with `@dagrejs/dagre` for auto-layout
- **Preact** via `preact/compat` to cut React's 42KB overhead to ~3KB
- Total bundle: ~200KB with Preact
- Best for: If users need to rearrange nodes, has built-in minimap/controls

**Option C -- Minimal (for sidebar preview):**
- **Mermaid.js** for simple DAG preview
- Zero interactivity but instant rendering
- Best for: Quick read-only preview in sidebar WebviewView

### ELK.js Configuration for Pipeline DAGs

```typescript
const graphData = {
  id: 'root',
  layoutOptions: {
    'elk.algorithm': 'layered',
    'elk.direction': 'DOWN',              // Top-to-bottom pipeline
    'elk.spacing.nodeNode': '50',         // Vertical spacing
    'elk.layered.spacing.nodeNodeBetweenLayers': '80', // Between layers
    'elk.spacing.edgeEdge': '20',
    'elk.layered.mergeEdges': '1',        // Clean edge routing
    'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
    'elk.edgeRouting': 'ORTHOGONAL',      // Right-angle edges (pipeline style)
  },
  children: [
    { id: 'research', width: 160, height: 48, labels: [{ text: 'research' }] },
    { id: 'summarize', width: 160, height: 48, labels: [{ text: 'summarize' }] },
  ],
  edges: [
    { id: 'e1', sources: ['research'], targets: ['summarize'] },
  ],
};
```

---

## 2. Webview postMessage Communication

### Bidirectional Pattern

**Extension -> Webview (push data):**
```typescript
// Extension side (TypeScript)
panel.webview.postMessage({
  type: 'dagData',
  nodes: [...],
  edges: [...],
});
```

**Webview -> Extension (events):**
```typescript
// Webview side (JavaScript in HTML)
const vscode = acquireVsCodeApi(); // Call ONCE, reuse

// Send event to extension
vscode.postMessage({
  type: 'nodeClick',
  nodeId: 'research',
  filePath: '/path/to/workflow.nika.yaml',
  line: 12,
});

// Receive data from extension
window.addEventListener('message', (event) => {
  const msg = event.data;
  switch (msg.type) {
    case 'dagData':
      renderGraph(msg.nodes, msg.edges);
      break;
    case 'statusUpdate':
      updateNodeStatus(msg.nodeId, msg.status);
      break;
  }
});
```

**Extension receiving webview events:**
```typescript
panel.webview.onDidReceiveMessage(
  (message) => {
    switch (message.type) {
      case 'nodeClick':
        navigateToTaskDefinition(message.filePath, message.line);
        break;
      case 'ready':
        // Webview loaded, send initial data
        sendDagData(panel);
        break;
    }
  },
  undefined,
  context.subscriptions,
);
```

### Typed Message Protocol

```typescript
// Shared types (used by both extension and webview)
type ExtensionToWebview =
  | { type: 'dagData'; nodes: DagNode[]; edges: DagEdge[] }
  | { type: 'statusUpdate'; nodeId: string; status: TaskStatus }
  | { type: 'highlight'; nodeId: string }
  | { type: 'theme'; isDark: boolean };

type WebviewToExtension =
  | { type: 'nodeClick'; nodeId: string; filePath: string; line: number }
  | { type: 'ready' }
  | { type: 'zoomChanged'; level: number };
```

### State Persistence

```typescript
// Webview side -- survives panel hide/show
const vscode = acquireVsCodeApi();
const state = vscode.getState() || { zoom: 1, panX: 0, panY: 0 };

// Save on change
function onZoomPan(zoom, x, y) {
  vscode.setState({ zoom, panX: x, panY: y });
}
```

---

## 3. Webview Performance Best Practices

### SVG vs Canvas

| Aspect | SVG | Canvas |
|--------|-----|--------|
| **Node count < 200** | Preferred | Overkill |
| **Node count > 500** | Sluggish (DOM bloat) | Preferred |
| **Interactivity** | Native DOM events per element | Manual hit testing |
| **Text rendering** | Crisp, selectable | Blurry at low res |
| **Accessibility** | Good (DOM elements) | Poor |
| **For Nika** | **Recommended** (typical DAGs are 5-50 nodes) | Not needed |

### Framework Choice for Webview

| Framework | Size (gzip) | Best For |
|-----------|-------------|----------|
| **Vanilla D3** | 0KB framework overhead | Full control, smallest bundle |
| **Lit** | ~5KB | Web components, fast updates |
| **Preact** | ~3KB | React API compat, hooks |
| **React** | ~42KB | Only if using React Flow |

**Recommendation for Nika:** Vanilla D3.js with no framework. Nika DAGs are small (5-50 nodes), updates are infrequent (task status changes), and D3's data-join pattern handles incremental updates efficiently.

### Key Performance Settings

```typescript
const panel = vscode.window.createWebviewPanel(
  'nikaDag',
  'Nika DAG',
  vscode.ViewColumn.Beside,
  {
    enableScripts: true,
    retainContextWhenHidden: true,  // CRITICAL: preserve graph state on tab switch
    localResourceRoots: [
      vscode.Uri.joinPath(context.extensionUri, 'dist'),
      vscode.Uri.joinPath(context.extensionUri, 'media'),
    ],
  },
);
```

### Debouncing Updates

```typescript
// Extension side: throttle status updates to 60fps max
let pendingUpdates: Map<string, TaskStatus> = new Map();
let updateTimer: NodeJS.Timeout | undefined;

function queueStatusUpdate(nodeId: string, status: TaskStatus) {
  pendingUpdates.set(nodeId, status);
  if (!updateTimer) {
    updateTimer = setTimeout(() => {
      panel.webview.postMessage({
        type: 'batchStatusUpdate',
        updates: Object.fromEntries(pendingUpdates),
      });
      pendingUpdates.clear();
      updateTimer = undefined;
    }, 16); // ~60fps
  }
}
```

---

## 4. Existing Extensions with Graph Panels

### Reference Implementations

| Extension | GitHub | Library | Navigation |
|-----------|--------|---------|------------|
| **hediet/vscode-drawio** | github.com/hediet/vscode-drawio | mxGraph (Diagrams.net) embedded | Shape metadata -> postMessage -> showTextDocument |
| **vscode-interactive-graphviz** | github.com/tintinweb/vscode-interactive-graphviz | Viz.js (WASM Graphviz) + D3.js zoom | SVG node click -> postMessage -> highlight source |
| **CodeGraphy** | marketplace: codegraphy.codegraphy | D3.js force simulation | File nodes -> postMessage -> openTextDocument |
| **Mermaid Chart** | marketplace | Mermaid.js core | SVG elements -> postMessage -> revealRange |
| **Jupyter** | github.com/microsoft/vscode-jupyter | Plotly.js, D3.js | Cell outputs -> revealCellRangeInView |

### Key Patterns from These Extensions

1. **All use `createWebviewPanel` with `enableScripts: true`**
2. **All bundle graph library into a single JS file loaded via `asWebviewUri`**
3. **All use postMessage for click-to-source navigation**
4. **draw.io and Graphviz use `retainContextWhenHidden: true`** for complex state
5. **CodeGraphy demonstrates D3.js force graph in webview** -- closest to our use case

---

## 5. Click-to-Source Navigation

### Complete Implementation

```typescript
// Extension: handle node click from webview
panel.webview.onDidReceiveMessage(async (msg) => {
  if (msg.type === 'nodeClick') {
    const uri = vscode.Uri.file(msg.filePath);
    const start = new vscode.Position(msg.line - 1, 0);  // 0-indexed
    const end = new vscode.Position(msg.endLine - 1, 999);
    const range = new vscode.Range(start, end);

    await vscode.window.showTextDocument(uri, {
      selection: range,     // Highlights the YAML task block
      preview: false,       // Open as persistent tab
      viewColumn: vscode.ViewColumn.One,  // Editor column
    });
  }
});
```

### Mapping Nika Task IDs to YAML Locations

The extension needs a mapping from task ID to source location. Two approaches:

**Approach A -- Parse YAML with line tracking:**
```typescript
import * as yaml from 'yaml';

function getTaskLocations(content: string): Map<string, { line: number; endLine: number }> {
  const doc = yaml.parseDocument(content);
  const locations = new Map();
  const tasks = doc.get('tasks');
  if (tasks && yaml.isSeq(tasks)) {
    for (const item of tasks.items) {
      if (yaml.isMap(item)) {
        const idNode = item.get('id', true);
        if (idNode && item.range) {
          locations.set(String(idNode), {
            line: /* compute from range[0] offset */,
            endLine: /* compute from range[1] offset */,
          });
        }
      }
    }
  }
  return locations;
}
```

**Approach B -- Use `nika check --json` output:**
If Nika CLI can emit AST with source locations, pipe that to the extension.

### Webview Side

```javascript
// D3.js click handler on SVG nodes
d3.selectAll('.dag-node').on('click', function (event, d) {
  event.stopPropagation();
  vscode.postMessage({
    type: 'nodeClick',
    nodeId: d.id,
    filePath: d.sourceFile,
    line: d.sourceLine,
    endLine: d.sourceEndLine,
  });
});
```

---

## 6. Webview Security (CSP)

### Complete CSP Pattern with Nonce

```typescript
function getNonce(): string {
  let text = '';
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  for (let i = 0; i < 32; i++) {
    text += chars.charAt(Math.floor(Math.random() * chars.length));
  }
  return text;
}

function getWebviewContent(
  webview: vscode.Webview,
  extensionUri: vscode.Uri,
): string {
  const nonce = getNonce();

  const scriptUri = webview.asWebviewUri(
    vscode.Uri.joinPath(extensionUri, 'dist', 'webview', 'dag.js'),
  );
  const styleUri = webview.asWebviewUri(
    vscode.Uri.joinPath(extensionUri, 'dist', 'webview', 'dag.css'),
  );

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy"
    content="default-src 'none';
             img-src ${webview.cspSource} https: data:;
             style-src ${webview.cspSource} 'nonce-${nonce}';
             script-src ${webview.cspSource} 'nonce-${nonce}';
             font-src ${webview.cspSource};">
  <link href="${styleUri}" rel="stylesheet">
  <title>Nika DAG</title>
</head>
<body>
  <div id="dag-container"></div>
  <script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
}
```

### Security Checklist

- `default-src 'none'` -- deny everything by default
- `script-src` with nonce -- only extension scripts + nonce-tagged inline
- `style-src` with `webview.cspSource` -- only extension styles
- `img-src` with `data:` -- needed for inline SVG images
- `localResourceRoots` -- restrict to `dist/` and `media/` directories only
- NO `'unsafe-inline'` or `'unsafe-eval'` -- these defeat CSP
- Webpack/esbuild: disable sourcemap generation that uses code evaluation in production builds

---

## 7. Real-Time Execution Status Updates

### Architecture

```
Nika CLI (execution)
    |
    | stdout/stderr (NDJSON events)
    v
VS Code Extension (watches execution)
    |
    | postMessage (throttled)
    v
Webview (D3.js SVG updates)
```

### Extension: Watch Execution and Push Status

```typescript
class DagExecutionWatcher {
  private panel: vscode.WebviewPanel;
  private pendingUpdates = new Map<string, TaskStatus>();
  private flushTimer?: NodeJS.Timeout;

  constructor(panel: vscode.WebviewPanel) {
    this.panel = panel;
  }

  // Called when nika run emits a task event
  onTaskEvent(taskId: string, status: TaskStatus, duration?: number) {
    this.pendingUpdates.set(taskId, status);
    this.scheduleFlush();
  }

  private scheduleFlush() {
    if (!this.flushTimer) {
      this.flushTimer = setTimeout(() => {
        this.panel.webview.postMessage({
          type: 'statusBatch',
          updates: Array.from(this.pendingUpdates.entries()).map(
            ([id, status]) => ({ id, status }),
          ),
        });
        this.pendingUpdates.clear();
        this.flushTimer = undefined;
      }, 50); // 20fps is plenty for status changes
    }
  }
}
```

### Webview: Update Node Appearance

```javascript
// Status -> visual mapping
const STATUS_STYLES = {
  pending:  { fill: '#6b7280', stroke: '#4b5563', icon: 'circle',   animate: false },
  running:  { fill: '#3b82f6', stroke: '#2563eb', icon: 'spinner',  animate: true  },
  success:  { fill: '#10b981', stroke: '#059669', icon: 'check',    animate: false },
  failed:   { fill: '#ef4444', stroke: '#dc2626', icon: 'x',        animate: false },
  skipped:  { fill: '#9ca3af', stroke: '#6b7280', icon: 'skip',     animate: false },
};

// Verb type -> base color
const VERB_COLORS = {
  infer:  '#8b5cf6',  // Purple -- LLM generation
  exec:   '#f59e0b',  // Amber -- shell commands
  fetch:  '#06b6d4',  // Cyan -- HTTP requests
  invoke: '#ec4899',  // Pink -- tool calls
  agent:  '#14b8a6',  // Teal -- multi-turn agents
};

function updateNodeStatus(nodeId, status) {
  const style = STATUS_STYLES[status];
  const node = d3.select(`#node-${nodeId}`);

  // Update fill with transition
  node.select('.node-bg')
    .transition()
    .duration(300)
    .attr('fill', style.fill)
    .attr('stroke', style.stroke);

  // Running spinner animation
  if (style.animate) {
    node.select('.status-icon')
      .classed('spinning', true);
  } else {
    node.select('.status-icon')
      .classed('spinning', false);
  }

  // Update status icon
  node.select('.status-icon text')
    .text(getStatusIcon(status));
}

// CSS for spinner
// @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
// .spinning { animation: spin 1s linear infinite; }
```

### Complete Node SVG Structure

```javascript
function renderNode(selection) {
  const g = selection.append('g')
    .attr('class', 'dag-node')
    .attr('id', d => `node-${d.id}`)
    .attr('transform', d => `translate(${d.x}, ${d.y})`)
    .style('cursor', 'pointer');

  // Background rectangle (colored by verb type)
  g.append('rect')
    .attr('class', 'node-bg')
    .attr('width', d => d.width)
    .attr('height', d => d.height)
    .attr('rx', 8)
    .attr('fill', d => VERB_COLORS[d.verb] || '#6b7280')
    .attr('stroke', '#374151')
    .attr('stroke-width', 2);

  // Verb type badge (top-left)
  g.append('text')
    .attr('class', 'verb-badge')
    .attr('x', 8)
    .attr('y', 16)
    .attr('font-size', '10px')
    .attr('fill', 'rgba(255,255,255,0.7)')
    .text(d => d.verb);

  // Task ID label (center)
  g.append('text')
    .attr('class', 'node-label')
    .attr('x', d => d.width / 2)
    .attr('y', d => d.height / 2 + 4)
    .attr('text-anchor', 'middle')
    .attr('font-size', '13px')
    .attr('font-weight', '600')
    .attr('fill', 'white')
    .text(d => d.id);

  // Status indicator (bottom-right)
  g.append('g')
    .attr('class', 'status-icon')
    .attr('transform', d => `translate(${d.width - 20}, ${d.height - 18})`)
    .append('circle')
    .attr('r', 6)
    .attr('fill', '#6b7280'); // pending by default

  // Click handler
  g.on('click', function (event, d) {
    event.stopPropagation();
    vscode.postMessage({
      type: 'nodeClick',
      nodeId: d.id,
      filePath: d.sourceFile,
      line: d.sourceLine,
    });
  });
}
```

---

## 8. Recommended Architecture for Nika VS Code Extension DAG View

### Project Structure

```
nika-vscode/
  src/
    extension/
      extension.ts          -- activate(), commands
      dagPanel.ts           -- WebviewPanel lifecycle
      dagDataProvider.ts    -- Parse .nika.yaml -> DAG data
      executionWatcher.ts   -- Watch nika run output
    webview/
      dag.ts                -- Entry: D3.js + ELK.js rendering
      layout.ts             -- ELK layout computation
      renderer.ts           -- D3 SVG node/edge rendering
      statusUpdater.ts      -- Handle real-time status changes
      theme.ts              -- VS Code theme integration
      dag.css               -- Styles (verb colors, animations)
  dist/
    extension/              -- esbuild Node.js bundle
    webview/                -- esbuild browser bundle
  media/
    icons/                  -- Verb type icons (SVG)
  esbuild.mjs              -- Dual build config
```

### Build Configuration (esbuild)

```javascript
// esbuild.mjs
import * as esbuild from 'esbuild';

// Extension (Node.js)
await esbuild.build({
  entryPoints: ['src/extension/extension.ts'],
  bundle: true,
  outfile: 'dist/extension/extension.js',
  external: ['vscode'],
  format: 'cjs',
  platform: 'node',
  sourcemap: true,
  minify: process.env.NODE_ENV === 'production',
});

// Webview (Browser)
await esbuild.build({
  entryPoints: ['src/webview/dag.ts'],
  bundle: true,
  outfile: 'dist/webview/dag.js',
  format: 'esm',
  platform: 'browser',
  sourcemap: true,
  minify: process.env.NODE_ENV === 'production',
  define: { global: 'globalThis' },
  // D3 + ELK bundled into this single file
});
```

### Data Flow

```
.nika.yaml file
  |
  | (parse YAML, extract tasks + deps + verb types + line numbers)
  v
dagDataProvider.ts --> DagNode[] + DagEdge[]
  |
  | postMessage({ type: 'dagData', ... })
  v
webview/dag.ts
  |
  | ELK.js layout() -> compute x,y positions
  | D3.js render() -> SVG nodes + edges
  v
Interactive SVG in webview
  |
  | User clicks node
  | postMessage({ type: 'nodeClick', ... })
  v
dagPanel.ts -> vscode.window.showTextDocument(uri, { selection: range })
```

### Technology Choices Summary

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Layout | ELK.js (layered) | Best Sugiyama for pipeline DAGs, orthogonal edges |
| Rendering | D3.js (SVG) | Full control, <50 nodes typical, crisp text |
| Framework | None (vanilla TS) | Smallest bundle, D3 handles updates |
| Bundler | esbuild | Fast, dual-target, tree-shaking |
| Communication | postMessage | Only option in VS Code webviews |
| State | getState/setState | Persist zoom/pan across tab switches |
| Security | CSP + nonce | VS Code best practice |

---

## Sources

1. VS Code Webview API -- https://code.visualstudio.com/api/extension-guides/webview
2. ELK.js -- https://github.com/kieler/elkjs
3. D3.js -- https://d3js.org
4. d3-dag -- https://github.com/erikbrinkman/d3-dag
5. @dagrejs/dagre -- https://github.com/dagrejs/dagre (maintained fork)
6. React Flow (@xyflow/react) -- https://reactflow.dev
7. vscode-interactive-graphviz -- https://github.com/tintinweb/vscode-interactive-graphviz
8. CodeGraphy -- D3.js force graph in VS Code webview
9. hediet/vscode-drawio -- https://github.com/hediet/vscode-drawio
10. @vscode/webview-ui-toolkit -- https://github.com/microsoft/vscode-webview-ui-toolkit
11. VS Code webview-sample -- https://github.com/microsoft/vscode-extension-samples/tree/main/webview-sample
12. d3-hwschematic (ELK + D3) -- https://github.com/Nic30/d3-hwschematic
13. Matt Bierner's webview learnings -- https://blog.mattbierner.com/vscode-webview-web-learnings/

## Confidence Level

**High** -- Based on official VS Code documentation, active open-source projects, and established library ecosystems. The postMessage API, CSP patterns, and D3/ELK rendering approaches are well-documented and battle-tested in production extensions.

## Further Research Suggestions

- Benchmark ELK.js vs d3-dag Sugiyama layout for Nika's typical DAG shapes (linear chains with fan-out/fan-in)
- Evaluate VS Code theme variable CSS integration (`--vscode-editor-background` etc.) for dark/light mode
- Prototype YAML source mapping: test the `yaml` npm library's `range` tracking for accurate line numbers
- Investigate `nika check --json` or `nika graph --json` for pre-computed DAG data with source locations
- Consider WebviewView (sidebar panel) vs WebviewPanel (editor tab) for the DAG view UX
