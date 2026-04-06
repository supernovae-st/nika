// dag.ts — ELK.js layered layout + D3.js SVG rendering for Nika DAG visualization
//
// Runs inside VS Code webview (browser context). Bundled as IIFE by esbuild.
// elkjs/lib/elk.bundled.js is aliased by esbuild — no Web Worker needed.
//
// D3 imports are selective (d3-selection, d3-zoom, d3-shape, d3-transition)
// to keep the bundle under 700 KB instead of 1.5 MB with the full d3 package.

import ELK, {
  type ElkNode,
  type ElkExtendedEdge,
  type ElkPoint,
} from 'elkjs';

import { select, type Selection } from 'd3-selection';
import { zoom, zoomIdentity, type ZoomBehavior, type D3ZoomEvent } from 'd3-zoom';
import { line, curveMonotoneY, type Line } from 'd3-shape';
// Side-effect import: patches Selection.prototype with .transition()
import 'd3-transition';

// ─── Types (mirrored from dagPanel.ts — no shared import in webview) ────────

type TaskStatus = 'pending' | 'running' | 'success' | 'failed' | 'skipped';
type Verb = 'infer' | 'exec' | 'fetch' | 'invoke' | 'agent';

interface DagNode {
  id: string;
  label: string;
  verb: Verb;
  status: TaskStatus;
  durationMs?: number;
  provider?: string;
  model?: string;
  dependsOn: string[];
}

interface DagEdge {
  id: string;
  source: string;
  target: string;
  isDataEdge: boolean;
}

interface DagGraph {
  workflowName: string;
  nodes: DagNode[];
  edges: DagEdge[];
}

type ExtToWebviewMessage =
  | { kind: 'dag:load'; graph: DagGraph }
  | { kind: 'dag:updateStatus'; taskId: string; status: TaskStatus; durationMs?: number }
  | { kind: 'dag:batchUpdateStatus'; updates: Array<{ taskId: string; status: TaskStatus; durationMs?: number }> }
  | { kind: 'dag:clear' }
  | { kind: 'dag:fitToView' }
  | { kind: 'theme:changed' };

// ─── VS Code API ────────────────────────────────────────────────────────────

interface VsCodeApi {
  postMessage(msg: unknown): void;
  getState(): WebviewState | undefined;
  setState(state: WebviewState): void;
}

interface WebviewState {
  graph?: DagGraph;
  zoom?: number;
  panX?: number;
  panY?: number;
}

declare function acquireVsCodeApi(): VsCodeApi;
const vscode = acquireVsCodeApi();

// ─── Constants ──────────────────────────────────────────────────────────────

const NODE_WIDTH = 200;
const NODE_HEIGHT = 56;
const NODE_RADIUS = 8;
const PADDING = 40;

/** Verb -> icon (simple Unicode — no font dependency) */
const VERB_ICONS: Record<Verb, string> = {
  infer: '\u2728', // sparkles
  exec: '\u25B6',  // play triangle
  fetch: '\u2197',  // north-east arrow
  invoke: '\u2699', // gear
  agent: '\u267B',  // recycling (loop)
};

// ─── ELK Layout Engine ──────────────────────────────────────────────────────

const elk = new ELK();

/** Convert DagGraph into ELK input, run layout, return positioned result */
async function computeLayout(graph: DagGraph): Promise<ElkNode> {
  const elkGraph: ElkNode = {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'DOWN',
      'elk.spacing.nodeNode': '40',
      'elk.layered.spacing.nodeNodeBetweenLayers': '60',
      'elk.edgeRouting': 'ORTHOGONAL',
      'elk.layered.nodePlacement.strategy': 'NETWORK_SIMPLEX',
      'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
      'elk.layered.spacing.edgeNodeBetweenLayers': '20',
      'elk.layered.spacing.edgeEdgeBetweenLayers': '15',
      'elk.padding': `[top=${PADDING},left=${PADDING},bottom=${PADDING},right=${PADDING}]`,
      'elk.layered.nodePlacement.bk.fixedAlignment': 'BALANCED',
    },
    children: graph.nodes.map((node) => ({
      id: node.id,
      width: NODE_WIDTH,
      height: NODE_HEIGHT,
    })),
    edges: graph.edges.map((edge) => ({
      id: edge.id,
      sources: [edge.source],
      targets: [edge.target],
    })),
  };

  return elk.layout(elkGraph);
}

// ─── D3 SVG Renderer ────────────────────────────────────────────────────────

