import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['src/test/**/*.test.ts'],
    // Don't try to resolve vscode module — it's provided at runtime
    server: {
      deps: {
        external: ['vscode'],
      },
    },
  },
});
