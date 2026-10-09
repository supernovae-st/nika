- **A path in a reply is no longer corrected as an unknown model.** The
  reply guard judged any slash-shaped token whose prefix was not on a
  folder list, so `news/2026-10-09.md` read as provider `news`. It now
  judges a `provider/name` only in a model slot — a `model:` field, a
  `--model` flag, or a name the prose calls a model — and there with
  the same words whatever the name looks like; the folder list is gone.
