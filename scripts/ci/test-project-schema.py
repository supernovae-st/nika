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
    "\u0345://foo.nika", "\u2160://foo.nika", "a\u0345://foo.nika", "é://foo.nika",
)


def project(name):
    return {"nika": "schema-test", "arm": [{
        "workflow": name, "cadence": "TZ=UTC 0 9 * * 1", "plafond": 0.1,
        "manqué": "sauter",
    }]}


def assert_project_verdict(checked, name, accepted, path):
    evidence = (name, checked.returncode, checked.stdout, checked.stderr)
    assert checked.returncode == (0 if accepted else 2), evidence
    verdict = json.loads(checked.stdout)
    assert verdict["report_version"] == 1 and verdict["kind"] == "project", evidence
    assert verdict["file"] == str(path) and verdict["clean"] is accepted, evidence
    findings = verdict["findings"]
    if accepted:
        assert findings == [], evidence
    else:
        assert len(findings) == 1, evidence
        assert findings[0]["code"] == "project.bad-value", evidence
        assert findings[0]["message"].startswith(f"`workflow: {name}`"), evidence
        assert "a `*.nika` path relative to the registry" in findings[0]["message"], evidence


def check_verdict_judge():
    """A process failure or an unrelated finding must never prove refusal."""
    path, name = Path("nika.yaml"), "invalid.yaml"
    good = {"report_version": 1, "kind": "project", "file": str(path), "clean": False,
            "findings": [{"code": "project.bad-value",
                          "message": f"`workflow: {name}` — a `*.nika` path relative to the registry"}]}

    def result(code, payload):
        return subprocess.CompletedProcess([], code, json.dumps(payload), "")

    assert_project_verdict(result(2, good), name, False, path)
    bad = [result(code, good) for code in (-11, 127, 1, 0)]
    bad.append(result(2, {**good, "findings": [{"code": "project.unknown-key", "message": "unrelated"}]}))
    bad.append(result(2, {**good, "findings": [{"code": "project.bad-value", "message": "`cadence: invalid`"}]}))
    bad.append(subprocess.CompletedProcess([], 2, "INJECTED unrelated failure", ""))
    for checked in bad:
        try:
            assert_project_verdict(checked, name, False, path)
        except (AssertionError, json.JSONDecodeError):
            continue
        raise AssertionError(f"unrelated failure accepted: {checked}")
    print(f"PASS native verdict judge refuses {len(bad)} crash/exit/diagnostic mutations")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, help="also compare emission and parser verdicts")
    parser.add_argument("--out", type=Path, help="save the verified emission and its receipt")
    args = parser.parse_args()
    if args.out and not args.binary:
        parser.error("--out requires a real --binary")
    check_verdict_judge()
    raw = (ROOT / "crates/nika-vocab/src/project.schema.json").read_text()
    packed = (ROOT / "crates/nika-pack/pack/schemas/project.schema.json").read_text()
    assert raw == packed, "embedded Spec projection differs from engine vocab owner"
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
                checked = call("check", str(path), "--json")
                assert_project_verdict(checked, name, accepted, path)
            print(f"PASS real CLI emission and project parser: {len(cases)} workflow cases")
            if args.out:
                changed = subprocess.check_output(
                    ["git", "diff", "HEAD", "--name-only"], cwd=ROOT, text=True).strip()
                assert not changed, f"emission receipt requires unchanged tracked source: {changed}"
                args.out.parent.mkdir(parents=True, exist_ok=True)
                args.out.write_text(emitted.stdout)
                version = call("--version")
                assert version.returncode == 0, version.stderr
                receipt = {
                    "source_sha": subprocess.check_output(
                        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                    "source_tree_sha": subprocess.check_output(
                        ["git", "rev-parse", "HEAD^{tree}"], cwd=ROOT, text=True).strip(),
                    "tracked_source_clean": True,
                    "binary_version": version.stdout.strip(),
                    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                    "project_schema_sha256": hashlib.sha256(emitted.stdout.encode()).hexdigest(),
                    "workflow_cases": len(cases),
                }
                args.out.with_suffix(".receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
