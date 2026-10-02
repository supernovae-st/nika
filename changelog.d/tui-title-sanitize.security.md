- **Keep control characters out of the terminal title.** Bare `nika` names the
  terminal window after the project directory; an escape or bell byte in that
  name could end the title sequence and start one of its own (a clipboard write
  under tmux). Control characters (C0, DEL, C1) are now dropped before the title
  is set.
