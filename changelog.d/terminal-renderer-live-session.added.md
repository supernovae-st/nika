- **`nika --tui`: the session behind the terminal renderer (ADR-139 · UX-2).**
  On an interactive terminal, `nika --tui` opens the same native session as
  bare `nika` (the same census and kept intelligence choice, the same
  history, the same runtime) behind the inline viewport: every outcome of a
  turn maps to the renderer's typed beats one to one, the composer's line
  goes to the choice, the consent, the gate or the turn by the state the
  plain loop reads, the first screen is asked through the composer with the
  census's own law. A run keeps the plain path: the shell hands the terminal
  back, `nika run` prints below the viewport as it always has, the
  observation returns into the viewport with a fresh inline anchor. The loop
  is synchronous by design (the run path builds its own executor; one cannot
  start inside another). Proven on the real binary through a PTY: a French
  sentence becomes a proposal, `oui` lands the exact bytes and the real check
  runs, « run it » produces the file, the observation is committed above the
  composer, `/quit` restores the terminal; `--tui` on a pipe keeps the
  concierge and writes no escape sequence. Bare `nika` and its goldens are
  untouched.
