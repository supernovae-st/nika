// dagPanel.ts — WebviewPanel provider for DAG visualization
//
// Manages the VS Code webview panel lifecycle: creation, CSP, nonce generation,
// asset URIs, postMessage typed protocol, state serialization/restoration.

import * as vscode from 'vscode';
import * as crypto from 'crypto';

// ─── Typed Message Protocol ──────────────────────────────────────────────────
// Discriminated union: every message has a `kind` field.
// Extension -> Webview messages:

export type TaskStatus = 'pending' | 'running' | 'success' | 'failed' | 'skipped';

export interface DagNode {
  id: string;
  label: string;
  verb: 'infer' | 'exec' | 'fetch' | 'invoke' | 'agent';
  status: TaskStatus;
  /** Duration in milliseconds, if completed */
  durationMs?: number;
  /** Provider name (for infer/agent) */
  provider?: string;
  /** Model name (for infer/agent) */
  model?: string;
  /** Upstream dependency IDs */
  dependsOn: string[];
}

export interface DagEdge {
  id: string;
  source: string;
  target: string;
  /** True if this edge carries data (with: binding), false if ordering only (depends_on) */
  isDataEdge: boolean;
}

export interface DagGraph {
  workflowName: string;
  nodes: DagNode[];
  edges: DagEdge[];
}

// Extension -> Webview
export type ExtToWebviewMessage =
  | { kind: 'dag:load'; graph: DagGraph }
  | { kind: 'dag:updateStatus'; taskId: string; status: TaskStatus; durationMs?: number }
  | { kind: 'dag:batchUpdateStatus'; updates: Array<{ taskId: string; status: TaskStatus; durationMs?: number }> }
  | { kind: 'dag:clear' }
  | { kind: 'dag:fitToView' }
  | { kind: 'theme:changed' };

// Webview -> Extension
export type WebviewToExtMessage =
  | { kind: 'dag:ready' }
  | { kind: 'dag:nodeClicked'; taskId: string }
  | { kind: 'dag:nodeDoubleClicked'; taskId: string }
  | { kind: 'dag:requestRefresh' }
  | { kind: 'dag:viewportChanged'; zoom: number; panX: number; panY: number };

// ─── Nonce Generator ─────────────────────────────────────────────────────────
function getNonce(): string {
  return crypto.randomBytes(16).toString('base64');
}

// ─── DAG Panel ───────────────────────────────────────────────────────────────
export class DagPanel implements vscode.Disposable {
  public static readonly viewType = 'nika.dagView';

