- **Draw the session renderer's own glyphs in ASCII under `--ascii`.**
  The terminal renderer of bare `nika` now honours the theme's ASCII glyph column, as the CLI
  frames already did: its block faces, its loader, its prompt markers, the focus rule and the
  separators of its own hints take their ASCII twins. The Session's own words are shown as
  written.
