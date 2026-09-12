- **Offline text-only agent probes.** `mock/text` returns deterministic text
  even when tools are offered, allowing completion and budget refusal paths
  to be exercised without a paid provider. `mock/echo` keeps its existing
  tool-call behavior.
