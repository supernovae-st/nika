# Research: TDD Patterns for VS Code Extensions & LSP Servers

> Date: 2026-04-06
> Author: Thibaut + Nika
> Scope: Test strategy for nika-lang VS Code extension + nika-lsp Rust server

---

## Executive Summary

VS Code extension testing operates at **4 distinct layers**, each with different
tools and tradeoffs. The key insight: the official `@vscode/test-electron` framework
downloads a real VS Code instance and runs tests inside it -- there is no way to
"unit test" code that uses `vscode.*` APIs without either (a) running inside VS Code
or (b) mocking the entire `vscode` module. rust-analyzer chose option (a) for
integration tests and option (b) for pure logic tests. That is the right pattern
for Nika.

---

## 1. The 4 Testing Layers

```
Layer 4: E2E (VS Code instance)     @vscode/test-electron + mocha
Layer 3: Integration (LSP protocol)  Rust: spawn binary + JSON-RPC over stdio
Layer 2: Unit (extension logic)      vitest/mocha + vscode module mock
Layer 1: Unit (LSP core logic)       cargo test --lib (pure Rust, no IO)
```

### Recommendation for Nika

| Layer | What | Framework | Where |
|-------|------|-----------|-------|
| L1 | LSP intelligence (completions, diagnostics, hover) | `cargo test --lib` | `tools/nika-lsp-core/` |
| L2 | Extension logic (artifact name, version check, config) | vitest + vscode mock | `editors/vscode/src/test/unit/` |
| L3 | LSP protocol integration | Rust integration tests | `tools/nika-lsp/tests/` |
| L4 | Full extension in VS Code | `@vscode/test-electron` + mocha | `editors/vscode/src/test/e2e/` |

---

## 2. Framework Options (Detailed Comparison)

### 2.1 `@vscode/test-electron` (v2.5.2)

The official framework. Downloads a real VS Code binary, launches it with your
extension loaded, and runs a test entry point inside the extension host.

**How it works:**
```ts
// runTests.ts — the launcher (runs in Node.js, NOT in VS Code)
import { runTests } from '@vscode/test-electron';

async function main() {
  await runTests({
    extensionDevelopmentPath: path.resolve(__dirname, '../../'),
    extensionTestsPath: path.resolve(__dirname, './suite/index'),
    launchArgs: ['--disable-extensions'],
    version: 'stable', // or '1.75.0', 'insiders'
  });
}
```

```ts
// suite/index.ts — the entry point (runs INSIDE VS Code extension host)
import * as Mocha from 'mocha';
import { glob } from 'glob';

export function run(testsRoot: string, cb: (error: any, failures?: number) => void): void {
  const mocha = new Mocha({ ui: 'tdd' });
  glob('**/**.test.js', { cwd: testsRoot }).then((files) => {
    files.forEach((f) => mocha.addFile(path.resolve(testsRoot, f)));
    mocha.run((failures) => cb(null, failures));
  });
}
```

```ts
// suite/extension.test.ts — actual test (has full vscode.* access)
import * as vscode from 'vscode';
import * as assert from 'assert';

suite('Extension Tests', () => {
  test('Extension activates on .nika.yaml file', async () => {
    const doc = await vscode.workspace.openTextDocument({
      language: 'nika',
      content: 'schema: "nika/workflow@0.12"\n',
    });
    await vscode.window.showTextDocument(doc);
    // Extension should now be active
    const ext = vscode.extensions.getExtension('supernovae.nika-lang');
    assert.ok(ext?.isActive);
  });
});
```

**Pros:** Real VS Code APIs, tests what users actually experience.
**Cons:** Slow (downloads VS Code, ~5-15s startup), requires display (use `xvfb` in CI),
flaky on CI runners, cannot run in `cargo test --lib`.

### 2.2 `@vscode/test-cli` (v0.0.12)

A wrapper around `@vscode/test-electron` that provides a config-driven CLI runner
with Mocha integration. The recommended approach for new extensions.

```js
// .vscode-test.mjs
import { defineConfig } from '@vscode/test-cli';

export default defineConfig([
  {
    files: 'out/test/e2e/**/*.test.js',
    version: 'stable',
    workspaceFolder: './test-fixtures/sample-project',
    mocha: {
      timeout: 20000,
    },
  },
  {
    // Test on minimum supported version too
    files: 'out/test/e2e/**/*.test.js',
    version: '1.75.0',
  },
]);
```

Run: `npx vscode-test` (discovers tests, downloads VS Code, runs them).

**Key advantage:** Supports running tests from VS Code's built-in test explorer,
which means you can debug tests with breakpoints.

### 2.3 `@vscode/test-web` (v0.0.80)

For web extensions only. Downloads VS Code for the Web and runs tests in a browser.
Not relevant for Nika (native binary extension).

### 2.4 Vitest / Jest for Unit Tests (no VS Code dependency)

For pure logic that does not touch `vscode.*` APIs:

```ts
// src/utils.ts — pure logic, no vscode imports
export function getArtifactName(platform: string, arch: string): string | null {
  if (platform === 'darwin' && arch === 'arm64') return 'nika-macos-arm64';
  if (platform === 'darwin' && arch === 'x64') return 'nika-macos-x64';
  if (platform === 'linux' && arch === 'x64') return 'nika-linux-x64';
  if (platform === 'linux' && arch === 'arm64') return 'nika-linux-arm64';
  if (platform === 'win32' && arch === 'x64') return 'nika-windows-x64';
  return null;
}

export function parseServerVersion(stdout: string): string | null {
  const match = stdout.match(/(\d+\.\d+)\.\d+/);
  return match ? match[1] : null;
}
```

