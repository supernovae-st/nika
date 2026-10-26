- **Capped runs refuse paid media the catalog cannot price, beats included.**
  A beat always runs under its mandatory `plafond:`, and a plain `nika run`
  in a project with `ceiling:` runs under that cap. Both now refuse openai or
  gemini images and openai or elevenlabs speech before any request
  (`nika arm` shows the beat UNREADY with NIKA-1709), where 0.123.2 ran and
  billed them with no bound. Use `xai` for images (no speech provider has a
  catalog price yet), or `local` / `mock`; only a run with no cap at all
  makes such a call.
