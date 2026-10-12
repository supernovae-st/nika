- **A development build no longer asks for the keychain password on every run.** A macOS
  keychain grant (« Always Allow ») is bound to the signature of the binary that asked. A
  Developer ID release keeps the same signature from one version to the next, so its grant
  sticks; an ad-hoc build (every `cargo build`, every dev build) is a new identity each time, so
  the password prompt came back on every run. Now only a binary whose signature can keep a grant
  opens the keychain for its run-key; any other takes the explicit `NIKA_RUN_KEY_FILE` pair or
  the 0600 files under `~/.nika/keys`, silently, and runs unsigned when there is none, never
  under a new key. `nika serve` never opens the keychain. A secret key file other users can read
  is refused, as ssh refuses one. `nika doctor` says in one `run-key` line where the key comes
  from, the fingerprint of the key this process would sign with, and why the keychain is or is
  not part of it. A machine whose keychain holds the key a signed release signs with, beside a
  0600 pair dev builds and `nika serve` sign with, has two identities: a release's doctor names
  both keys, and `nika key align`, run from a signed release, gives the 0600 files the keychain
  key and retires the one they held (the journals it sealed stay verifiable). At init, rotate and
  align a signed release records the keychain key's fingerprint and the item's modification date
  in `~/.nika/keys/keychain.json`; a dev build compares through that record only while the item
  is unchanged, and otherwise says it does not know. `nika key init` in a process that does not
  open the keychain refuses to mint a second key beside the keychain's, unless forced. Even a
  signed release now opens the secret keychain item only to sign or to align: `nika key trust`,
  `nika doctor`, the memory fold, a seal grade and a workflow signature read the public item
  alone, and each item is read at most once per process. `nika key init` sees whether the keychain
  holds a key from the items' attributes, never their data. `NIKA_KEYCHAIN=off` still turns the
  keychain off everywhere.
