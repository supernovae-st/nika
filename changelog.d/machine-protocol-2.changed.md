- **Machine protocol 2.** This engine reports `machineProtocolVersion: 2` on `/health` and
  `nika --sdk-identity`.
  - **Why.** Compile questions may be `choice`, which `@supernovae-st/nika` 0.120.3 refuses
    mid-compile. From now on, any change to an object a published SDK reads closed raises the
    protocol in the same commit, held by a contract test against that SDK's closed sets. That SDK
    then refuses this engine at connect (`NikaCompatibilityError`), before any job is admitted,
    never in the middle of a run. Use the SDK of the same release. It reads protocols 1 and 2, so
    upgrade the SDK first, then the engine.
  - **Resident stores.** A resident stamps its stores with the protocol once it holds the lease,
    so stores a 0.123 resident wrote are refused by an older resident, by design. Before
    upgrading, copy the state root (by default `.nika/serve`). To roll back, serve that copy, or
    a fresh `--state-root`.
  - **Doctor.** Its resident row fails for stores a newer protocol wrote, whether a resident runs
    or not, and names those two ways out. It used to read a stopped newer resident's stores as
    fine and suggest a restart this binary would refuse.
