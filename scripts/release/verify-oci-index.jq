# BuildKit stores provenance beside executable images as unknown/unknown
# attestation-manifest descriptors bound to their image digest. They are not
# additional runnable platforms. This judges index structure, not signatures.
# https://docs.docker.com/build/metadata/attestations/attestation-storage/
def attestation:
  .annotations["vnd.docker.reference.type"] == "attestation-manifest";
def descriptor:
  type == "object" and
  .mediaType == "application/vnd.oci.image.manifest.v1+json" and
  (.digest | type == "string" and test("^sha256:[0-9a-f]{64}$")) and
  (.size | type == "number" and . > 0 and floor == .);

def release_index:
.schemaVersion == 2 and
.mediaType == "application/vnd.oci.image.index.v1+json" and
(.manifests | type == "array" and length == 4) and
(.manifests as $all |
  all($all[]; descriptor) and
  ([$all[].digest] | unique | length == 4) and
  ([$all[] | select(attestation | not)] as $images |
   [$all[] | select(attestation)] as $proofs |
    ([$images[].platform | "\(.os)/\(.architecture)"] | sort) ==
      ["linux/amd64", "linux/arm64"] and
    ($proofs | length == 2) and
    all($proofs[]; .platform.os == "unknown" and .platform.architecture == "unknown") and
    ([$proofs[].annotations["vnd.docker.reference.digest"]] | sort) ==
      ([$images[].digest] | sort)));

# Invoked with --slurp: an extra JSON value cannot hide an earlier refusal.
length == 1 and (.[0] | release_index)
