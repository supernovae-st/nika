// 🏰 FORTRESS: Conventional Commits configuration
// https://commitlint.js.org/

module.exports = {
  extends: ['@commitlint/config-conventional'],
  rules: {
    // Type must be one of these
    'type-enum': [
      2,
      'always',
      [
        'feat',     // New feature
        'fix',      // Bug fix
        'docs',     // Documentation only
        'style',    // Formatting (no code change)
        'refactor', // Code refactoring
        'test',     // Adding/updating tests
        'chore',    // Tooling, CI, dependencies
        'perf',     // Performance improvement
        'ci',       // CI configuration
        'build',    // Build system changes
        'revert',   // Revert previous commit
      ],
    ],
    // Type must be lowercase
    'type-case': [2, 'always', 'lower-case'],
    // Subject must not be empty
    'subject-empty': [2, 'never'],
    // Subject must not end with period
    'subject-full-stop': [2, 'never', '.'],
    // Header max length (type + scope + subject)
    'header-max-length': [2, 'always', 100],
    // Body max line length
    'body-max-line-length': [1, 'always', 100],
    // Scope is optional but if present, must be lowercase
    'scope-case': [2, 'always', 'lower-case'],
  },
  // Common scopes for Nika
  // Usage: feat(tui): add new widget
  //        fix(mcp): handle timeout
  helpUrl: 'https://www.conventionalcommits.org/',
};