```ts
// src/test/unit/utils.test.ts
import { describe, it, expect } from 'vitest';
import { getArtifactName, parseServerVersion } from '../../utils';

describe('getArtifactName', () => {
  it('returns correct name for macOS ARM', () => {
    expect(getArtifactName('darwin', 'arm64')).toBe('nika-macos-arm64');
  });

  it('returns null for unsupported platform', () => {
    expect(getArtifactName('freebsd', 'x64')).toBeNull();
  });
});

describe('parseServerVersion', () => {
  it('extracts major.minor from nika --version output', () => {
    expect(parseServerVersion('nika 0.72.0')).toBe('0.72');
  });

  it('handles dev version format', () => {
    expect(parseServerVersion('0.72.0-dev (abc1234, built 2h ago)')).toBe('0.72');
  });

  it('returns null for garbage', () => {
    expect(parseServerVersion('not a version')).toBeNull();
  });
});
```

**Key pattern:** Extract pure logic from extension.ts into separate modules
that have zero `vscode` imports, then test those with vitest.

---

## 3. How rust-analyzer Tests Their VS Code Extension

### Architecture

rust-analyzer splits testing into two clear tiers:

**Tier 1: TypeScript unit tests** (`editors/code/tests/unit/`)
- Run inside VS Code via `@vscode/test-electron`
- Custom test runner (no Mocha -- they built their own `Suite`/`Context` classes)
- Tests run on BOTH the minimum supported version AND `stable`
- Tests cover: config substitution, bootstrap/toolchain selection, task generation, launch configs

```ts
// Their custom runner (editors/code/tests/runTests.ts)
await runTests({
  version: minimalVersion,   // from package.json engines.vscode
  launchArgs: ['--disable-extensions', extensionDevelopmentPath],
  extensionDevelopmentPath,
  extensionTestsPath: path.resolve(__dirname, './unit/index'),
});
// Then again on stable
await runTests({
  version: 'stable',
  launchArgs: ['--disable-extensions', extensionDevelopmentPath],
  extensionDevelopmentPath,
  extensionTestsPath: path.resolve(__dirname, './unit/index'),
});
```

**Tier 2: Rust LSP integration tests** (`crates/rust-analyzer/tests/slow-tests/`)
- Full LSP event loop over `lsp_server::Connection`
- Spawn server in-process (not a child process)
- Test fixture system: inline Rust source files in test strings
- `#[ignore]` gated behind `skip_slow_tests()` for CI

```rust
// Their test support harness (simplified)
struct Server {
    dir: TestDir,
    client: Connection,
    messages: RefCell<Vec<Message>>,
}

impl Server {
    fn send_request<R: lsp_types::request::Request>(&self, params: R::Params) -> R::Result {
        // Sends JSON-RPC request, reads response, deserializes
    }

    fn wait_until_workspace_is_loaded(&self) -> &Self {
        // Reads messages until indexing is complete
    }
}

// Example: testing completions
let server = Project::with_fixture(r#"
//- /Cargo.toml
[package]
name = "foo"
//- /src/lib.rs
use std::collections::Spam;
"#).server().wait_until_workspace_is_loaded();

let res = server.send_request::<Completion>(CompletionParams {
    text_document_position: TextDocumentPositionParams::new(
        server.doc_id("src/lib.rs"),
        Position::new(0, 23),
    ),
    ..Default::default()
});
assert!(res.to_string().contains("HashMap"));
```

**Key takeaways from rust-analyzer:**
1. They do NOT test JSON shapes between client and server -- "there's little value"
2. Heavy investment in the Rust-side test harness, minimal TypeScript tests
3. TypeScript tests only cover config/bootstrap logic, not LSP features
4. All LSP feature testing is done in Rust via the `lsp_server` crate
5. Tests use `crossbeam_channel` for message passing with timeouts
6. Wildcard matching (`[..]`) for assertions on paths/versions

---

## 4. Testing LSP Features (Code Lens, Inlay Hints, Completions)

### 4.1 Approach A: Rust Integration Tests (nika-lsp already has this)

The existing `tools/nika-lsp/tests/e2e_harness.rs` is well-structured. It spawns
the `nika-lsp` binary and communicates via JSON-RPC over stdio. This is the right
pattern and covers:

- Initialize handshake
- Diagnostics on valid/invalid documents
- Completions
- Hover
- Document symbols
- Folding ranges
- Semantic tokens
- Code lens
- Rename
- Inlay hints

**What to add for completeness:**

