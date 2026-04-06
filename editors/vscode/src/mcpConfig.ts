// mcpConfig.ts — IDE-specific MCP configuration
//
// Auto-generates MCP config files for VS Code, Cursor, and Windsurf.
// All functions receive resolvedServerPath and log as parameters (no module state).

import { workspace, Uri, env } from 'vscode';
import * as fs from 'fs';
import * as path from 'path';

export type LogFn = (level: string, msg: string) => void;

export function isCursor(): boolean {
  return env.appName === 'Cursor' || env.uriScheme === 'cursor';
}

export function isWindsurf(): boolean {
  return env.appName === 'Windsurf' || env.uriScheme === 'windsurf';
}

export async function ensureCursorMcpConfig(resolvedServerPath: string | undefined, log: LogFn): Promise<void> {
  const folder = workspace.workspaceFolders?.[0];
  if (!folder) { return; }

  const cursorDir = Uri.joinPath(folder.uri, '.cursor');
  const mcpPath = Uri.joinPath(cursorDir, 'mcp.json');

  try {
    await workspace.fs.stat(mcpPath);
    return; // Already exists — don't overwrite
  } catch {
    // File doesn't exist — create it
  }

  const nikaPath = resolvedServerPath ?? 'nika';
  const mcpConfig = {
    mcpServers: {
      nika: {
        command: nikaPath,
        args: ['mcp', 'serve', '--stdio'],
      },
    },
  };

  await workspace.fs.createDirectory(cursorDir);
  await workspace.fs.writeFile(mcpPath, Buffer.from(JSON.stringify(mcpConfig, null, 2)));
  log('INFO', 'Auto-generated .cursor/mcp.json for Cursor MCP integration');
}

export async function ensureCursorRules(log: LogFn): Promise<void> {
  const folder = workspace.workspaceFolders?.[0];
  if (!folder) { return; }

  const rulesDir = Uri.joinPath(folder.uri, '.cursor', 'rules');
  const rulePath = Uri.joinPath(rulesDir, 'nika.mdc');

  try {
    await workspace.fs.stat(rulePath);
    return; // Already exists
  } catch {
    // Create
  }

  const content = [
    '---',
    'description: Nika workflow engine rules for AI assistance',
    'globs: ["**/*.nika.yaml"]',
    'alwaysApply: false',
    '---',
    '',
    '# Nika Workflow Engine',
    '',
    'Schema: nika/workflow@0.12. Extension: .nika.yaml.',
    '',
    '## 5 Verbs',
    '- infer: LLM generation',
    '- exec: Shell command',
    '- fetch: HTTP request',
    '- invoke: MCP/builtin tool call',
    '- agent: Multi-turn loop',
    '',
    '## Key Rules',
    '- Bindings: with: { alias: $task_id } then {{with.alias}}',
    '- Always start with schema: "nika/workflow@0.12"',
    '- depends_on is always an array: depends_on: [task_id]',
    '- for_each output is always an array',
    '- shell: true requires | shell transform on bindings',
    '- Secrets via $env.VAR_NAME, never hardcode',
    '- timeout is always in seconds (not milliseconds)',
    '',
    '## Providers',
    'anthropic, openai, mistral, groq, deepseek, gemini, xai, native, mock',
    '',
    'Refer to AGENTS.md for complete documentation.',
  ].join('\n');

  await workspace.fs.createDirectory(rulesDir);
  await workspace.fs.writeFile(rulePath, Buffer.from(content));
  log('INFO', 'Auto-generated .cursor/rules/nika.mdc');
}

export async function ensureVscodeMcpConfig(resolvedServerPath: string | undefined, log: LogFn): Promise<void> {
  const folder = workspace.workspaceFolders?.[0];
  if (!folder) { return; }

  const vscodeDir = Uri.joinPath(folder.uri, '.vscode');
  const mcpPath = Uri.joinPath(vscodeDir, 'mcp.json');

  try {
    await workspace.fs.stat(mcpPath);
    return;
  } catch {
    // Create
  }

  const nikaPath = resolvedServerPath ?? 'nika';
  const mcpConfig = {
    servers: {
      nika: {
        type: 'stdio',
        command: nikaPath,
        args: ['mcp', 'serve', '--stdio'],
      },
    },
  };

  await workspace.fs.createDirectory(vscodeDir);
  await workspace.fs.writeFile(mcpPath, Buffer.from(JSON.stringify(mcpConfig, null, 2)));
  log('INFO', 'Auto-generated .vscode/mcp.json for VS Code MCP integration');
}

export async function ensureWindsurfMcpConfig(resolvedServerPath: string | undefined, log: LogFn): Promise<void> {
  // Windsurf uses a global config at ~/.codeium/windsurf/mcp_config.json
  const homeDir = process.env.HOME ?? process.env.USERPROFILE;
  if (!homeDir) { return; }

  const configDir = path.join(homeDir, '.codeium', 'windsurf');
  const configPath = path.join(configDir, 'mcp_config.json');

  if (fs.existsSync(configPath)) {
    // Check if nika is already configured
    try {
      const existing = JSON.parse(fs.readFileSync(configPath, 'utf-8'));
      if (existing?.mcpServers?.nika) { return; }
    } catch {
      // Malformed JSON — don't overwrite
      return;
    }
  }

  const nikaPath = resolvedServerPath ?? 'nika';
  const mcpConfig = {
    mcpServers: {
      nika: {
        command: nikaPath,
        args: ['mcp', 'serve', '--stdio'],
      },
    },
  };

  try {
    fs.mkdirSync(configDir, { recursive: true });
    if (fs.existsSync(configPath)) {
      // Merge into existing config
      const existing = JSON.parse(fs.readFileSync(configPath, 'utf-8'));
      existing.mcpServers = { ...existing.mcpServers, nika: mcpConfig.mcpServers.nika };
      fs.writeFileSync(configPath, JSON.stringify(existing, null, 2));
    } else {
      fs.writeFileSync(configPath, JSON.stringify(mcpConfig, null, 2));
    }
    log('INFO', 'Auto-configured Windsurf MCP at ~/.codeium/windsurf/mcp_config.json');
  } catch (err) {
    log('WARN', `Failed to configure Windsurf MCP: ${err}`);
  }
}
