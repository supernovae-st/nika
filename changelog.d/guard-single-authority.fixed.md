- **The guard has one scope judge.** The shell shim no longer guesses
  command ownership or hook dialect from raw substrings, which escaped
  commands and decoy markers could defeat. Missing, broken or unavailable
  judges block the hook action, including ordinary commands until repaired;
  unsupported payload shapes are integration errors rather than no opinion.
  Stdin reaches the engine's bounded reader without a shell-side copy.
