import {
  workspace,
  commands,
  ExtensionContext,
  window,
  Uri,
  Position,
  Range,
  env,
  StatusBarAlignment,
} from 'vscode';
import { execFile } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { WorkflowTreeProvider } from './workflowTree';
import { DagPanel, DagGraph } from './dagPanel';
import {
  getArtifactName,
  downloadNikaBinary,
  isBinaryWorking,
  findBundledBinary,
  GITHUB_INSTALL_URL,
} from './binaryInstaller';
import {
  startClient,
  getNikaPath,
  runNikaCommand,
  type ClientState,
  type LogFn,
} from './lspClient';

// ─── Shared mutable state ──────────────────────────────────────────────────
// Owned here, passed by reference to module functions via ClientState.
const state: ClientState = {
  client: undefined,
  statusBarItem: undefined,
  statusPollInterval: undefined,
  activeDagPanel: undefined,
  resolvedServerPath: undefined,
};

let outputChannel: import('vscode').OutputChannel | undefined;

function log(level: string, msg: string): void {
  if (outputChannel) {
    outputChannel.appendLine(`[${new Date().toISOString()}] [${level}] ${msg}`);
  }
}

// ─── Activation ─────────────────────────────────────────────────────────────

export function activate(context: ExtensionContext): void {
  // Output channel for structured logging
  outputChannel = window.createOutputChannel('Nika Language Server');
  context.subscriptions.push(outputChannel);

  // Status bar item
  state.statusBarItem = window.createStatusBarItem(StatusBarAlignment.Left, 100);
  state.statusBarItem.command = 'nika.showOutput';
  state.statusBarItem.text = '$(zap) Nika: Starting...';
  state.statusBarItem.tooltip = 'Nika Language Server';
  state.statusBarItem.show();
  context.subscriptions.push(state.statusBarItem);

  // Command: Show output channel
  context.subscriptions.push(
    commands.registerCommand('nika.showOutput', () => {
      outputChannel?.show();
    }),
  );

  log('INFO', `Nika extension v${context.extension.packageJSON.version} activating`);
  log('INFO', `Platform: ${process.platform}/${process.arch}`);

  // Sidebar tree view — workflow explorer
  const workflowTree = new WorkflowTreeProvider();
  context.subscriptions.push(window.registerTreeDataProvider('nikaWorkflows', workflowTree));
  const watcher = workspace.createFileSystemWatcher('**/*.nika.yaml');
  watcher.onDidCreate(() => workflowTree.refresh());
  watcher.onDidDelete(() => workflowTree.refresh());
  watcher.onDidChange(() => workflowTree.refresh());
  context.subscriptions.push(watcher);

  // DAG webview panel — track the active workflow URI for node-click navigation
  let dagWorkflowUri: Uri | undefined;

  const dagPanel = new DagPanel(
    context.extensionUri,
    (taskId) => {
      // On node click: jump to the task's line in the editor
      if (!dagWorkflowUri) { return; }
      if (state.client?.isRunning()) {
        state.client.sendRequest<{ nodes: Array<{ id: string; line: number }> }>(
          'nika/workflowGraph',
          { uri: dagWorkflowUri.toString() },
        ).then((graph) => {
          const node = graph.nodes.find((n) => n.id === taskId);
          if (node) {
            const pos = new Position(node.line, 0);
            window.showTextDocument(dagWorkflowUri!, {
              selection: new Range(pos, pos),
              preview: false,
            });
          }
        }).catch(() => {});
      }
    },
  );
  state.activeDagPanel = dagPanel;
  context.subscriptions.push(dagPanel);

  // Register all commands SYNCHRONOUSLY before any async work.

  // Command: Jump to task location (used by tree view and DAG panel)
  context.subscriptions.push(
    commands.registerCommand('nika.openTaskLocation', (uri: Uri, line: number) => {
      const pos = new Position(line, 0);
      window.showTextDocument(uri, { selection: new Range(pos, pos), preview: false });
    }),
  );

  // Command: Run current workflow
  context.subscriptions.push(
    commands.registerCommand('nika.runWorkflow', (uri?: Uri) => {
      const filePath = uri?.fsPath ?? window.activeTextEditor?.document.fileName;
      if (!filePath?.endsWith('.nika.yaml')) {
        window.showWarningMessage('Open a .nika.yaml file first.');
        return;
      }
      runNikaCommand(state.resolvedServerPath, 'run', filePath);
    }),
  );

  // Command: Validate current workflow
  context.subscriptions.push(
    commands.registerCommand('nika.checkWorkflow', (uri?: Uri) => {
      const filePath = uri?.fsPath ?? window.activeTextEditor?.document.fileName;
      if (!filePath?.endsWith('.nika.yaml')) {
        window.showWarningMessage('Open a .nika.yaml file first.');
        return;
      }
      runNikaCommand(state.resolvedServerPath, 'check', filePath);
    }),
  );

  // Command: New workflow from template
  context.subscriptions.push(
    commands.registerCommand('nika.newWorkflow', async () => {
      const name = await window.showInputBox({
        prompt: 'Workflow name (without extension)',
        placeHolder: 'my-workflow',
        validateInput: (v) => /^[a-z0-9-]+$/.test(v) ? null : 'Use lowercase letters, numbers, hyphens',
      });
      if (!name) { return; }

      const folder = workspace.workspaceFolders?.[0];
      if (!folder) {
        window.showErrorMessage('Open a folder first.');
        return;
      }

      const filePath = Uri.joinPath(folder.uri, `${name}.nika.yaml`);
      const content = Buffer.from(
        `schema: "nika/workflow@0.12"\nworkflow: ${name}\ndescription: ""\nprovider: anthropic\nmodel: claude-sonnet-4-20250514\n\ntasks:\n  - id: start\n    infer: ""\n`,
        'utf-8',
      );
      await workspace.fs.writeFile(filePath, content);
      const doc = await workspace.openTextDocument(filePath);
      await window.showTextDocument(doc);
    }),
  );

  // Command: Show tasks (focus outline view)
  context.subscriptions.push(
    commands.registerCommand('nika.showTasks', () => {
      commands.executeCommand('workbench.action.focusOutline');
    }),
  );

  // Command: Show DAG webview
  context.subscriptions.push(
    commands.registerCommand('nika.showDag', async (uri?: Uri) => {
      const filePath = uri?.fsPath ?? window.activeTextEditor?.document.fileName;
      if (!filePath?.endsWith('.nika.yaml')) {
        window.showWarningMessage('Open a .nika.yaml file first.');
        return;
      }
      // Track the workflow URI for node-click navigation
      dagWorkflowUri = Uri.file(filePath);

      // Try to get graph data from LSP, otherwise show empty panel
      let graph: DagGraph | undefined;
      if (state.client?.isRunning()) {
        try {
          graph = await state.client.sendRequest<DagGraph>('nika/workflowGraph', {
            uri: dagWorkflowUri.toString(),
          });
        } catch {
          log('WARN', 'LSP nika/workflowGraph not available, showing empty panel');
        }
      }
      dagPanel.show(graph);
    }),
  );

  // Command: Restart language server
  context.subscriptions.push(
    commands.registerCommand('nika.restartServer', async () => {
      if (state.client) {
        await state.client.stop();
        state.client = undefined;
      }
      startClient(context, state, log, state.resolvedServerPath);
      window.showInformationMessage('Nika language server restarted.');
    }),
  );

  // ─── Binary discovery & LSP start ──────────────────────────────────────────

  const configPath = getNikaPath();
  const autoDownload = workspace.getConfiguration('nika').get<boolean>('server.autoDownload', true);

  const storagePath = context.globalStorageUri.fsPath;
  const isWindows = process.platform === 'win32';
  const cachedBinary = path.join(storagePath, isWindows ? 'nika.exe' : 'nika');

  const tryStartWithBinary = (binaryPath: string): void => {
    state.resolvedServerPath = binaryPath;
    startClient(context, state, log, binaryPath);
  };

  const fallbackToWarning = (reason: string): void => {
    window.showWarningMessage(
      `Nika binary not found (${reason}). Install: cargo install nika`,
      'Open Install Guide',
    ).then((choice) => {
      if (choice === 'Open Install Guide') {
        env.openExternal(Uri.parse(GITHUB_INSTALL_URL));
      }
    });
    // Still attempt to start the client — it may fail gracefully if PATH is set later.
    startClient(context, state, log);
  };

  // Discovery priority: 1. explicit config → 2. bundled binary → 3. PATH → 4. cached → 5. download
  if (configPath !== 'nika') {
    // User set an explicit path — use it directly
    log('INFO', `Using configured binary: ${configPath}`);
    tryStartWithBinary(configPath);
    return;
  }

  const bundled = findBundledBinary(context);
  if (bundled) {
    log('INFO', `Using bundled binary: ${bundled}`);
    tryStartWithBinary(bundled);
    return;
  }

  execFile(configPath, ['--version'], { timeout: 5000 }, async (pathError) => {
    if (!pathError) {
      // Binary found in PATH — use it directly.
      state.resolvedServerPath = configPath;
      startClient(context, state, log);
      return;
    }

    // Binary not found via PATH. Check cached binary first.
    const cachedExists = fs.existsSync(cachedBinary);
    if (cachedExists) {
      const cachedWorks = await isBinaryWorking(cachedBinary);
      if (cachedWorks) {
        tryStartWithBinary(cachedBinary);
        return;
      }
      // Cached binary is broken — delete and re-download.
      fs.unlink(cachedBinary, () => undefined);
    }

    if (!autoDownload) {
      fallbackToWarning('auto-download disabled');
      return;
    }

    if (getArtifactName() === null) {
      fallbackToWarning(`unsupported platform: ${process.platform}/${process.arch}`);
      return;
    }

    try {
      const downloadedPath = await downloadNikaBinary(storagePath);
      if (downloadedPath && fs.existsSync(downloadedPath)) {
        const works = await isBinaryWorking(downloadedPath);
        if (works) {
          window.showInformationMessage('Nika language server downloaded successfully.');
          tryStartWithBinary(downloadedPath);
          return;
        }
      }
      fallbackToWarning('download succeeded but binary did not run');
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      fallbackToWarning(`download failed: ${message}`);
    }
  });
}

export function deactivate(): Thenable<void> | undefined {
  if (state.statusPollInterval !== undefined) {
    clearInterval(state.statusPollInterval);
    state.statusPollInterval = undefined;
  }
  if (!state.client) {
    return undefined;
  }
  return state.client.stop();
}
