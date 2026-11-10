- **A refused route declaration no longer suggests a pin that cannot help.**
  When a run is refused before task 1 (`NIKA-1800`) and this machine has
  a signed-in seat, `nika run` adds "or use a seat present on this
  machine: `--access <seat>`". It also did so when the refusal was about
  the file's own `run.access` declaration, for example a `service_tier`
  its route cannot carry. Under a declared route, another seat is refused
  by that declaration and the same seat changes nothing. The line is now
  printed only when the refusal is about what the machine lacks, such as
  a missing credential.
