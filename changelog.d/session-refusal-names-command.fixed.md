- **A Session line the host cannot parse is refused naming its command.**
  Both doors refuse such a line `malformed` naming the command identity its
  JSON carries, when a valid one, so a native client tells the refusal of
  its own line from another's instead of taking it for a reply to a read; a
  line naming none, or an invalid one, is still refused naming none.
