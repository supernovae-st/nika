- **Validate public custody before disclosure and preserve existing keys.**
  Trust output and rotation now reconstruct decoded public keys, dropping
  untrusted comments and trailing bytes. Signing also binds the public key
  and its key number to the opened secret. Broken explicit custody refuses
  fallback; non-forced initialization preserves corrupt or orphaned files
  and refuses concurrent writers. Known path aliases cannot collapse the
  two key slots. One guarded keyring constructor and one public-box decoder
  serve signing, trace and evidence readers. Retired records keep their
  historical fingerprints; older imported custom-comment seals require
  their original public enrollment record after canonical retirement.
  Engine-generated public boxes are unchanged. File pairs are not atomic.
