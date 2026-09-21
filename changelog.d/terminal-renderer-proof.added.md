- **The terminal renderer proof (ADR-139 · UX-1 · `nika-tui`).** The
  session's future face joins the workspace as a WIP L4 crate: one owner of
  the terminal (raw mode, bracketed paste, focus events, the probed keyboard
  protocol and the alternate screen enabled in one fixed order and restored
  in reverse from one place, the panic hook restoring BEFORE the message
  prints), an inline-first presentation whose finished blocks go into the
  terminal's own scrollback through `insert_before` with scrolling regions
  (history stays copyable, tmux and SSH stay ordinary), a focus presentation
  on the alternate screen entered with `Ctrl+T` and left with `Esc` and the
  draft intact, one event broker paused around every cursor-position query,
  and a composer (`ratatui-textarea` behind a wrapper) where `Enter` sends,
  `Alt+Enter` breaks a line, a bracketed paste is data (a pasted `yes` or
  `/quit` acts on nothing) and history recalls only at the buffer's edges.
  The `nika-tui-proto` binary drives the shell over a canned conversation
  in both presentations; the PTY suite proves the terminal is restored on
  the normal close, two `Ctrl+C`, a panic inside the loop and `SIGTERM`,
  and that a pipe is refused with exit 2 and no escape sequence. Bare
  `nika` is untouched: the plain line loop and its goldens stay byte for
  byte until the next wave wires the session through the same typed beats.