```rust
// Test: code lens resolving
#[test]
#[ignore = "e2e: requires cargo build -p nika-lsp"]
fn test_code_lens_resolve() {
    let mut client = LspClient::new();
    client.initialize();

    let doc = r#"schema: "nika/workflow@0.12"
workflow: lens-test
provider: mock
model: mock-model
tasks:
  - id: step1
    infer: "Hello world"
"#;

    client.open_document("file:///tmp/lens-resolve.nika.yaml", doc);
    std::thread::sleep(Duration::from_millis(300));

    // Get code lenses
    let resp = client.send_request("textDocument/codeLens", json!({
        "textDocument": { "uri": "file:///tmp/lens-resolve.nika.yaml" },
    }));

    let lenses = resp["result"].as_array();
    if let Some(lenses) = lenses {
        if !lenses.is_empty() {
            // Resolve first lens
            let resolve_resp = client.send_request("codeLens/resolve", lenses[0].clone());
            assert!(resolve_resp.get("error").is_none(),
                "codeLens/resolve error: {:?}", resolve_resp);
            // Resolved lens should have a command
            assert!(resolve_resp["result"]["command"].is_object(),
                "resolved lens should have command");
        }
    }

    client.shutdown();
}

// Test: textDocument/didChange triggers re-diagnostics
#[test]
#[ignore = "e2e: requires cargo build -p nika-lsp"]
fn test_didchange_updates_diagnostics() {
    let mut client = LspClient::new();
    client.initialize();

    // Start with valid document
    let valid_doc = r#"schema: "nika/workflow@0.12"
workflow: change-test
provider: mock
model: m
tasks:
  - id: s1
    infer: "hello"
"#;
    let uri = "file:///tmp/change.nika.yaml";
    client.open_document(uri, valid_doc);

    // Wait for clean diagnostics
    let _ = client.read_until(
        |msg| msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics"),
        Duration::from_secs(5),
    );

    // Change document to invalid content
    client.send_notification("textDocument/didChange", json!({
        "textDocument": { "uri": uri, "version": 2 },
        "contentChanges": [{
            "text": "schema: \"invalid\"\nworkflow: bad\ntasks: []\n"
        }],
    }));

    // Wait for error diagnostics
    let diag = client.read_until(
        |msg| {
            msg.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics")
                && msg["params"]["diagnostics"].as_array().map(|a| !a.is_empty()).unwrap_or(false)
        },
        Duration::from_secs(5),
    );

    assert!(diag.is_some(), "expected error diagnostics after invalid change");
    client.shutdown();
}
```

### 4.2 Approach B: Pure Rust Unit Tests (nika-lsp-core)

The heaviest testing should be in `nika-lsp-core` where the intelligence lives.
These tests need zero IO and run in `cargo test --lib`:

```rust
// nika-lsp-core/src/completion.rs
#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc(text: &str) -> Document {
        Document::new("file:///test.nika.yaml".into(), text.into(), 1)
    }

    #[test]
    fn completes_verbs_after_task_id() {
        let doc = make_doc("schema: \"nika/workflow@0.12\"\ntasks:\n  - id: s1\n    ");
        let pos = Position { line: 3, character: 4 };
        let items = compute_completions(&doc, pos);

        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"infer"));
        assert!(labels.contains(&"exec"));
        assert!(labels.contains(&"fetch"));
        assert!(labels.contains(&"invoke"));
        assert!(labels.contains(&"agent"));
    }

    #[test]
    fn completes_provider_names() {
        let doc = make_doc("schema: \"nika/workflow@0.12\"\nprovider: ");
        let pos = Position { line: 1, character: 10 };
        let items = compute_completions(&doc, pos);

        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"anthropic"));
        assert!(labels.contains(&"openai"));
        assert!(labels.contains(&"mock"));
    }

    #[test]
    fn completes_binding_references() {
        let doc = make_doc("schema: \"nika/workflow@0.12\"\ntasks:\n  - id: step1\n    infer: \"hi\"\n  - id: step2\n    with:\n      data: $");
        let pos = Position { line: 6, character: 12 };
        let items = compute_completions(&doc, pos);

        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"step1"));
    }

    #[test]
    fn hover_on_infer_returns_docs() {
        let doc = make_doc("schema: \"nika/workflow@0.12\"\ntasks:\n  - id: s1\n    infer: \"hi\"");
        let pos = Position { line: 3, character: 6 }; // on "infer"
        let hover = compute_hover(&doc, pos);

        assert!(hover.is_some());
        let content = hover.unwrap().contents;
        assert!(content.contains("LLM generation"));
    }

    #[test]
    fn diagnostics_detect_dag_cycle() {
        let doc = make_doc(r#"schema: "nika/workflow@0.12"
tasks:
  - id: a
    depends_on: [b]
    infer: "hello"
  - id: b
    depends_on: [a]
    infer: "world"
"#);
        let diags = compute_diagnostics(&doc);
        assert!(diags.iter().any(|d| d.message.contains("cycle")));
    }

    #[test]
    fn inlay_hints_show_model_info() {
        let doc = make_doc(r#"schema: "nika/workflow@0.12"
provider: anthropic
tasks:
  - id: gen
    infer:
      prompt: "test"
      model: claude-sonnet-4-20250514
"#);
        let range = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 7, character: 0 },
        };
        let hints = compute_inlay_hints(&doc, range);
        // Should have hint next to model showing pricing or context window
        assert!(!hints.is_empty());
    }
}
```

### 4.3 Approach C: VS Code E2E Tests

For testing the full integration (extension -> LSP client -> LSP server):

