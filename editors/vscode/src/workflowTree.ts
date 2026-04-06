import {
  TreeDataProvider,
  TreeItem,
  TreeItemCollapsibleState,
  EventEmitter,
  Event,
  workspace,
  Uri,
  ThemeIcon,
  Command,
} from 'vscode';
import * as fs from 'fs';
import { parseWorkflowTasks } from './workflowParser';

type WorkflowItem = WorkflowFileItem | WorkflowTaskItem;

class WorkflowFileItem extends TreeItem {
  constructor(
    public readonly uri: Uri,
    public readonly tasks: { id: string; line: number; verb: string }[],
  ) {
    super(
      uri.path.split('/').pop() ?? uri.fsPath,
      tasks.length > 0
        ? TreeItemCollapsibleState.Expanded
        : TreeItemCollapsibleState.None,
    );
    this.resourceUri = uri;
    this.iconPath = new ThemeIcon('file');
    this.contextValue = 'workflowFile';
    this.tooltip = uri.fsPath;
    this.description = `${tasks.length} task${tasks.length !== 1 ? 's' : ''}`;
    this.command = {
      command: 'vscode.open',
      title: 'Open Workflow',
      arguments: [uri],
    };
  }
}

class WorkflowTaskItem extends TreeItem {
  constructor(
    public readonly taskId: string,
    public readonly uri: Uri,
    public readonly line: number,
    public readonly verb: string,
  ) {
    super(taskId, TreeItemCollapsibleState.None);
    this.iconPath = WorkflowTaskItem.verbIcon(verb);
    this.contextValue = 'workflowTask';
    this.description = verb;
    this.tooltip = `${taskId} (${verb}) — line ${line + 1}`;
    this.command = {
      command: 'nika.openTaskLocation',
      title: 'Go to Task',
      arguments: [uri, line],
    } as Command;
  }

  private static verbIcon(verb: string): ThemeIcon {
    switch (verb) {
      case 'infer': return new ThemeIcon('sparkle');
      case 'exec': return new ThemeIcon('terminal');
      case 'fetch': return new ThemeIcon('cloud-download');
      case 'invoke': return new ThemeIcon('plug');
      case 'agent': return new ThemeIcon('robot');
      default: return new ThemeIcon('circle-outline');
    }
  }
}

export class WorkflowTreeProvider implements TreeDataProvider<WorkflowItem> {
  private _onDidChangeTreeData = new EventEmitter<WorkflowItem | undefined | void>();
  readonly onDidChangeTreeData: Event<WorkflowItem | undefined | void> =
    this._onDidChangeTreeData.event;

  refresh(): void {
    this._onDidChangeTreeData.fire();
  }

  getTreeItem(element: WorkflowItem): TreeItem {
    return element;
  }

  async getChildren(element?: WorkflowItem): Promise<WorkflowItem[]> {
    if (!element) {
      // Root: list all .nika.yaml files
      const files = await workspace.findFiles('**/*.nika.yaml', '**/node_modules/**', 100);
      files.sort((a, b) => a.fsPath.localeCompare(b.fsPath));

      return files.map((uri) => {
        try {
          const content = fs.readFileSync(uri.fsPath, 'utf-8');
          const tasks = parseWorkflowTasks(content);
          return new WorkflowFileItem(uri, tasks);
        } catch {
          return new WorkflowFileItem(uri, []);
        }
      });
    }

    if (element instanceof WorkflowFileItem) {
      return element.tasks.map(
        (t) => new WorkflowTaskItem(t.id, element.uri, t.line, t.verb),
      );
    }

    return [];
  }
}
