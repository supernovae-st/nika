- **Trace verification bounds its custody, anchor and lease reads.** Reads
  use held regular-file descriptors and refuse final symlinks. Unavailable
  side inputs remain distinct from absent or malformed ones; an unreadable
  anchor reports UNAVAILABLE, never a claim of forgery. The selected parent
  may resolve through a link; contained project reads still refuse child links.
