- **`nika check` refuses a `temperature:` on `openai/gpt-5`, `gpt-5-mini`,
  `gpt-5-nano` and the o-series.** These models accept only the default
  temperature, and OpenAI answers any other value with HTTP 400 at run
  (« Only the default (1) value is supported »), while check showed
  `capacity fit ✔`. The catalog now records that GPT-5, GPT-5 mini and
  GPT-5 nano (and their 2025-08-07 snapshots) take no temperature, as it
  already did for the o-series, and the fixed-sampling finding covers
  OpenAI's own API beside Anthropic's for the models the catalog lists. A
  Session label call on these models, a dated snapshot included, no longer
  sends a temperature. The finding does not yet name a dated snapshot id
  (`gpt-5-mini-2025-08-07`), which the catalog's model list does not carry.
  OpenRouter and other compatible routes, which drop the control, and the
  later dot variants (`gpt-5.1` onward) are unchanged.
