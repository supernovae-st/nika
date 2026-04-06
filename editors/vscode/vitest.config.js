"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const config_1 = require("vitest/config");
exports.default = (0, config_1.defineConfig)({
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
//# sourceMappingURL=vitest.config.js.map