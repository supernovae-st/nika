- **Release assets rejoin the successful provenance lane explicitly.** The
  deliberately skipped alternative no longer suppresses asset convergence
  or stable-pointer jobs through GitHub's transitive skip rule. Every direct
  prerequisite must still succeed; failed or skipped proofs never authorize
  publication, and prereleases never update stable pointers.
