#!/usr/bin/env bash
# Prove that each digest-addressed Linux image carries the matching tarball binary.
set -euo pipefail

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <image> <digest> <version> <artifacts-dir>" >&2
  exit 64
fi

image="$1"
digest="$2"
version="$3"
artifacts="$4"
[[ "$image" =~ ^[A-Za-z0-9._/-]+$ ]] || {
  echo "oci payload: invalid image: $image" >&2
  exit 64
}
[[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || {
  echo "oci payload: invalid digest: $digest" >&2
  exit 64
}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || {
  echo "oci payload: invalid version: $version" >&2
  exit 64
}
[ -d "$artifacts" ] || {
  echo "oci payload: artifacts directory is missing" >&2
  exit 66
}

scratch="$(mktemp -d)"
containers=()
cleanup() {
  local container
  for container in "${containers[@]}"; do
    docker rm "$container" >/dev/null 2>&1 || true
  done
  rm -r "$scratch"
}
trap cleanup EXIT

for platform in linux/amd64 linux/arm64; do
  case "$platform" in
    linux/amd64) archive_arch=x64 ;;
    linux/arm64) archive_arch=arm64 ;;
  esac
  archive="${artifacts}/nika-linux-${archive_arch}-${version}.tar.gz"
  [ -f "$archive" ] || {
    echo "oci payload: missing archive: $archive" >&2
    exit 66
  }
  mkdir "$scratch/$archive_arch"
  tar -xzf "$archive" -C "$scratch/$archive_arch" nika
  local_hash="$(sha256sum "$scratch/$archive_arch/nika" | awk '{print $1}')"
  docker pull --platform "$platform" "${image}@${digest}" >/dev/null
  container="$(docker create --platform "$platform" "${image}@${digest}")"
  [[ "$container" =~ ^[0-9a-f]{12,64}$ ]] || {
    echo "oci payload: malformed stopped-container id for ${platform}" >&2
    exit 73
  }
  containers+=("$container")
  docker cp "${container}:/usr/local/bin/nika" \
    "$scratch/container-${archive_arch}-nika"
  remote_hash="$(sha256sum "$scratch/container-${archive_arch}-nika" \
    | awk '{print $1}')"
  [ "$remote_hash" = "$local_hash" ] || {
    echo "oci payload: REFUSED binary drift on ${platform}" >&2
    exit 73
  }
done

echo "oci payload: both Linux image binaries match their native tarballs"
