#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise native scaffolding, filled lessons and adversarial authoring cases.

Uses only mock inference and disposable local fixtures. --source-only judges
the same source workflows with a released binary whose embedded shelf predates
them. A native qualification must also run without that flag.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

import yaml


class WorkflowLoader(yaml.SafeLoader):
    """Nika dates are strings; PyYAML's implicit timestamp is not language data."""


WorkflowLoader.yaml_implicit_resolvers = {
    key: [(tag, pattern) for tag, pattern in entries
          if tag != "tag:yaml.org,2002:timestamp"]
    for key, entries in yaml.SafeLoader.yaml_implicit_resolvers.items()
}


def load_workflow(text):
    return yaml.load(text, Loader=WorkflowLoader)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--pack", type=Path, default=Path(__file__).resolve().parents[1] / "crates/nika-pack/pack")
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--source-only", action="store_true")
    args = parser.parse_args()
    binary, pack, out = args.binary.resolve(), args.pack.resolve(), args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    templates = pack / "templates"
    rehearsals = load_workflow((templates / "rehearsals.yaml").read_text())["rehearsals"]
    pairs = {row["template"]: row for row in rehearsals}
    results = []

    with tempfile.TemporaryDirectory(prefix="nika-authoring-gauntlet-") as temp:
        root = Path(temp)
        home = root / "home"
        home.mkdir()
        env = {"HOME": str(home), "PATH": os.defpath, "NIKA_KEYCHAIN": "off", "NO_COLOR": "1", "TERM": "dumb"}

        def run(case, argv, cwd, expected=0, diagnostic=None, stdin=None):
            proc = subprocess.run([str(binary), *argv], cwd=cwd, env=env,
                                  input=stdin, capture_output=True, text=True, timeout=90)
            (out / f"{case}.stdout").write_text(proc.stdout)
            (out / f"{case}.stderr").write_text(proc.stderr)
            passed = proc.returncode == expected and (diagnostic is None or diagnostic in proc.stdout + proc.stderr)
            row = {"case": case, "exit": proc.returncode, "expected": expected, "passed": passed}
            results.append(row)
            return proc, row

        def require(row, condition, reason):
            if not condition:
                row["passed"] = False
                row.setdefault("failures", []).append(reason)

        for source in sorted(templates.glob("*.nika")):
            name = source.name.removesuffix(".nika")
            directory = root / (name + " project")
            directory.mkdir()
            path = directory / "workflow with spaces.nika"
            preview = None
            body = source.read_text()
            value_slots = any("<SLOT:" in line and ("#" not in line or line.index("<SLOT:") < line.index("#"))
                              for line in body.splitlines())
            if args.source_only:
                shutil.copyfile(source, path)
            else:
                proc, row = run(name + "-preview", ["compile", name, "--json"], directory,
                                2 if value_slots else 0)
                preview = json.loads(proc.stdout)
                require(row, preview.get("status") == ("incomplete" if value_slots else "ready"),
                        "preview completeness disagrees with the committed skeleton")
                require(row, preview.get("written") is None and not path.exists(), "preview wrote a file")
                require(row, isinstance(preview.get("candidate"), str), "missing structured candidate")
                if not isinstance(preview.get("candidate"), str):
                    continue
                body = preview["candidate"]
                require(row, load_workflow(body) == load_workflow(source.read_text()),
                        "preview changed the skeleton semantics")
                if value_slots:
                    proc, row = run(name + "-incomplete-no-write", ["compile", name, path.name, "--json"], directory, 2)
                    require(row, json.loads(proc.stdout).get("written") is None and not path.exists(),
                            "an incomplete candidate was materialized")
            # The harness may inspect unfilled source; the product only writes Ready.
            unfilled = directory / "unfilled.nika"
            unfilled.write_text(body)
            run(name + "-unfilled", ["check", unfilled.name, "--json", "--model", "mock/echo"], directory, 2 if value_slots else 0)
            fill = pairs[name]["fill"] if name in pairs else "answered by the committed corpus golden"
            # Scalar values are filled structurally so quotes and backslashes
            # cannot alter the workflow graph or permits block.
            def fill_values(value):
                if isinstance(value, str):
                    return re.sub(r"<SLOT:[^>]*>", lambda _: fill, value)
                if isinstance(value, list):
                    return [fill_values(v) for v in value]
                if isinstance(value, dict):
                    return {k: fill_values(v) for k, v in value.items()}
                return value
            filled = fill_values(load_workflow(body))
            if args.source_only:
                path.write_text(yaml.safe_dump(filled, sort_keys=False))
            else:
                command = ["compile", name, path.name, "--json"]
                for question in preview["questions"]:
                    command += ["--answer", question["key"] + "=" + json.dumps(fill)]
                proc, row = run(name + "-compile", command, directory)
                receipt = json.loads(proc.stdout)
                require(row, receipt.get("status") == "ready" and receipt.get("written") == path.name,
                        "answered Compile did not produce a Ready write receipt")
                require(row, path.is_file(), "Ready Compile did not write its explicit destination")
                if not path.is_file():
                    continue
                filled["nika"] = "workflow-with-spaces"
                require(row, load_workflow(path.read_text()) == filled,
                        "answered Compile changed more than the supplied slots and explicit identity")
                before = path.read_bytes()
                _, row = run(name + "-no-overwrite", command, directory, 3)
                require(row, path.read_bytes() == before, "refusal changed the existing file")
            run(name + "-filled", ["check", path.name, "--json", "--native-strict", "--model", "mock/echo"], directory)
            shutil.copyfile(templates / (source.name + ".golden.json"), Path(str(path) + ".golden.json"))
            test_args = ["test", path.name]
            if name in ("etl-state", "human-gated-ship"):
                run(name + "-unattended", test_args, directory, 1, "NIKA-BUILTIN-PROMPT-001")
                test_args += ["--answer", "approve=false" if name == "etl-state" else "human=false"]
            run(name + "-golden", test_args, directory)
            negative = templates / (name + ".negative.yaml")
            match = re.search(r"^# Expected · (NIKA-[^\s.]+)", negative.read_text(), re.M)
            if not match:
                raise ValueError(f"missing exact negative diagnostic: {negative}")
            staged = directory / f"{name}.nika"
            shutil.copyfile(negative, staged)
            run(name + "-negative", ["check", staged.name, "--json", "--model", "mock/echo"], directory, 2, match[1])

        def execute_case(case, body, expected=None, blocked=(), files=None):
            directory = root / case
            directory.mkdir()
            for name, content in (files or {}).items():
                (directory / name).write_text(content)
            path = directory / "workflow.nika"
            path.write_text(yaml.safe_dump(body, sort_keys=False))
            proc, row = run(case, ["run", path.name, "--json", "--model", "mock/echo"], directory, 1 if blocked else 0)
            events = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
            settled = [event for event in events if event.get("kind") == "run_settled"]
            require(row, len(settled) == 1, "missing unique run_settled")
            if expected is not None:
                require(row, bool(settled) and settled[0].get("outputs") == expected, "typed outputs differ from expected values")
            if blocked:
                started = {field["value"] for event in events if event.get("kind") == "task_started"
                           for field in event.get("fields", []) if field["key"] == "task"}
                require(row, not set(blocked) & started, "rejected input started downstream work")
                require(row, bool(started), "trace has no task_started evidence to judge")
                require(row, bool(settled) and settled[0].get("status") == "failed", "rejection did not fail the run")
                require(row, "NIKA-BUILTIN-ASSERT-001" in proc.stdout + proc.stderr, "rejection did not originate at an assertion")

        for pair in rehearsals:
            example = pair["example"]
            source = pack / "examples" / (example + ".nika")
            body = load_workflow(source.read_text())
            if not args.source_only:
                directory = root / (example + "-take")
                directory.mkdir()
                # Filled lessons are discovery, not another creation compiler.
                # Read the actual embedded source through the public MCP tool;
                # execution below chooses mock explicitly without changing it.
                request = {"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                           "params": {"name": "nika_template", "arguments":
                                      {"name": pair["template"], "filled": True}}}
                proc, row = run(example + "-discover", ["mcp"], directory,
                                stdin=json.dumps(request) + "\n")
                replies = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
                result = next((reply.get("result", {}) for reply in replies if reply.get("id") == 1), {})
                text = "".join(block.get("text", "") for block in result.get("content", []))
                require(row, result.get("isError") is False and text == source.read_text(),
                        "native lesson discovery changed committed source")
                if text:
                    body = load_workflow(text)
            expected = json.loads((templates / (pair["template"] + ".nika.golden.json")).read_text())
            execute_case(example + "-run", body, expected)

        for case in json.loads((templates / "rehearsal-cases.json").read_text())["cases"]:
            pair = pairs[case["template"]]
            body = load_workflow((pack / "examples" / (pair["example"] + ".nika")).read_text())
            body["const"].update(copy.deepcopy(case["const"]))
            execute_case(case["template"] + "-" + case["case"], body,
                         case.get("outputs"), case.get("blocked", ()), case.get("files"))

        if not args.source_only:
            # Judge the actual stdio transport, not only the Rust handler.
            requests = [{"jsonrpc": "2.0", "id": 0, "method": "tools/list"}]
            expectations = {}
            for pair in rehearsals:
                for filled in (False, True):
                    identifier = len(requests)
                    path = (pack / "examples" / (pair["example"] + ".nika") if filled else
                            templates / (pair["template"] + ".nika"))
                    arguments = {"name": pair["template"], "filled": filled}
                    requests.append({"jsonrpc": "2.0", "id": identifier, "method": "tools/call",
                                     "params": {"name": "nika_template", "arguments": arguments}})
                    expectations[identifier] = (f"mcp-{pair['template']}-{'filled' if filled else 'skeleton'}", path.read_text(), False)
            for name, arguments in [("nika_template", {"filled": True}),
                                    ("nika_template", {"name": "../bounded-batch", "filled": True}),
                                    ("nika_template", {"name": "chain", "filled": True}),
                                    ("nika_examples", {"slug": "do stuff with things"})]:
                identifier = len(requests)
                requests.append({"jsonrpc": "2.0", "id": identifier, "method": "tools/call",
                                 "params": {"name": name, "arguments": arguments}})
                expectations[identifier] = (f"mcp-refusal-{identifier}", None, True)
            proc, transport = run("mcp-stdio", ["mcp"], root,
                                  stdin="".join(json.dumps(request) + "\n" for request in requests))
            replies = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
            require(transport, len(replies) == len(requests), "MCP response count differs from requests")
            by_id = {reply.get("id"): reply for reply in replies}
            catalog = by_id.get(0, {}).get("result", {}).get("tools", [])
            tool = next((tool for tool in catalog if tool["name"] == "nika_template"), {})
            require(transport, tool.get("inputSchema", {}).get("properties", {}).get("filled", {}).get("type") == "boolean",
                    "MCP did not advertise the filled view")
            for identifier, (case, source, is_error) in expectations.items():
                result = by_id.get(identifier, {}).get("result", {})
                text = "".join(block.get("text", "") for block in result.get("content", []))
                results.append({"case": case, "passed": result.get("isError") == is_error and
                                (bool(text) if source is None else text == source)})

    summary = {"binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "source_only": args.source_only,
               "passed": sum(r["passed"] for r in results), "failed": sum(not r["passed"] for r in results), "cases": results}
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({k: v for k, v in summary.items() if k != "cases"}))
    for row in results:
        if not row["passed"]:
            print(json.dumps(row))
    return int(summary["failed"] != 0)


if __name__ == "__main__":
    raise SystemExit(main())
