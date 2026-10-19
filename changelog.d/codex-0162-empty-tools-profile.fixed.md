- **Codex authors again on codex-cli 0.162.** Nika admits Codex as an author only on the codex-cli
  minors where its pre-execution empty-tools profile was measured, and that was 0.160 alone: after
  the CLI updated to 0.162.1, every authoring call refused with « codex-cli 0.162 has no measured
  pre-execution empty-tools profile », so the `codex` account could not write anything. The
  profile is now measured on 0.162.1 (every disabled feature reads `false`, a prompt asking to read
  a file, run a command and search the web ran no tool and never saw the file, a JSON-schema answer
  came back in one turn) and that minor is admitted. Any other unmeasured minor still refuses
  before the prompt.