```ts
// editors/vscode/src/test/e2e/lsp.test.ts
import * as vscode from 'vscode';
import * as assert from 'assert';
import * as path from 'path';

suite('LSP Integration', () => {
  const fixturesPath = path.resolve(__dirname, '../../../test-fixtures');

  suiteSetup(async function () {
    this.timeout(30000);
    // Wait for extension to activate
    const ext = vscode.extensions.getExtension('supernovae.nika-lang');
    if (ext && !ext.isActive) {
      await ext.activate();
    }
    // Give LSP server time to initialize
    await new Promise(resolve => setTimeout(resolve, 3000));
  });

  test('Diagnostics appear for invalid workflow', async function () {
    this.timeout(15000);

    const doc = await vscode.workspace.openTextDocument({
      language: 'nika',
      content: 'schema: "invalid"\nworkflow: bad\ntasks: []\n',
    });
    await vscode.window.showTextDocument(doc);

    // Wait for diagnostics
    await new Promise<void>((resolve) => {
      const disposable = vscode.languages.onDidChangeDiagnostics((e) => {
        const diags = vscode.languages.getDiagnostics(doc.uri);
        if (diags.length > 0) {
          disposable.dispose();
          resolve();
        }
      });
      // Timeout fallback
      setTimeout(() => { disposable.dispose(); resolve(); }, 10000);
    });

    const diags = vscode.languages.getDiagnostics(doc.uri);
    assert.ok(diags.length > 0, 'Expected diagnostics for invalid schema');
  });

  test('Completions include verb names', async function () {
    this.timeout(15000);

    const doc = await vscode.workspace.openTextDocument({
      language: 'nika',
      content: 'schema: "nika/workflow@0.12"\ntasks:\n  - id: s1\n    ',
    });
    const editor = await vscode.window.showTextDocument(doc);

    // Trigger completion at end of document
    const position = new vscode.Position(3, 4);
    const completions = await vscode.commands.executeCommand<vscode.CompletionList>(
      'vscode.executeCompletionItemProvider',
      doc.uri,
      position,
    );

    assert.ok(completions, 'Expected completion list');
    const labels = completions.items.map(i => typeof i.label === 'string' ? i.label : i.label.label);
    assert.ok(labels.some(l => l.includes('infer')), `Expected 'infer' in completions, got: ${labels}`);
  });

  test('Hover shows verb documentation', async function () {
    this.timeout(15000);

    const doc = await vscode.workspace.openTextDocument({
      language: 'nika',
      content: 'schema: "nika/workflow@0.12"\ntasks:\n  - id: s1\n    infer: "hello"\n',
    });
    await vscode.window.showTextDocument(doc);

    const hovers = await vscode.commands.executeCommand<vscode.Hover[]>(
      'vscode.executeHoverProvider',
      doc.uri,
      new vscode.Position(3, 6),
    );

    assert.ok(hovers && hovers.length > 0, 'Expected hover content for verb');
  });
});
```

---

## 5. Testing Webview Panels

### The Problem

Webviews run in an isolated iframe. There is no direct DOM access from the
extension host. Communication is exclusively via `postMessage`.

### 5.1 Architecture for Testability

Split webview code into three testable units:

```
extension host                  |   webview iframe
--------------------------------|----------------------------
WebviewController               |   webview app (React/Svelte)
  - creates panel                |     - receives messages
  - sends postMessage           |     - sends postMessage
  - handles onDidReceiveMessage |     - renders UI
  - manages state               |
```

```ts
// src/webview/controller.ts — testable without webview
export interface WebviewBridge {
  postMessage(msg: unknown): Thenable<boolean>;
  onDidReceiveMessage: vscode.Event<unknown>;
}

export class DagViewController {
  private state: DagState = { tasks: [], selectedTask: null };

  constructor(private bridge: WebviewBridge) {
    bridge.onDidReceiveMessage((msg: any) => {
      this.handleMessage(msg);
    });
  }

  handleMessage(msg: { type: string; payload?: unknown }): void {
    switch (msg.type) {
      case 'taskSelected':
        this.state.selectedTask = msg.payload as string;
        break;
      case 'requestState':
        this.bridge.postMessage({ type: 'stateUpdate', payload: this.state });
        break;
    }
  }

  updateTasks(tasks: DagTask[]): void {
    this.state.tasks = tasks;
    this.bridge.postMessage({ type: 'stateUpdate', payload: this.state });
  }
}
```

### 5.2 Unit Testing the Controller (no VS Code needed)

```ts
// src/test/unit/dag-view-controller.test.ts
import { describe, it, expect, vi } from 'vitest';
import { DagViewController } from '../../webview/controller';

function createMockBridge() {
  const listeners: Array<(msg: unknown) => void> = [];
  return {
    postMessage: vi.fn().mockResolvedValue(true),
    onDidReceiveMessage: (listener: (msg: unknown) => void) => {
      listeners.push(listener);
      return { dispose: () => {} };
    },
    // Test helper: simulate message from webview
    simulateMessage(msg: unknown) {
      listeners.forEach(l => l(msg));
    },
  };
}

describe('DagViewController', () => {
  it('sends state update on requestState', () => {
    const bridge = createMockBridge();
    const controller = new DagViewController(bridge);

    bridge.simulateMessage({ type: 'requestState' });

    expect(bridge.postMessage).toHaveBeenCalledWith({
      type: 'stateUpdate',
      payload: { tasks: [], selectedTask: null },
    });
  });

  it('updates selected task on taskSelected', () => {
    const bridge = createMockBridge();
    const controller = new DagViewController(bridge);

    bridge.simulateMessage({ type: 'taskSelected', payload: 'step1' });
    bridge.simulateMessage({ type: 'requestState' });

    expect(bridge.postMessage).toHaveBeenLastCalledWith({
      type: 'stateUpdate',
      payload: expect.objectContaining({ selectedTask: 'step1' }),
    });
  });

  it('broadcasts new tasks to webview', () => {
    const bridge = createMockBridge();
    const controller = new DagViewController(bridge);

    controller.updateTasks([{ id: 'step1', verb: 'infer' }]);

    expect(bridge.postMessage).toHaveBeenCalledWith({
      type: 'stateUpdate',
      payload: {
        tasks: [{ id: 'step1', verb: 'infer' }],
        selectedTask: null,
      },
    });
  });
});
```

### 5.3 Testing the Webview App Itself

The webview HTML/JS is a normal web app. Test it with a standard web testing tool:

