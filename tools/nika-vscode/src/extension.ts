// extension.ts — VS Code extension entry point
//
// Registers the DAG panel command and wires up lifecycle.

import * as vscode from 'vscode';
import { DagPanel, DagPanelSerializer } from './dagPanel';
import type { DagGraph } from './dagPanel';

let dagPanel: DagPanel | undefined;

export function activate(context: vscode.ExtensionContext): void {
  // Register the DAG panel serializer for persistence across restarts
  context.subscriptions.push(
    vscode.window.registerWebviewPanelSerializer(
      DagPanel.viewType,
      new DagPanelSerializer(context.extensionUri),
    ),
  );

  // Command: Show DAG
  context.subscriptions.push(
    vscode.commands.registerCommand('nika.showDag', () => {
      if (!dagPanel) {
        dagPanel = new DagPanel(
          context.extensionUri,
          // onNodeClicked — open the workflow file and jump to task
          (taskId) => {
            vscode.window.showInformationMessage(`Task: ${taskId}`);
          },
          // onNodeDoubleClicked — open task output
          (taskId) => {
            vscode.commands.executeCommand('nika.showTaskOutput', taskId);
          },
        );
      }

      // Demo graph for testing — replace with real workflow parsing
      const demoGraph: DagGraph = {
        workflowName: 'research-and-summarize',
        nodes: [
          { id: 'research', label: 'research', verb: 'infer', status: 'success', durationMs: 2340, provider: 'anthropic', model: 'claude-sonnet-4-6', dependsOn: [] },
          { id: 'scrape', label: 'scrape', verb: 'fetch', status: 'success', durationMs: 890, dependsOn: ['research'] },
          { id: 'transform', label: 'transform', verb: 'invoke', status: 'running', dependsOn: ['scrape'] },
          { id: 'summarize', label: 'summarize', verb: 'infer', status: 'pending', provider: 'anthropic', dependsOn: ['transform'] },
          { id: 'format', label: 'format', verb: 'exec', status: 'pending', dependsOn: ['summarize'] },
          { id: 'notify', label: 'notify', verb: 'fetch', status: 'pending', dependsOn: ['format'] },
        ],
        edges: [
          { id: 'e1', source: 'research', target: 'scrape', isDataEdge: true },
          { id: 'e2', source: 'scrape', target: 'transform', isDataEdge: true },
          { id: 'e3', source: 'transform', target: 'summarize', isDataEdge: true },
          { id: 'e4', source: 'summarize', target: 'format', isDataEdge: true },
          { id: 'e5', source: 'format', target: 'notify', isDataEdge: false },
        ],
      };

      dagPanel.show(demoGraph);
    }),
  );
}

export function deactivate(): void {
  dagPanel?.dispose();
  dagPanel = undefined;
}
