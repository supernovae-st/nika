- **The ecosystem coherence bot reads live and served surfaces.** It compared
  the retired `supernovae.nika-lang` editor listing (frozen at 0.116.3) instead
  of the live `supernovae.nika`, and the site rows read raw files of a
  repository that is no longer publicly readable, reported as a permanent WARN.
  The bot now queries the live extension identity on both registries, reads the
  deployed site's engine version from `https://nika.sh/llms.txt` and its spec
  pin from the served well-known document, and a tag-pin or immune surface that
  stays unreadable past the 24-hour cascade window is a FAIL instead of a
  permanent WARN.