```ts
// webview-ui/src/test/app.test.ts (vitest with jsdom)
import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte'; // or React
import DagView from '../DagView.svelte';

// Mock the vscode API that gets injected into the webview
const mockVsCodeApi = {
  postMessage: vi.fn(),
  getState: vi.fn().mockReturnValue(null),
  setState: vi.fn(),
};

// In the webview, acquireVsCodeApi() returns this
vi.stubGlobal('acquireVsCodeApi', () => mockVsCodeApi);

describe('DagView webview', () => {
  it('sends taskSelected when node clicked', async () => {
    const { getByTestId } = render(DagView, {
      props: { tasks: [{ id: 'step1', verb: 'infer' }] },
    });

    await fireEvent.click(getByTestId('task-node-step1'));

    expect(mockVsCodeApi.postMessage).toHaveBeenCalledWith({
      type: 'taskSelected',
      payload: 'step1',
    });
  });
});
```

### 5.4 E2E Webview Test (in VS Code)

```ts
// src/test/e2e/webview.test.ts
suite('DAG Webview', () => {
  test('Opens DAG view without errors', async function () {
    this.timeout(15000);

    // Open a workflow file
    const doc = await vscode.workspace.openTextDocument({
      language: 'nika',
      content: 'schema: "nika/workflow@0.12"\ntasks:\n  - id: s1\n    infer: "hi"\n',
    });
    await vscode.window.showTextDocument(doc);

    // Execute the command that opens the webview
    await vscode.commands.executeCommand('nika.showDag');

    // We cannot inspect webview DOM, but we can verify:
    // 1. No error was thrown
    // 2. The webview panel exists
    // 3. The extension state is correct
  });
});
```

**Limitation:** You cannot assert on webview DOM content from E2E tests.
The webview is sandboxed. Test the webview UI separately with jsdom/vitest.

---

## 6. Testing postMessage Communication

### 6.1 Type-Safe Message Protocol

Define a shared message type contract:

```ts
// shared/messages.ts (imported by both extension and webview)
export type ExtensionToWebview =
  | { type: 'stateUpdate'; payload: DagState }
  | { type: 'taskOutput'; payload: { taskId: string; output: string } }
  | { type: 'themeChanged'; payload: 'light' | 'dark' };

export type WebviewToExtension =
  | { type: 'taskSelected'; payload: string }
  | { type: 'requestState' }
  | { type: 'runTask'; payload: string };
```

### 6.2 Contract Test Pattern

```ts
// src/test/unit/message-contract.test.ts
import { describe, it, expect } from 'vitest';
import type { ExtensionToWebview, WebviewToExtension } from '../../shared/messages';

// These tests validate that the message protocol is consistent

describe('Message contract', () => {
  it('stateUpdate has required fields', () => {
    const msg: ExtensionToWebview = {
      type: 'stateUpdate',
      payload: { tasks: [], selectedTask: null },
    };
    expect(msg.type).toBe('stateUpdate');
    expect(msg.payload).toHaveProperty('tasks');
  });

  it('extension handles all webview message types', () => {
    // This test ensures the handler switch covers all cases
    const allTypes: WebviewToExtension['type'][] = [
      'taskSelected',
      'requestState',
      'runTask',
    ];

    // Import the actual handler
    const { handleMessage } = require('../../webview/controller');
    // Each type should not throw
    for (const type of allTypes) {
      expect(() => handleMessage({ type })).not.toThrow();
    }
  });
});
```

### 6.3 Round-Trip Integration Test

```ts
// src/test/unit/postmessage-roundtrip.test.ts
import { describe, it, expect, vi } from 'vitest';
import { DagViewController } from '../../webview/controller';

describe('postMessage round-trip', () => {
  it('requestState -> stateUpdate -> render cycle', () => {
    const sent: unknown[] = [];
    const listeners: Array<(msg: unknown) => void> = [];

    const bridge = {
      postMessage: vi.fn((msg) => { sent.push(msg); return Promise.resolve(true); }),
      onDidReceiveMessage: (l: (msg: unknown) => void) => {
        listeners.push(l);
        return { dispose: () => {} };
      },
    };

    const controller = new DagViewController(bridge);
    controller.updateTasks([{ id: 's1', verb: 'infer' }]);

    // Simulate webview requesting current state
    listeners.forEach(l => l({ type: 'requestState' }));

    // Should have sent: initial stateUpdate + response to requestState
    expect(sent).toHaveLength(2);
    expect(sent[1]).toMatchObject({
      type: 'stateUpdate',
      payload: { tasks: [{ id: 's1', verb: 'infer' }] },
    });
  });
});
```

---

## 7. Mocking VS Code APIs in Unit Tests

### 7.1 The Problem

`import * as vscode from 'vscode'` only resolves inside the VS Code extension host.
In a normal Node.js/vitest environment, it fails.

### 7.2 Pattern: Dependency Injection

The preferred approach (used by rust-analyzer): extract pure logic into modules
that do NOT import `vscode`, and test those directly.

```ts
// BAD: tightly coupled to vscode
import * as vscode from 'vscode';
export function getServerPath(): string {
  return vscode.workspace.getConfiguration('nika').get<string>('server.path', 'nika');
}

// GOOD: dependency injection
export function getServerPath(config: { get<T>(key: string, defaultValue: T): T }): string {
  return config.get<string>('server.path', 'nika');
}

// Test:
it('returns configured path', () => {
  const mockConfig = { get: <T>(_key: string, def: T) => '/custom/nika' as unknown as T };
  expect(getServerPath(mockConfig)).toBe('/custom/nika');
});
```