class DagRenderer {
  private svg: Selection<SVGSVGElement, unknown, null, undefined>;
  private rootGroup: Selection<SVGGElement, unknown, null, undefined>;
  private edgeGroup: Selection<SVGGElement, unknown, null, undefined>;
  private nodeGroup: Selection<SVGGElement, unknown, null, undefined>;
  private zoomBehavior: ZoomBehavior<SVGSVGElement, unknown>;
  private currentGraph: DagGraph | undefined;
  private nodeMap: Map<string, DagNode> = new Map();
  private container: HTMLElement;
  /** Use smooth curves instead of orthogonal segments for edges */
  public smoothEdges = false;

  constructor(containerId: string) {
    this.container = document.getElementById(containerId)!;

    this.svg = select(this.container)
      .append<SVGSVGElement>('svg')
      .attr('class', 'dag-svg')
      .attr('width', '100%')
      .attr('height', '100%');

    // SVG defs: arrowhead markers + glow filter
    const defs = this.svg.append('defs');
    this.createArrowMarkers(defs);
    this.createGlowFilter(defs);

    // Root group receives all zoom/pan transforms
    this.rootGroup = this.svg.append<SVGGElement>('g').attr('class', 'dag-root');
    // Edges below nodes
    this.edgeGroup = this.rootGroup.append<SVGGElement>('g').attr('class', 'dag-edges');
    // Nodes on top
    this.nodeGroup = this.rootGroup.append<SVGGElement>('g').attr('class', 'dag-nodes');

    // Zoom + pan
    this.zoomBehavior = zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.1, 4])
      .on('zoom', (event: D3ZoomEvent<SVGSVGElement, unknown>) => {
        this.rootGroup.attr('transform', event.transform.toString());
        this.saveState({
          zoom: event.transform.k,
          panX: event.transform.x,
          panY: event.transform.y,
        });
        vscode.postMessage({
          kind: 'dag:viewportChanged',
          zoom: event.transform.k,
          panX: event.transform.x,
          panY: event.transform.y,
        });
      });

    this.svg.call(this.zoomBehavior);

    // Restore saved viewport
    const savedState = vscode.getState();
    if (savedState?.zoom != null && savedState.panX != null && savedState.panY != null) {
      const t = zoomIdentity
        .translate(savedState.panX, savedState.panY)
        .scale(savedState.zoom);
      this.svg.call(this.zoomBehavior.transform, t);
    }
  }

  private createArrowMarkers(defs: Selection<SVGDefsElement, unknown, null, undefined>): void {
    // Data edge arrow (solid, blue)
    defs
      .append('marker')
      .attr('id', 'arrow-data')
      .attr('viewBox', '0 0 10 10')
      .attr('refX', 10)
      .attr('refY', 5)
      .attr('markerWidth', 8)
      .attr('markerHeight', 8)
      .attr('orient', 'auto-start-reverse')
      .append('path')
      .attr('d', 'M 0 0 L 10 5 L 0 10 z')
      .attr('class', 'arrow-data');

    // Dependency arrow (subtle, gray)
    defs
      .append('marker')
      .attr('id', 'arrow-dep')
      .attr('viewBox', '0 0 10 10')
      .attr('refX', 10)
      .attr('refY', 5)
      .attr('markerWidth', 6)
      .attr('markerHeight', 6)
      .attr('orient', 'auto-start-reverse')
      .append('path')
      .attr('d', 'M 0 0 L 10 5 L 0 10 z')
      .attr('class', 'arrow-dep');
  }

  private createGlowFilter(defs: Selection<SVGDefsElement, unknown, null, undefined>): void {
    const filter = defs
      .append('filter')
      .attr('id', 'glow-running')
      .attr('x', '-20%')
      .attr('y', '-20%')
      .attr('width', '140%')
      .attr('height', '140%');

    filter
      .append('feGaussianBlur')
      .attr('in', 'SourceGraphic')
      .attr('stdDeviation', '3')
      .attr('result', 'blur');

    const merge = filter.append('feMerge');
    merge.append('feMergeNode').attr('in', 'blur');
    merge.append('feMergeNode').attr('in', 'SourceGraphic');
  }

  // ─── Render Pipeline ────────────────────────────────────────────────────

  async render(graph: DagGraph): Promise<void> {
    this.currentGraph = graph;
    this.nodeMap.clear();
    graph.nodes.forEach((n) => this.nodeMap.set(n.id, n));

    this.saveState({ graph });

    const layoutResult = await computeLayout(graph);

    // Update toolbar
    const titleEl = document.getElementById('dag-title');
    if (titleEl) titleEl.textContent = graph.workflowName;

    // Render edges first (below nodes in SVG z-order)
    this.renderEdges(layoutResult.edges ?? [], graph.edges);

    // Render nodes
    this.renderNodes(layoutResult.children ?? [], graph.nodes);

    this.updateStatusDisplay();

    // Auto-fit on first render if no saved viewport
    const savedState = vscode.getState();
    if (!savedState?.zoom) {
      this.fitToView(layoutResult);
    }
  }

  private renderNodes(elkNodes: ElkNode[], dagNodes: DagNode[]): void {
    const elkMap = new Map(elkNodes.map((n) => [n.id, n]));

    // D3 data join keyed by task ID
    const groups = this.nodeGroup
      .selectAll<SVGGElement, DagNode>('g.dag-node')
      .data(dagNodes, (d) => d.id);

    // EXIT
    groups.exit()
      .transition().duration(200)
      .attr('opacity', 0)
      .remove();

    // ENTER
    const enter = groups
      .enter()
      .append('g')
      .attr('class', (d) => `dag-node status-${d.status} verb-${d.verb}`)
      .attr('data-id', (d) => d.id)
      .attr('opacity', 0);

    // Node background rect
    enter
      .append('rect')
      .attr('class', 'node-bg')
      .attr('width', NODE_WIDTH)
      .attr('height', NODE_HEIGHT)
      .attr('rx', NODE_RADIUS)
      .attr('ry', NODE_RADIUS);

    // Status indicator bar (left edge)
    enter
      .append('rect')
      .attr('class', 'node-status-bar')
      .attr('width', 4)
      .attr('height', NODE_HEIGHT - 8)
      .attr('x', 4)
      .attr('y', 4)
      .attr('rx', 2)
      .attr('ry', 2);

    // Verb icon
    enter
      .append('text')
      .attr('class', 'node-icon')
      .attr('x', 20)
      .attr('y', NODE_HEIGHT / 2 - 6)
      .attr('text-anchor', 'middle')
      .attr('dominant-baseline', 'central')
      .text((d) => VERB_ICONS[d.verb]);

    // Task ID label
    enter
      .append('text')
      .attr('class', 'node-label')
      .attr('x', 36)
      .attr('y', NODE_HEIGHT / 2 - 6)
      .attr('dominant-baseline', 'central')
      .text((d) => d.label);

    // Subtitle (verb + provider/duration)
    enter
      .append('text')
      .attr('class', 'node-subtitle')
      .attr('x', 36)
      .attr('y', NODE_HEIGHT / 2 + 12)
      .attr('dominant-baseline', 'central')
      .text((d) => this.getSubtitle(d));

    // Running spinner (animated via CSS)
    enter
      .append('circle')
      .attr('class', 'node-spinner')
      .attr('cx', NODE_WIDTH - 20)
      .attr('cy', NODE_HEIGHT / 2)
      .attr('r', 6);

    // Click handler
    enter.on('click', (_event: MouseEvent, d: DagNode) => {
      vscode.postMessage({ kind: 'dag:nodeClicked', taskId: d.id });
      this.nodeGroup.selectAll('.dag-node').classed('selected', false);
      this.nodeGroup.select(`[data-id="${d.id}"]`).classed('selected', true);
    });

    // Double-click handler
    enter.on('dblclick', (_event: MouseEvent, d: DagNode) => {
      vscode.postMessage({ kind: 'dag:nodeDoubleClicked', taskId: d.id });
    });

    // Native tooltip
    enter.append('title').text((d) => this.tooltipText(d));

    // MERGE: enter + update
    const merged = enter.merge(groups);

    // Animate into position
    merged
      .transition().duration(300)
      .attr('opacity', 1)
      .attr('transform', (d) => {
        const elk = elkMap.get(d.id);
        return elk ? `translate(${elk.x},${elk.y})` : '';
      });

    // Update classes
    merged.attr('class', (d) => `dag-node status-${d.status} verb-${d.verb}`);

    // Update dynamic text
    merged.select('.node-subtitle').text((d) => this.getSubtitle(d));
    merged.select('title').text((d) => this.tooltipText(d));
  }

  private renderEdges(elkEdges: ElkExtendedEdge[], dagEdges: DagEdge[]): void {
    const dagEdgeMap = new Map(dagEdges.map((e) => [e.id, e]));

    const paths = this.edgeGroup
      .selectAll<SVGPathElement, ElkExtendedEdge>('path.dag-edge')
      .data(elkEdges, (d) => d.id);

    paths.exit()
      .transition().duration(200)
      .attr('opacity', 0)
      .remove();

    const enter = paths
      .enter()
      .append('path')
      .attr('class', (d) => {
        const meta = dagEdgeMap.get(d.id);
        return `dag-edge ${meta?.isDataEdge ? 'edge-data' : 'edge-dep'}`;
      })
      .attr('marker-end', (d) => {
        const meta = dagEdgeMap.get(d.id);
        return `url(#arrow-${meta?.isDataEdge ? 'data' : 'dep'})`;
      })
      .attr('opacity', 0);

    enter
      .merge(paths)
      .transition().duration(300)
      .attr('opacity', 1)
      .attr('d', (d) => this.smoothEdges ? this.edgePathSmooth(d) : this.edgePath(d));
  }

  /** Convert ELK edge sections into an SVG path string.
   *  Uses straight segments for orthogonal routing. */
  private edgePath(edge: ElkExtendedEdge): string {
    if (!edge.sections?.length) return '';

    const parts: string[] = [];
    for (const section of edge.sections) {
      parts.push(`M ${section.startPoint.x} ${section.startPoint.y}`);
      if (section.bendPoints?.length) {
        for (const bp of section.bendPoints) {
          parts.push(`L ${bp.x} ${bp.y}`);
        }
      }
      parts.push(`L ${section.endPoint.x} ${section.endPoint.y}`);
    }
    return parts.join(' ');
  }

  /** Alternative: smooth curves using d3-shape curveMonotoneY.
   *  Call this instead of edgePath() for a softer look. */
  private edgePathSmooth(edge: ElkExtendedEdge): string {
    if (!edge.sections?.length) return '';

    const points: ElkPoint[] = [];
    for (const section of edge.sections) {
      points.push(section.startPoint);
      if (section.bendPoints) points.push(...section.bendPoints);
      points.push(section.endPoint);
    }
    if (points.length < 2) return '';

    const lineGen: Line<ElkPoint> = line<ElkPoint>()
      .x((p) => p.x)
      .y((p) => p.y)
      .curve(curveMonotoneY);

    return lineGen(points) ?? '';
  }

  // ─── Status Updates ──────────────────────────────────────────────────────

  updateNodeStatus(taskId: string, status: TaskStatus, durationMs?: number): void {
    const node = this.nodeMap.get(taskId);
    if (!node) return;

    node.status = status;
    if (durationMs != null) node.durationMs = durationMs;

    const el = this.nodeGroup.select(`[data-id="${taskId}"]`);
    el.attr('class', `dag-node status-${status} verb-${node.verb}`);
    el.select('.node-subtitle').text(this.getSubtitle(node));
    el.select('title').text(this.tooltipText(node));

    this.saveState({ graph: this.currentGraph });
    this.updateStatusDisplay();
  }

  batchUpdateStatus(updates: Array<{ taskId: string; status: TaskStatus; durationMs?: number }>): void {
    for (const u of updates) {
      this.updateNodeStatus(u.taskId, u.status, u.durationMs);
    }
  }

  // ─── Viewport Controls ───────────────────────────────────────────────────

  fitToView(elkResult?: ElkNode): void {
    const svgEl = this.svg.node();
    if (!svgEl) return;

    const { width: svgW, height: svgH } = svgEl.getBoundingClientRect();
    if (svgW === 0 || svgH === 0) return;

    let graphW: number;
    let graphH: number;

    if (elkResult) {
      graphW = (elkResult.width ?? svgW) + PADDING * 2;
      graphH = (elkResult.height ?? svgH) + PADDING * 2;
    } else {
      const rootNode = this.rootGroup.node();
      if (!rootNode) return;
      const bbox = rootNode.getBBox();
      graphW = bbox.width + PADDING * 2;
      graphH = bbox.height + PADDING * 2;
    }

    const scale = Math.min(svgW / graphW, svgH / graphH, 1.5);
    const tx = (svgW - graphW * scale) / 2;
    const ty = (svgH - graphH * scale) / 2;

    const t = zoomIdentity.translate(tx, ty).scale(scale);
    this.svg
      .transition().duration(500)
      .call(this.zoomBehavior.transform as any, t);
  }

  zoomIn(): void {
    this.svg
      .transition().duration(300)
      .call(this.zoomBehavior.scaleBy as any, 1.3);
  }

  zoomOut(): void {
    this.svg
      .transition().duration(300)
      .call(this.zoomBehavior.scaleBy as any, 0.7);
  }

  clear(): void {
    this.edgeGroup.selectAll('*').remove();
    this.nodeGroup.selectAll('*').remove();
    this.currentGraph = undefined;
    this.nodeMap.clear();

    const titleEl = document.getElementById('dag-title');
    if (titleEl) titleEl.textContent = '';
    const statusEl = document.getElementById('dag-status');
    if (statusEl) statusEl.textContent = '';

    vscode.setState(undefined as unknown as WebviewState);
  }

  // ─── Helpers ─────────────────────────────────────────────────────────────

  private getSubtitle(node: DagNode): string {
    if (node.status === 'running') return `${node.verb} ...`;
    if (node.durationMs != null) {
      const dur = node.durationMs >= 1000
        ? `${(node.durationMs / 1000).toFixed(1)}s`
        : `${node.durationMs}ms`;
      return `${node.verb} \u00B7 ${dur}`;
    }
    if (node.provider) return `${node.verb} \u00B7 ${node.provider}`;
    return node.verb;
  }

  private tooltipText(node: DagNode): string {
    const lines = [`${node.id}`, `Verb: ${node.verb}`, `Status: ${node.status}`];
    if (node.provider) lines.push(`Provider: ${node.provider}`);
    if (node.model) lines.push(`Model: ${node.model}`);
    if (node.durationMs != null) lines.push(`Duration: ${node.durationMs}ms`);
    return lines.join('\n');
  }

  private updateStatusDisplay(): void {
    if (!this.currentGraph) return;

    const counts: Record<TaskStatus, number> = {
      pending: 0, running: 0, success: 0, failed: 0, skipped: 0,
    };
    for (const node of this.currentGraph.nodes) {
      counts[node.status]++;
    }

    const parts: string[] = [];
    if (counts.success > 0) parts.push(`${counts.success} done`);
    if (counts.running > 0) parts.push(`${counts.running} running`);
    if (counts.failed > 0) parts.push(`${counts.failed} failed`);
    if (counts.pending > 0) parts.push(`${counts.pending} pending`);
    if (counts.skipped > 0) parts.push(`${counts.skipped} skipped`);

    const statusEl = document.getElementById('dag-status');
    if (statusEl) statusEl.textContent = parts.join(' \u00B7 ');
  }

  private saveState(partial: Partial<WebviewState>): void {
    const current = vscode.getState() ?? {};
    vscode.setState({ ...current, ...partial });
  }
}

