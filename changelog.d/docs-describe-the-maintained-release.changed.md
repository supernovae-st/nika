- **The repository's documents describe the released 0.123.0 and its maintenance.** The README,
  `llms.txt`, the Session guide, CONTRIBUTING, SECURITY, ROADMAP and DIAMOND say that 0.123 is
  maintained with security fixes, bug fixes and compatibility, with no hosted service and no 1.0
  launch planned; SECURITY's code table lists only codes `nika explain` resolves, and it names the
  MCP pin codes it does not. The serve operations page gives a first run that works (a project
  `nika.yaml` it keeps when one exists, a served rehearsal workflow, a health and job request, the
  served cancel route), `llms-install.md` adds the container and provenance paths and an
  uninstall that names what `~/.nika` holds, and the README and the Session guide say that runs are
  signed once `nika key init` minted a key. The README says what the TypeScript SDK's server door
  changes (`allowInsecureHttp` for plain loopback HTTP, an `idempotencyKey` and no `maxCostUsd`),
  and the Session guide lists the routes a Run admits. The README clips are captured again from
  0.123.0, and a new `sdk-doors` clip shows the SDK's two doors (the bundled engine and
  `nika serve`, with the two lines whose options change) from a capture of the published
  `@supernovae-st/nika` 0.123.0.
