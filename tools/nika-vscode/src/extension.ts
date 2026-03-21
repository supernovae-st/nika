import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const config = vscode.workspace.getConfiguration('nika.lsp');
  const enabled = config.get<boolean>('enabled', true);

  if (!enabled) {
    return;
  }

  const binary = config.get<string>('path', 'nika');

  const serverOptions: ServerOptions = {
    command: binary,
    args: ['lsp'],
    options: { env: { ...process.env } },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'nika-yaml' }],
    outputChannelName: 'Nika LSP',
  };

  client = new LanguageClient(
    'nika-lsp',
    'Nika Language Server',
    serverOptions,
    clientOptions,
  );

  client.start();
  context.subscriptions.push({ dispose: () => stopClient() });
}

export function deactivate(): Thenable<void> | undefined {
  return stopClient();
}

function stopClient(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  const stopping = client.stop();
  client = undefined;
  return stopping;
}
