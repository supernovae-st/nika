import {
  workspace,
  commands,
  ExtensionContext,
  window,
  Uri,
} from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';
import { exec } from 'child_process';

let client: LanguageClient | undefined;

function getNikaPath(): string {
  return workspace.getConfiguration('nika').get<string>('server.path', 'nika');
}

function startClient(context: ExtensionContext): void {
  const config = workspace.getConfiguration('nika');
  const serverPath = getNikaPath();
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

function runNikaCommand(subcmd: string, filePath: string): void {
  const nika = getNikaPath();
  const terminal = window.createTerminal({ name: `Nika: ${subcmd}` });
  terminal.show();
  terminal.sendText(`${nika} ${subcmd} "${filePath}"`);
}

export function activate(context: ExtensionContext): void {
  startClient(context);

  // Command: Run current workflow
  context.subscriptions.push(
    commands.registerCommand('nika.runWorkflow', () => {
      const editor = window.activeTextEditor;
      if (!editor || !editor.document.fileName.endsWith('.nika.yaml')) {
        window.showWarningMessage('Open a .nika.yaml file first.');
        return;
      }
      runNikaCommand('run', editor.document.fileName);
    }),
  );

  // Command: Validate current workflow
  context.subscriptions.push(
    commands.registerCommand('nika.checkWorkflow', () => {
      const editor = window.activeTextEditor;
      if (!editor || !editor.document.fileName.endsWith('.nika.yaml')) {
        window.showWarningMessage('Open a .nika.yaml file first.');
        return;
      }
      runNikaCommand('check', editor.document.fileName);
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
        `schema: "nika/workflow@0.12"\nworkflow: ${name}\ndescription: ""\nprovider: anthropic\n\ntasks:\n  - id: start\n    infer: ""\n`,
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

  // Command: Restart language server
  context.subscriptions.push(
    commands.registerCommand('nika.restartServer', async () => {
      if (client) {
        await client.stop();
        client = undefined;
      }
      startClient(context);
      window.showInformationMessage('Nika language server restarted.');
    }),
  );
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
