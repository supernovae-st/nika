// lspClient.ts — LSP client lifecycle management
//
// Creates, starts, and manages the Nika language server connection.
// State is owned by extension.ts and passed via the ClientState interface.

import {
  workspace,
  ExtensionContext,
  window,
  commands,
  env,
  Uri,
  Position,
  Range,
} from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';
import { execFile } from 'child_process';
import { DagPanel } from './dagPanel';
import {
  isCursor,
  isWindsurf,
  ensureCursorMcpConfig,
  ensureCursorRules,
  ensureVscodeMcpConfig,
  ensureWindsurfMcpConfig,
  type LogFn,
} from './mcpConfig';

export type { LogFn } from './mcpConfig';

/** Shared mutable state owned by extension.ts, passed by reference. */
export interface ClientState {
  client: LanguageClient | undefined;
  statusBarItem: import('vscode').StatusBarItem | undefined;
  statusPollInterval: ReturnType<typeof setInterval> | undefined;
  activeDagPanel: DagPanel | undefined;
  resolvedServerPath: string | undefined;
}

export function getNikaPath(): string {
  return workspace.getConfiguration('nika').get<string>('server.path', 'nika');
}

export function runNikaCommand(resolvedServerPath: string | undefined, subcmd: string, filePath: string): void {
  const nika = resolvedServerPath ?? getNikaPath();
  const escaped = filePath.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  const terminal = window.createTerminal({ name: `Nika: ${subcmd}` });
  terminal.show();
  terminal.sendText(`"${nika}" ${subcmd} "${escaped}"`);
}

/** Compare extension version with LSP server version and warn on mismatch. */
export function checkVersionMismatch(context: ExtensionContext, log: LogFn): void {
  const extVersion = context.extension.packageJSON.version as string;
  const serverPath = getNikaPath();

  execFile(serverPath, ['--version'], { timeout: 5000 }, (error, stdout) => {
    if (error) { return; }
    // Output format: "nika 0.58.0" or "0.58.0-dev (abc1234, built 2h ago)"
    const match = stdout.match(/(\d+\.\d+)\.\d+/);
    if (!match) { return; }

    const serverMajorMinor = match[1]; // e.g. "0.58"
    const extMatch = extVersion.match(/(\d+\.\d+)\.\d+/);
    if (!extMatch) { return; }
    const extMajorMinor = extMatch[1]; // e.g. "0.42"

    if (extMajorMinor !== serverMajorMinor) {
      log('WARN', `Version mismatch: extension v${extVersion}, server v${stdout.trim()}`);
      const extParts = extMajorMinor.split('.').map(Number);
      const srvParts = serverMajorMinor.split('.').map(Number);
      // Only warn if extension is BEHIND the server (not ahead, which is dev)
      if (extParts[0] < srvParts[0] || (extParts[0] === srvParts[0] && extParts[1] < srvParts[1])) {
        window.showWarningMessage(
          `Nika extension v${extVersion} is outdated (server is v${serverMajorMinor}.x). ` +
          `Update for the best experience.`,
          'Update Extension',
        ).then((choice) => {
          if (choice === 'Update Extension') {
            commands.executeCommand(
              'workbench.extensions.installExtension',
              'supernovae.nika-lang',
            ).then(undefined, () => {
              // Cursor and other hosts may not support this command — open marketplace
              env.openExternal(Uri.parse(
                'https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang'
              ));
            });
          }
        });
      }
    } else {
      log('INFO', `Version match: extension v${extVersion}, server v${stdout.trim()}`);
    }
  });
}

export function startClient(
  context: ExtensionContext,
  state: ClientState,
  log: LogFn,
  overridePath?: string,
): void {
  const config = workspace.getConfiguration('nika');
  const serverPath = overridePath ?? getNikaPath();
  const extraArgs = config.get<string[]>('server.extraArgs', []);

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: ['lsp', '--embedded-daemon', ...extraArgs],
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'nika' },
      { scheme: 'file', pattern: '**/*.nika.yaml' },
    ],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher('**/*.nika.yaml'),
    },
    outputChannelName: 'Nika Language Server',
  };

  state.client = new LanguageClient(
    'nika',
    'Nika Language Server',
    serverOptions,
    clientOptions,
  );

  log('INFO', `Starting LSP: ${serverPath} lsp ${extraArgs.join(' ')}`);

  state.client.start().then(() => {
    log('INFO', 'Language server started successfully');
    if (state.statusBarItem) {
      state.statusBarItem.text = '$(zap) Nika: Ready';
      state.statusBarItem.backgroundColor = undefined;
    }

    // Check for version mismatch between extension and LSP server
    checkVersionMismatch(context, log);

    // Auto-configure MCP for the current IDE
    if (isCursor()) {
      ensureCursorMcpConfig(state.resolvedServerPath, log);
      ensureCursorRules(log);
    } else if (isWindsurf()) {
      ensureWindsurfMcpConfig(state.resolvedServerPath, log);
    } else {
      ensureVscodeMcpConfig(state.resolvedServerPath, log);
    }

    // Forward execution events from LSP to DAG webview for live updates
    if (state.client) {
      state.client.onNotification('nika/executionEvent', (event: { taskId: string; status: string }) => {
        log('INFO', `Execution event: ${event.taskId} → ${event.status}`);
        if (state.activeDagPanel) {
          state.activeDagPanel.updateTaskStatus(event.taskId, event.status as any);
        }
      });
    }

    // Poll daemon status every 30s — clear previous interval to prevent accumulation
    if (state.statusPollInterval !== undefined) {
      clearInterval(state.statusPollInterval);
    }
    state.statusPollInterval = setInterval(async () => {
      if (!state.client || !state.client.isRunning()) {
        if (state.statusBarItem) {
          state.statusBarItem.text = '$(zap) Nika: LSP $(x)';
        }
        return;
      }
      try {
        const status = await state.client.sendRequest<{ connected: boolean }>('nika/daemonStatus');
        if (state.statusBarItem) {
          state.statusBarItem.text = status.connected
            ? '$(zap) Nika: LSP $(check) | Daemon $(check)'
            : '$(zap) Nika: LSP $(check) | Daemon $(x)';
          state.statusBarItem.backgroundColor = undefined;
        }
      } catch {
        if (state.statusBarItem) {
          state.statusBarItem.text = '$(zap) Nika: LSP $(check)';
        }
      }
    }, 30000);
  }).catch((err: Error) => {
    log('ERROR', `LSP failed to start: ${err.message}`);
    if (state.statusBarItem) {
      state.statusBarItem.text = '$(zap) Nika: LSP $(x)';
    }
    window.showErrorMessage(
      `Failed to start Nika language server: ${err.message}. ` +
      `Make sure 'nika' is installed and in your PATH, or set nika.server.path.`,
    );
  });

  context.subscriptions.push({
    dispose: () => {
      if (state.client) {
        state.client.stop();
      }
    },
  });
}