### 7.3 Pattern: Module Mock (when DI is impractical)

For code that deeply uses `vscode.*`, mock the entire module:

```ts
// vitest.config.ts
export default defineConfig({
  test: {
    alias: {
      vscode: path.resolve(__dirname, './src/test/mocks/vscode.ts'),
    },
  },
});
```

```ts
// src/test/mocks/vscode.ts
import { vi } from 'vitest';

export const workspace = {
  getConfiguration: vi.fn(() => ({
    get: vi.fn((key: string, def: unknown) => def),
  })),
  createFileSystemWatcher: vi.fn(() => ({
    onDidChange: vi.fn(),
    onDidCreate: vi.fn(),
    onDidDelete: vi.fn(),
    dispose: vi.fn(),
  })),
  workspaceFolders: [],
  openTextDocument: vi.fn(),
  fs: {
    writeFile: vi.fn(),
  },
};

export const window = {
  showInformationMessage: vi.fn(),
  showWarningMessage: vi.fn(),
  showErrorMessage: vi.fn(),
  showInputBox: vi.fn(),
  createOutputChannel: vi.fn(() => ({
    appendLine: vi.fn(),
    show: vi.fn(),
    dispose: vi.fn(),
  })),
  createStatusBarItem: vi.fn(() => ({
    show: vi.fn(),
    hide: vi.fn(),
    dispose: vi.fn(),
    text: '',
    tooltip: '',
    command: '',
    backgroundColor: undefined,
  })),
  activeTextEditor: undefined,
  createTerminal: vi.fn(() => ({
    show: vi.fn(),
    sendText: vi.fn(),
    dispose: vi.fn(),
  })),
};

export const commands = {
  registerCommand: vi.fn(),
  executeCommand: vi.fn(),
};

export const extensions = {
  getExtension: vi.fn(),
};

export const languages = {
  getDiagnostics: vi.fn(() => []),
  onDidChangeDiagnostics: vi.fn(),
};

export const Uri = {
  parse: vi.fn((s: string) => ({ toString: () => s, fsPath: s })),
  joinPath: vi.fn((_base: any, ...parts: string[]) => ({
    fsPath: parts.join('/'),
    toString: () => parts.join('/'),
  })),
  file: vi.fn((p: string) => ({ fsPath: p })),
};

export const env = {
  openExternal: vi.fn(),
};

export enum StatusBarAlignment {
  Left = 1,
  Right = 2,
}

export enum ProgressLocation {
  Notification = 15,
}

// Minimal DiagnosticSeverity
export enum DiagnosticSeverity {
  Error = 0,
  Warning = 1,
  Information = 2,
  Hint = 3,
}

export class Position {
  constructor(public line: number, public character: number) {}
}

export class Range {
  constructor(public start: Position, public end: Position) {}
}
```

### 7.4 Pattern: Thin Adapter (rust-analyzer's actual approach)

rust-analyzer exports internal functions with `_private` for testing:

```ts
// src/bootstrap.ts
function orderFromPath(path: string, getVersion: (p: string) => Promise<string | undefined>): Promise<string> {
  // ... implementation
}

// Export for tests ONLY
export const _private = { orderFromPath, earliestToolchainPath };
```

```ts
// tests/unit/bootstrap.test.ts
import { _private } from '../../src/bootstrap';

suite.addTest('Order of nightly RA', async () => {
  assert.deepStrictEqual(
    await _private.orderFromPath(
      '/path/to/nightly/rust-analyzer',
      async (path) => 'rust-analyzer 1.67.0-nightly (b7bc90fe 2022-11-21)',
    ),
    '0-2022-11-21/0',
  );
});
```

---

## 8. Testing MCP Servers (rmcp, stdio transport)

### 8.1 rmcp Test Patterns

rmcp (the Rust MCP SDK) tests focus on tool macros and schema generation:

```rust
// Define a test server
#[derive(Debug, Clone, Default)]
pub struct Calculator;

#[tool(tool_box)]
impl Calculator {
    #[tool(description = "Calculate the sum of two numbers")]
    fn sum(&self, #[tool(aggr)] SumRequest { a, b }: SumRequest) -> String {
        (a + b).to_string()
    }
}

#[tool(tool_box)]
impl ServerHandler for Calculator {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("A simple calculator".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// Test the tool attributes and schema
#[tokio::test]
async fn test_tool_macros() {
    let server = Calculator::default();
    let attr = Calculator::sum_tool_attr();
    // Verify schema has correct type
    assert_eq!(attr.input_schema.get("type").unwrap(), "object");
}

// Test complex schema generation
#[test]
fn test_complex_schema() {
    let attr = Demo::chat_tool_attr();
    let input_schema = attr.input_schema;
    let enum_count = input_schema
        .get("definitions").unwrap()
        .as_object().unwrap()
        .get("ChatRole").unwrap()
        .as_object().unwrap()
        .get("enum").unwrap()
        .as_array().unwrap()
        .len();
    assert_eq!(enum_count, 4);
}
```

### 8.2 Testing an MCP Server via stdio (In-Process)

For Nika's MCP server, the best pattern is in-process testing with
`tokio::io::duplex`:

