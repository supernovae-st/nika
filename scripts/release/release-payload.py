#!/usr/bin/env python3
"""Select exact release bytes; checksums are integrity, not signature proof.

The draft owner supplies replay bytes by asset ID. The read-only workflow job
must verify their existing attestations before exposing the selected artifact.
"""

import argparse
import hashlib
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


def native_names(tag):
    return [f"nika-{platform}-{tag[1:]}.tar.gz" for platform in
            ("macos-arm64", "macos-x64", "linux-arm64", "linux-x64")]


def payload_names(tag):
    npm = f"supernovae-st-nika-check-wasm-{tag[1:]}.tgz"
    return [*native_names(tag), "SHA256SUMS", npm, npm + ".sha256"]


def regular(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size == 0:
        raise ValueError(f"missing, empty or non-regular payload: {path.name}")
    return path


def verify_checksums(directory, manifest, expected):
    records = {}
    for line in regular(directory / manifest).read_text().splitlines():
        match = re.fullmatch(r"([0-9a-fA-F]{64}) [ *]([^/\\]+)", line)
        if not match or match[2] in records:
            raise ValueError(f"malformed or duplicate checksum in {manifest}")
        records[match[2]] = match[1].lower()
    if set(records) != set(expected):
        raise ValueError(f"checksum names differ from the exact payload in {manifest}")
    for name, digest in records.items():
        with regular(directory / name).open("rb") as source:
            actual = hashlib.file_digest(source, "sha256").hexdigest()
        if actual != digest:
            raise ValueError(f"checksum mismatch: {name}")


def validate(tag, directory):
    names = payload_names(tag)
    if directory.is_symlink() or not directory.is_dir():
        raise ValueError("payload directory is missing or symlinked")
    if {p.name for p in directory.iterdir()} != set(names):
        raise ValueError("expected exactly seven payload files")
    for name in names:
        regular(directory / name)
    verify_checksums(directory, "SHA256SUMS", native_names(tag))
    verify_checksums(directory, names[-1], [names[-2]])


def inventory(tag, path):
    """Pin all eight release asset names/IDs, including the separate statement."""
    records = {}
    ids = set()
    for line in path.read_text().splitlines():
        parts = line.split("\t")
        if len(parts) != 2 or not re.fullmatch(r"[1-9][0-9]*", parts[0]):
            raise ValueError("malformed release asset inventory")
        asset_id, name = parts
        if name in records or asset_id in ids:
            raise ValueError("duplicate release asset identity")
        records[name] = asset_id
        ids.add(asset_id)
    if set(records) != {*payload_names(tag), "multiple.intoto.jsonl"}:
        raise ValueError("missing or extra original release assets; replay cannot rebuild them")
    for name in sorted(records):
        print(f"{records[name]}\t{name}")


def select(event, tag, native, npm, replay, output):
    if output.exists() or output.is_symlink():
        raise ValueError("payload output already exists")
    if event not in ("push", "workflow_dispatch"):
        raise ValueError(f"unsupported payload source event: {event}")
    names = payload_names(tag)
    # Copy without unpacking/repacking: archives, manifests and npm sidecars
    # retain their exact signed bytes. A replay NEVER consults rebuilt files.
    with tempfile.TemporaryDirectory(prefix="release-payload-", dir=output.parent) as tmp:
        staged = Path(tmp)
        for name in names:
            source = replay if event == "workflow_dispatch" else (
                npm if name in names[-2:] else native)
            shutil.copyfile(regular(source / name), staged / name)
        validate(tag, staged)
        # copytree reserves the destination with mkdir: an existing/raced path
        # is refused, never replaced, including an empty directory or symlink.
        shutil.copytree(staged, output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for command in ("validate", "inventory"):
        sub = commands.add_parser(command)
        sub.add_argument("tag")
        sub.add_argument("path", type=Path)
    sub = commands.add_parser("select")
    sub.add_argument("event")
    sub.add_argument("tag")
    for name in ("native", "npm", "replay", "output"):
        sub.add_argument(name, type=Path)
    args = parser.parse_args()
    subprocess.run(["bash", str(Path(__file__).with_name("check-release-tag.sh")), args.tag],
                   check=True)
    if args.command == "select":
        select(args.event, args.tag, args.native, args.npm, args.replay, args.output)
    elif args.command == "validate":
        validate(args.tag, args.path)
    else:
        inventory(args.tag, args.path)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"release payload: REFUSED {error}", file=sys.stderr)
        sys.exit(73)
