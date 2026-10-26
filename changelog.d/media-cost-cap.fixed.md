- **`--max-cost-usd` and the run settlement now see paid media spend.**
  `nika:image_generate` on openai or gemini and `nika:tts_generate` on
  openai or elevenlabs bill a provider the catalog cannot price, yet they ran
  under any cap and the run settled `priced` with `unpriced_calls: 0`. Under
  a cap such a call now refuses before any request, NIKA-1709 at admission
  (NIKA-1704 before its own request when only the run decides the
  provider), the same refusal an unpriced cloud model gets. Without a cap it
  runs and settles `unpriced`. xAI images keep their catalog rate and are
  metered against the cap; local, mock and keyless media stay unmetered.
  `nika check` names each paid media call on its COST rung, priced per call
  or UNPRICED, and lists the cap refusal as `unpriced_media_cap` in
  `failure_plan`.
