import * as path from 'path';
import {
  workspace,
  ExtensionContext,
  window,
} from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext): void {
  const config = workspace.getConfiguration('nika');
  const serverPath = config.get<string>('server.path', 'nika');
  const extraArgs = config.get<string[]>('server.extraArgs', []);

  const serverOptions: ServerOptions = {
    command: serverPath,
    args: ['lsp', ...extraArgs],
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

  client = new LanguageClient(
    'nika',
    'Nika Language Server',
    serverOptions,
    clientOptions,
  );

  client.start().catch((err) => {
    window.showErrorMessage(
      `Failed to start Nika language server: ${err.message}. ` +
      `Make sure 'nika' is installed and in your PATH, or set nika.server.path.`,
    );
  });

  context.subscriptions.push({
    dispose: () => {
      if (client) {
        client.stop();
      }
    },
  });
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
