- **`--max-cost-usd` and the run settlement now see paid media spend.**
  `nika:image_generate` on openai or gemini and `nika:tts_generate` on
  openai or elevenlabs bill a provider the catalog cannot price, yet they ran
  under any cap, and the settlement left them out: a run whose only spend was
  such a call settled `unmetered`, and one beside a priced model settled
  `priced`. Under a cap such a call now refuses before any request: NIKA-1709
  at admission (its own task or a composed child's), NIKA-1704 before its own
  request when only the run decides the provider or when an `agent:` calls it
  through its tools (the refusal is fed back to the model). Without a cap it
  runs and is counted in `spend.unpriced_calls`, so the settlement reads
  `unpriced` or `partially_priced`; a child workflow's unpriced calls now
  reach its parent's settlement. xAI images keep their catalog rate and are
  metered against the cap; local and mock media stay unmetered, and the
  keyless `system` voice runs under any cap (an `invoke:` task's call still
  settles as unpriced local work, `local_model`).
  `nika check` names each paid media call on its COST rung (priced per call,
  UNPRICED, or DECIDED AT RUN), including a composed child's and the paid
  media an agent's tools can reach, and lists the cap refusal as
  `unpriced_media_cap` in `failure_plan`.
