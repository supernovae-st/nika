- **The language-step cap is sized from the catalog on a reasoning seat.**
  A structured draft on `openai/gpt-5-mini` died on `NIKA-INFER-002` because
  the reasoning trace ate a `max_tokens: 1200` cap before any answer was
  visible. The assembler knows the seat at Ready and reads the catalog
  there: a catalog-known reasoning seat gets at least 4096 on every language
  step (validate · extract · classify · per-record classify · draft ·
  per-item draft); any other seat, and `mock`, keeps the step's own cap. A
  cap is a ceiling the run never exceeds, never a spend.