```rust
// tests/mcp_server_test.rs
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use serde_json::{json, Value};

/// Create a pair of connected streams for testing MCP over stdio
fn create_stdio_pair() -> (DuplexStream, DuplexStream) {
    tokio::io::duplex(64 * 1024)
}

/// Send a JSON-RPC message over a stream
async fn send_jsonrpc(writer: &mut DuplexStream, msg: &Value) {
    let body = serde_json::to_string(msg).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await.unwrap();
    writer.write_all(body.as_bytes()).await.unwrap();
    writer.flush().await.unwrap();
}

/// Read a JSON-RPC message from a stream
async fn recv_jsonrpc(reader: &mut DuplexStream) -> Value {
    let mut header = Vec::new();
    let mut buf = [0u8; 1];
    // Read until \r\n\r\n
    loop {
        reader.read_exact(&mut buf).await.unwrap();
        header.push(buf[0]);
        if header.ends_with(b"\r\n\r\n") { break; }
    }
    let header_str = String::from_utf8(header).unwrap();
    let content_length: usize = header_str
        .lines()
        .find(|l| l.starts_with("Content-Length:"))
        .unwrap()
        .split(':').nth(1).unwrap()
        .trim().parse().unwrap();

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn test_mcp_server_initialize() {
    let (client_read, server_write) = create_stdio_pair();
    let (server_read, client_write) = create_stdio_pair();

    // Spawn MCP server task
    let server_handle = tokio::spawn(async move {
        run_mcp_server(server_read, server_write).await
    });

    let mut writer = client_write;
    let mut reader = client_read;

    // Send initialize
    send_jsonrpc(&mut writer, &json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "1.0" }
        }
    })).await;

    let response = recv_jsonrpc(&mut reader).await;
    assert_eq!(response["id"], 1);
    assert!(response["result"]["capabilities"]["tools"].is_object());

    // Send initialized notification
    send_jsonrpc(&mut writer, &json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })).await;

    // Call a tool
    send_jsonrpc(&mut writer, &json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "nika_validate",
            "arguments": {
                "workflow_path": "test.nika.yaml"
            }
        }
    })).await;

    let tool_response = recv_jsonrpc(&mut reader).await;
    assert_eq!(tool_response["id"], 2);
    assert!(tool_response["result"]["content"].is_array());

    server_handle.abort();
}
```

### 8.3 Testing MCP via Child Process (Like the LSP E2E Harness)

```rust
#[tokio::test]
#[ignore = "requires cargo build -p nika"]
async fn test_mcp_server_via_stdio() {
    let binary = env!("CARGO_BIN_EXE_nika");
    let mut child = Command::new(binary)
        .args(["mcp", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn nika mcp");

    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);

    // Initialize
    send_lsp_message(&mut stdin, &json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "nika-test", "version": "0.1" }
        }
    })).await;

    let resp = read_lsp_message(&mut reader).await;
    assert_eq!(resp["result"]["serverInfo"]["name"], "nika");

    child.kill().unwrap();
}
```

---

## 9. Testing Platform-Specific Binary Bundling

The current extension downloads platform-specific binaries at runtime.
Here is how to test that:

### 9.1 Artifact Name Resolution (Pure Unit Test)

```ts
// Already covered in section 2.4 — pure function test with vitest
describe('getArtifactName', () => {
  const cases: [string, string, string | null][] = [
    ['darwin', 'arm64', 'nika-macos-arm64'],
    ['darwin', 'x64', 'nika-macos-x64'],
    ['linux', 'x64', 'nika-linux-x64'],
    ['linux', 'arm64', 'nika-linux-arm64'],
    ['win32', 'x64', 'nika-windows-x64'],
    ['freebsd', 'x64', null],
    ['win32', 'arm64', null],
  ];

  test.each(cases)('(%s, %s) -> %s', (platform, arch, expected) => {
    expect(getArtifactName(platform, arch)).toBe(expected);
  });
});
```

### 9.2 Archive Extraction (Integration Test)

```ts
// src/test/integration/archive.test.ts
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';
import { extractBinaryFromTarGz, extractBinaryFromZip } from '../../archive';

describe('extractBinaryFromTarGz', () => {
  let tmpDir: string;

  beforeAll(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nika-test-'));
  });

  afterAll(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('extracts nika binary from tar.gz', async () => {
    // Create a test archive with a known binary
    const archivePath = path.join(__dirname, '../fixtures/nika-macos-arm64-0.1.0.tar.gz');
    const destPath = path.join(tmpDir, 'nika');

    await extractBinaryFromTarGz(archivePath, destPath);

    expect(fs.existsSync(destPath)).toBe(true);
    const stat = fs.statSync(destPath);
    expect(stat.size).toBeGreaterThan(0);
  });

  it('rejects archive without nika binary', async () => {
    const archivePath = path.join(__dirname, '../fixtures/empty-archive.tar.gz');
    const destPath = path.join(tmpDir, 'nika-empty');

    await expect(extractBinaryFromTarGz(archivePath, destPath))
      .rejects.toThrow('nika binary not found');
  });
});
```

### 9.3 Binary Health Check (Mock)

```ts
describe('isBinaryWorking', () => {
  it('returns true when binary runs successfully', async () => {
    // Use the actual system `echo` as a stand-in
    const result = await isBinaryWorking('/bin/echo');
    expect(result).toBe(true);
  });

  it('returns false for non-existent binary', async () => {
    const result = await isBinaryWorking('/nonexistent/path/nika');
    expect(result).toBe(false);
  });
});
```

### 9.4 SHA256 Verification

```ts
describe('checksum verification', () => {
  it('passes for matching checksum', () => {
    const content = Buffer.from('test binary content');
    const hash = crypto.createHash('sha256').update(content).digest('hex');
    expect(verifyChecksum(content, hash)).toBe(true);
  });

  it('fails for mismatched checksum', () => {
    const content = Buffer.from('test binary content');
    expect(verifyChecksum(content, 'deadbeef')).toBe(false);
  });
});
```

