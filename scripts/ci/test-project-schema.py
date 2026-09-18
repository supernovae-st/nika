#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Validate project workflow references and optionally prove real CLI emission."""

import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

import yaml
from jsonschema import Draft202012Validator


ROOT = Path(__file__).resolve().parents[2]
ACCEPTED = (
    "foo.nika", "support.v2.nika", "workflows/nightly.nika", "./foo.nika",
    "../foo.nika", "nested/../foo.nika", "nested//foo.nika", ".hidden.nika",
)
REFUSED = (
    "foo.nika.yaml", "foo.nika.yml", "foo.yaml", "foo.yml", "nika.yaml",
    "foo.NIKA", "foo.Nika", "foo.nika.evil", "foo.nika.minisig",
    "foo.nika.golden.json", "", ".nika", "workflows/.nika", "foo.nika/",
    "foo.nika\\", "/absolute/foo.nika", "workflows\\foo.nika", "C:foo.nika",
    "file:///tmp/foo.nika", "https://example.com/foo.nika", "foo.nika\n",
    "foo\n.nika", "foo\x00.nika", "foo\x7f.nika", "work\x00flows/foo.nika",
)


def project(name):
    return {"nika": "schema-test", "arm": [{
        "workflow": name, "cadence": "TZ=UTC 0 9 * * 1", "plafond": 0.1,
        "manqué": "sauter",
    }]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, help="also compare emission and parser verdicts")
    parser.add_argument("--out", type=Path, help="save the verified emission and its receipt")
    args = parser.parse_args()
    if args.out and not args.binary:
        parser.error("--out requires a real --binary")
    raw = (ROOT / "crates/nika-vocab/src/project.schema.json").read_text()
    schema = json.loads(raw)
    Draft202012Validator.check_schema(schema)
    validator = Draft202012Validator(schema)
    cases = [(name, True) for name in ACCEPTED] + [(name, False) for name in REFUSED]
    for name, accepted in cases:
        errors = list(validator.iter_errors(project(name)))
        assert (not errors) == accepted, (name, errors)
        if not accepted:
            assert len(errors) == 1 and errors[0].validator == "pattern", (name, errors)
            assert list(errors[0].absolute_path) == ["arm", 0, "workflow"]
    assert validator.is_valid({"nika": "schema-test", "traces": {"keep": "30d"}})
    for data in ("nika.yaml", "data.yaml", "data.yml"):
        document = project("workflows/nightly.nika")
        document["arm"][0]["inputs"] = {"source": data}
        assert validator.is_valid(document), data
    print(f"PASS actual project JSON Schema validator: {len(cases)} workflow cases + project/data controls")

    if args.binary:
        binary = args.binary.resolve()
        with tempfile.TemporaryDirectory(prefix="nika-project-schema-") as room:
            env = {"PATH": os.defpath, "HOME": room, "NIKA_KEYCHAIN": "off", "NO_COLOR": "1"}

            def call(*argv):
                return subprocess.run([str(binary), *argv], cwd=room, env=env,
                                      capture_output=True, text=True, timeout=30)

            emitted = call("spec", "--schema", "--project")
            assert emitted.returncode == 0, emitted.stderr
            assert emitted.stdout.rstrip() == raw.rstrip(), "CLI emission differs from vocab owner"
            for name, accepted in cases:
                path = Path(room) / "nika.yaml"
                path.write_text(yaml.safe_dump(project(name), allow_unicode=True))
                checked = call("check", str(path))
                assert (checked.returncode == 0) == accepted, (name, checked.stdout, checked.stderr)
            print(f"PASS real CLI emission and project parser: {len(cases)} workflow cases")
            if args.out:
                args.out.parent.mkdir(parents=True, exist_ok=True)
                args.out.write_text(emitted.stdout)
                version = call("--version")
                assert version.returncode == 0, version.stderr
                receipt = {
                    "source_sha": subprocess.check_output(
                        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                    "binary_version": version.stdout.strip(),
                    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                    "project_schema_sha256": hashlib.sha256(emitted.stdout.encode()).hexdigest(),
                    "workflow_cases": len(cases),
                }
                args.out.with_suffix(".receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
