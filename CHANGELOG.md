# Changelog

All notable changes to Nika will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Nothing yet

## [0.1.0] - Unreleased

### Added
- Initial release of Nika CLI
- 5 semantic verbs: `agent:`, `exec:`, `fetch:`, `invoke:`, `infer:`
- 4 scope presets: `minimal`, `default`, `debug`, `full`
- DAG-based workflow execution with fan-out/fan-in
- 7-layer validation pipeline
- JSONPath support for task outputs
- Template system with `${...}` syntax
- Event sourcing for workflow audit trail
- Multi-provider support (Anthropic, OpenAI, Google, Groq, Local)

### Technical
- Single-pass tokenization (85% faster)
- SmartString inline storage for strings ≤31 chars (93% faster)
- RuntimeContext with Arc<str> (96% faster)
- DataStore with ScopedAccessor for thread-safe context

[Unreleased]: https://github.com/supernovae-studio/nika/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/supernovae-studio/nika/releases/tag/v0.1.0