  private panel: vscode.WebviewPanel | undefined;
  private disposables: vscode.Disposable[] = [];
  private currentGraph: DagGraph | undefined;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly onNodeClicked?: (taskId: string) => void,
    private readonly onNodeDoubleClicked?: (taskId: string) => void,
  ) {}

  // ─── Public API ──────────────────────────────────────────────────────────

  /** Show the DAG panel (create if needed, reveal if exists) */
  public show(graph?: DagGraph): void {
    if (this.panel) {
      this.panel.reveal(vscode.ViewColumn.Beside);
      if (graph) {
        this.loadGraph(graph);
      }
      return;
    }

    this.panel = vscode.window.createWebviewPanel(
      DagPanel.viewType,
      'Nika DAG',
      { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
      {
        enableScripts: true,
        retainContextWhenHidden: true, // Keep state when panel is backgrounded
        localResourceRoots: [
          vscode.Uri.joinPath(this.extensionUri, 'dist', 'webview'),
        ],
      },
    );

    this.panel.iconPath = {
      light: vscode.Uri.joinPath(this.extensionUri, 'media', 'dag-light.svg'),
      dark: vscode.Uri.joinPath(this.extensionUri, 'media', 'dag-dark.svg'),
    };

    this.panel.webview.html = this.getHtml(this.panel.webview);

    // Listen for messages from the webview
    this.panel.webview.onDidReceiveMessage(
      (msg: WebviewToExtMessage) => this.handleMessage(msg),
      null,
      this.disposables,
    );

    // Clean up on dispose
    this.panel.onDidDispose(
      () => {
        this.panel = undefined;
        this.disposables.forEach((d) => d.dispose());
        this.disposables = [];
      },
      null,
      this.disposables,
    );

    // Re-send graph when panel becomes visible again
    this.panel.onDidChangeViewState(
      (e) => {
        if (e.webviewPanel.visible && this.currentGraph) {
          this.postMessage({ kind: 'dag:load', graph: this.currentGraph });
        }
      },
      null,
      this.disposables,
    );

    // React to theme changes
    this.disposables.push(
      vscode.window.onDidChangeActiveColorTheme(() => {
        this.postMessage({ kind: 'theme:changed' });
      }),
    );

    if (graph) {
      this.currentGraph = graph;
      // Wait for webview 'dag:ready' before sending — see handleMessage
    }
  }

  /** Load a new graph (replaces current) */
  public loadGraph(graph: DagGraph): void {
    this.currentGraph = graph;
    if (this.panel?.visible) {
      this.postMessage({ kind: 'dag:load', graph });
    }
  }

  /** Update a single task's status (during execution) */
  public updateTaskStatus(taskId: string, status: TaskStatus, durationMs?: number): void {
    if (this.currentGraph) {
      const node = this.currentGraph.nodes.find((n) => n.id === taskId);
      if (node) {
        node.status = status;
        node.durationMs = durationMs;
      }
    }
    this.postMessage({ kind: 'dag:updateStatus', taskId, status, durationMs });
  }

  /** Batch update multiple task statuses at once */
  public batchUpdateStatus(updates: Array<{ taskId: string; status: TaskStatus; durationMs?: number }>): void {
    if (this.currentGraph) {
      for (const u of updates) {
        const node = this.currentGraph.nodes.find((n) => n.id === u.taskId);
        if (node) {
          node.status = u.status;
          node.durationMs = u.durationMs;
        }
      }
    }
    this.postMessage({ kind: 'dag:batchUpdateStatus', updates });
  }

  /** Fit the view to show all nodes */
  public fitToView(): void {
    this.postMessage({ kind: 'dag:fitToView' });
  }

  /** Clear the DAG display */
  public clear(): void {
    this.currentGraph = undefined;
    this.postMessage({ kind: 'dag:clear' });
  }

  public dispose(): void {
    this.panel?.dispose();
  }

  // ─── Internals ───────────────────────────────────────────────────────────

  private postMessage(msg: ExtToWebviewMessage): void {
    this.panel?.webview.postMessage(msg);
  }

  private handleMessage(msg: WebviewToExtMessage): void {
    switch (msg.kind) {
      case 'dag:ready':
        // Webview has initialized — send the graph if we have one
        if (this.currentGraph) {
          this.postMessage({ kind: 'dag:load', graph: this.currentGraph });
        }
        break;

      case 'dag:nodeClicked':
        this.onNodeClicked?.(msg.taskId);
        break;

      case 'dag:nodeDoubleClicked':
        this.onNodeDoubleClicked?.(msg.taskId);
        break;

      case 'dag:requestRefresh':
        // Extension can re-parse workflow and send updated graph
        if (this.currentGraph) {
          this.postMessage({ kind: 'dag:load', graph: this.currentGraph });
        }
        break;

      case 'dag:viewportChanged':
        // Could persist viewport state here if needed
        break;
    }
  }

  // ─── HTML Generation ─────────────────────────────────────────────────────

  private getHtml(webview: vscode.Webview): string {
    const nonce = getNonce();

    // Resolve webview-safe URIs for our bundled assets
    const scriptUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this.extensionUri, 'dist', 'webview', 'dag.js'),
    );
    const styleUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this.extensionUri, 'dist', 'webview', 'dag.css'),
    );

    // CSP: lock down everything except what we explicitly need.
    // - default-src 'none'          — block all by default
    // - style-src ${cspSource}      — allow our bundled CSS + VS Code theme vars
    //          'unsafe-inline'       — needed for D3 inline styles (transform, etc.)
    // - script-src 'nonce-${nonce}' — only our script with the nonce can execute
    // - img-src ${cspSource} data:  — allow our assets + inline SVG data URIs
    // - font-src ${cspSource}       — allow VS Code's codicon font
    const csp = [
      `default-src 'none'`,
      `style-src ${webview.cspSource} 'unsafe-inline'`,
      `script-src 'nonce-${nonce}'`,
      `img-src ${webview.cspSource} data:`,
      `font-src ${webview.cspSource}`,
    ].join('; ');

    return /* html */ `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="${csp}">
  <link rel="stylesheet" href="${styleUri}">
  <title>Nika DAG</title>
</head>
<body>
  <div id="dag-toolbar">
    <button id="btn-fit" title="Fit to view (F)">Fit</button>
    <button id="btn-zoom-in" title="Zoom in (+)">+</button>
    <button id="btn-zoom-out" title="Zoom out (-)">-</button>
    <span id="dag-title"></span>
    <span id="dag-status"></span>
  </div>
  <div id="dag-container"></div>
  <script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
  }
}

// ─── Serializer for webview persistence across VS Code restarts ─────────────
// Register with: vscode.window.registerWebviewPanelSerializer(DagPanel.viewType, ...)
export class DagPanelSerializer implements vscode.WebviewPanelSerializer {
  constructor(
    private readonly extensionUri: vscode.Uri,
  ) {}

  async deserializeWebviewPanel(panel: vscode.WebviewPanel, _state: unknown): Promise<void> {
    // Restore the HTML — the webview script will call getState() to restore
    // its own zoom/pan state and the last graph.
    const nonce = getNonce();

    const scriptUri = panel.webview.asWebviewUri(
      vscode.Uri.joinPath(this.extensionUri, 'dist', 'webview', 'dag.js'),
    );
    const styleUri = panel.webview.asWebviewUri(
      vscode.Uri.joinPath(this.extensionUri, 'dist', 'webview', 'dag.css'),
    );

    const csp = [
      `default-src 'none'`,
      `style-src ${panel.webview.cspSource} 'unsafe-inline'`,
      `script-src 'nonce-${nonce}'`,
      `img-src ${panel.webview.cspSource} data:`,
      `font-src ${panel.webview.cspSource}`,
    ].join('; ');

    panel.webview.html = /* html */ `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="${csp}">
  <link rel="stylesheet" href="${styleUri}">
  <title>Nika DAG</title>
</head>
<body>
  <div id="dag-toolbar">
    <button id="btn-fit" title="Fit to view (F)">Fit</button>
    <button id="btn-zoom-in" title="Zoom in (+)">+</button>
    <button id="btn-zoom-out" title="Zoom out (-)">-</button>
    <span id="dag-title"></span>
    <span id="dag-status"></span>
  </div>
  <div id="dag-container"></div>
  <script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
  }
}