---

## 10. Recommended Test Configuration for Nika

### 10.1 Package.json Scripts

```json
{
  "scripts": {
    "build": "esbuild ./src/extension.ts --bundle --outfile=out/extension.js --external:vscode --format=cjs --platform=node",
    "watch": "npm run build -- --sourcemap --watch",
    "test:unit": "vitest run",
    "test:unit:watch": "vitest watch",
    "test:e2e": "vscode-test",
    "test": "npm run test:unit && npm run test:e2e",
    "typecheck": "tsc --noEmit",
    "pretest:e2e": "npm run typecheck && npm run build"
  }
}
```

### 10.2 vitest.config.ts

```ts
import { defineConfig } from 'vitest/config';
import * as path from 'path';

export default defineConfig({
  test: {
    include: ['src/test/unit/**/*.test.ts'],
    alias: {
      vscode: path.resolve(__dirname, './src/test/mocks/vscode.ts'),
    },
  },
});
```

### 10.3 .vscode-test.mjs

```js
import { defineConfig } from '@vscode/test-cli';

export default defineConfig([
  {
    files: 'out/test/e2e/**/*.test.js',
    version: 'stable',
    workspaceFolder: './test-fixtures/sample-project',
    mocha: { timeout: 30000 },
  },
]);
```

### 10.4 Directory Structure

```
editors/vscode/
  package.json
  vitest.config.ts
  .vscode-test.mjs
  tsconfig.json
  src/
    extension.ts               # Entry point (thin, delegates to modules)
    utils.ts                   # Pure logic (artifact name, version parse, etc.)
    archive.ts                 # tar.gz / zip extraction (extracted from extension.ts)
    webview/
      controller.ts            # WebviewBridge adapter (testable without vscode)
      messages.ts              # Shared message types
    test/
      mocks/
        vscode.ts              # Full vscode module mock
      unit/
        utils.test.ts
        archive.test.ts
        controller.test.ts
        messages.test.ts
      e2e/
        lsp.test.ts            # LSP features via real VS Code
        activation.test.ts     # Extension activation
        commands.test.ts       # Registered commands
      fixtures/
        nika-macos-arm64-0.1.0.tar.gz   # Test archive
        sample-project/
          nika.toml
          test.nika.yaml
  test-fixtures/
    sample-project/
      nika.toml
      hello.nika.yaml
```

---

## 11. CI Configuration

### GitHub Actions

```yaml
# .github/workflows/extension-test.yml
name: Extension Tests
on: [push, pull_request]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: 20 }
      - run: cd editors/vscode && npm ci
      - run: cd editors/vscode && npm run test:unit

  e2e-tests:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: 20 }
      # Linux needs xvfb for VS Code GUI
      - if: runner.os == 'Linux'
        run: sudo apt-get install -y xvfb
      # Build the LSP server
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build -p nika-lsp
      - run: cd editors/vscode && npm ci
      - run: cd editors/vscode && npm run build
      # Run E2E (xvfb-run on Linux)
      - if: runner.os == 'Linux'
        run: cd editors/vscode && xvfb-run -a npm run test:e2e
      - if: runner.os != 'Linux'
        run: cd editors/vscode && npm run test:e2e

  lsp-integration:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build -p nika-lsp
      - run: cargo test -p nika-lsp --test e2e_harness -- --ignored
```

---

## 12. Key Decisions for Nika

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Unit test framework (TS) | vitest | Fast, native ESM, better DX than mocha for unit tests |
| E2E framework | `@vscode/test-cli` + mocha | Official, VS Code test explorer integration |
| LSP protocol tests | Rust e2e_harness (existing) | Already solid, binary-level testing |
| LSP logic tests | `cargo test --lib` in nika-lsp-core | Fast, no IO, pure functions |
| Webview testing | vitest + jsdom (webview app) + controller unit tests | Cannot test webview DOM from extension host |
| VS Code API mock | Module alias in vitest.config.ts | Clean, type-safe, no runtime patching |
| CI display | `xvfb-run` on Linux | Required for VS Code E2E tests |

### Priority Order

1. **L1: nika-lsp-core unit tests** -- Highest ROI, fastest feedback, most testable
2. **L2: Extension unit tests (vitest)** -- Extract pure logic, test with mocks
3. **L3: LSP E2E (existing harness)** -- Already exists, expand coverage
4. **L4: VS Code E2E** -- Last, slowest, most brittle, but catches integration bugs

---

## Sources

1. [microsoft/vscode-test](https://github.com/Microsoft/vscode-test) -- Official test-electron library and sample project
2. [microsoft/vscode-test-cli](https://github.com/microsoft/vscode-test-cli) -- Config-driven CLI test runner
3. [rust-lang/rust-analyzer](https://github.com/rust-lang/rust-analyzer) -- editors/code/tests/ (TypeScript) + crates/rust-analyzer/tests/slow-tests/ (Rust)
4. [4t145/rmcp](https://github.com/4t145/rmcp) -- Rust MCP SDK test patterns (crates/rmcp/tests/)
5. Existing: `tools/nika-lsp/tests/e2e_harness.rs` -- Nika's own LSP E2E test harness
6. Existing: `editors/vscode/src/extension.ts` -- Current extension source (760 lines, zero tests)

## Confidence Level

**High** -- All patterns are sourced from production projects (rust-analyzer, VS Code's
own test infrastructure, rmcp). The nika-lsp E2E harness is already well-designed;
the main gap is TypeScript-side unit tests and webview testing.
