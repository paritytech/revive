#!/usr/bin/env python3
"""Bundles a compilation's lowered stages into what the viewer fetches.

One file per chain holding every stage's CFG, plus an index naming the chains
and marking which passes actually changed the IR — the manifest's `ir_hash` is
equal across adjacent stages of one object when a pass left it untouched, and
most passes do.
"""

import json
import sys
from pathlib import Path

from lower import lower_stage

OUTPUT = Path(__file__).parent / "data"


def bundle(stages_directory):
    directory = Path(stages_directory)
    manifest_path = next(directory.glob("*.stages.json"))
    manifest = json.loads(manifest_path.read_text())
    chain_id = manifest_path.name.removesuffix(".stages.json")

    stages, index = {}, []
    previous = None
    for entry in manifest["stages"]:
        record = directory / entry["file"]
        stages[entry["id"]] = lower_stage(record)
        changed = previous is not None and previous[0] == entry["object"] and previous[1] != entry["ir_hash"]
        index.append({
            "id": entry["id"], "pass": entry["pass"], "object": entry["object"],
            "changed": changed, "first": previous is None or previous[0] != entry["object"],
        })
        previous = (entry["object"], entry["ir_hash"])

    OUTPUT.mkdir(exist_ok=True)
    path = OUTPUT / f"{chain_id}.json"
    path.write_text(json.dumps(stages, separators=(",", ":")))
    return {
        "id": chain_id, "contract": manifest["contract"],
        "heap_size": manifest["heap_size"], "file": path.name, "stages": index,
    }, path.stat().st_size


# The index merges rather than being rewritten from the arguments alone.
# Rebuilding one contract is the common case, and dropping every other chain
# out of the picker is never what that was meant to do.
existing = {}
index_path = OUTPUT / "index.json"
if index_path.exists():
    for entry in json.loads(index_path.read_text())["chains"]:
        if (OUTPUT / entry["file"]).exists():
            existing[entry["id"]] = entry

rebuilt = set()
for argument in sys.argv[1:]:
    entry, size = bundle(argument)
    existing[entry["id"]] = entry
    rebuilt.add(entry["id"])
    changed = sum(1 for stage in entry["stages"] if stage["changed"])
    print(f"rebuilt  {entry['contract']:<20} {len(entry['stages'])} stages, "
          f"{changed} changed the IR, {size // 1024} KB")

chains = sorted(existing.values(), key=lambda entry: entry["contract"])
for entry in chains:
    if entry["id"] not in rebuilt:
        print(f"kept     {entry['contract']:<20} {len(entry['stages'])} stages")

index_path.write_text(json.dumps({"chains": chains}, separators=(",", ":")))
print(f"\nindex.json: {len(chains)} chains")

# A data file nothing points at is left from a chain dropped by hand.
referenced = {entry["file"] for entry in chains} | {"index.json"}
orphans = sorted(path.name for path in OUTPUT.glob("*.json") if path.name not in referenced)
if orphans:
    print("\nnot in the index, safe to delete:")
    for name in orphans:
        print(f"  data/{name}")