// ─── Initialize ─────────────────────────────────────────────────────────────

const renderer = new DagRenderer('dag-container');

// Restore from webview state (e.g., after being hidden and re-shown)
const savedState = vscode.getState();
if (savedState?.graph) {
  renderer.render(savedState.graph);
}

// ─── Message Handler ────────────────────────────────────────────────────────

window.addEventListener('message', (event: MessageEvent<ExtToWebviewMessage>) => {
  const msg = event.data;
  switch (msg.kind) {
    case 'dag:load':
      renderer.render(msg.graph);
      break;
    case 'dag:updateStatus':
      renderer.updateNodeStatus(msg.taskId, msg.status, msg.durationMs);
      break;
    case 'dag:batchUpdateStatus':
      renderer.batchUpdateStatus(msg.updates);
      break;
    case 'dag:clear':
      renderer.clear();
      break;
    case 'dag:fitToView':
      renderer.fitToView();
      break;
    case 'theme:changed':
      // CSS variables update automatically — nothing to do
      break;
  }
});

// ─── Toolbar Handlers ───────────────────────────────────────────────────────

document.getElementById('btn-fit')?.addEventListener('click', () => renderer.fitToView());
document.getElementById('btn-zoom-in')?.addEventListener('click', () => renderer.zoomIn());
document.getElementById('btn-zoom-out')?.addEventListener('click', () => renderer.zoomOut());

// Keyboard shortcuts
document.addEventListener('keydown', (e: KeyboardEvent) => {
  // Only handle if not in an input field
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
  if (e.key === 'f' || e.key === 'F') renderer.fitToView();
  if (e.key === '+' || e.key === '=') renderer.zoomIn();
  if (e.key === '-') renderer.zoomOut();
});

// ─── Signal Ready ───────────────────────────────────────────────────────────
vscode.postMessage({ kind: 'dag:ready' });
