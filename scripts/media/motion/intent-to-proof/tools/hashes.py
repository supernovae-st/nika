"""Recompute the hashes the film shows (captured/hashes.json).

program_sha256: the real fixture the film draws (scripts/media/fixtures/invoice-payments.nika)
plan_sha256:    the film's semantic plan as data (captured/plan.json)
proposal_blake3: the exact consent preview text shown in the film
"""
import hashlib
import json
import pathlib

import blake3

HERE = pathlib.Path(__file__).resolve().parent.parent
PREVIEW = "1 payment · ACME €120.00 · BRAVO €108.00 · total €228.00 · POST payments.example.invalid · rev 7"
prog = (HERE.parent.parent / "fixtures" / "invoice-payments.nika").read_bytes()
plan = (HERE / "captured" / "plan.json").read_bytes()
out = {
    "program_sha256": hashlib.sha256(prog).hexdigest(),
    "plan_sha256": hashlib.sha256(plan).hexdigest(),
    "proposal_blake3": blake3.blake3(PREVIEW.encode()).hexdigest(),
    "proposal_preview": PREVIEW,
}
(HERE / "captured" / "hashes.json").write_text(json.dumps(out, indent=1, ensure_ascii=False) + "\n")
print(json.dumps(out, indent=1, ensure_ascii=False))
