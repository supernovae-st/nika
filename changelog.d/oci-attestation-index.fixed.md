- **OCI release images keep their bound BuildKit attestations.** The release
  barrier distinguishes its two Linux images from their two metadata
  descriptors. It retains provenance and
  refuses foreign or duplicate subjects, missing metadata, extra platforms,
  malformed descriptors and multiple JSON documents before publication.
