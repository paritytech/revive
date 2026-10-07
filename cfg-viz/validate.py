#!/usr/bin/env python3
"""Structural checks on a lowered cfg.json.

The lowering is only useful if the graph is well formed: every edge lands
somewhere, every block is reachable, and every phi source is a real
predecessor. These are the invariants a correct region-to-block lowering
cannot violate, so a failure here is a lowering bug rather than a data quirk.
"""

import json
import sys
from collections import defaultdict

report = []


def check(graph):
    blocks = {b["id"]: b for b in graph["blocks"]}
    sinks = {s["id"] for s in graph["sinks"]}
    known = set(blocks) | sinks
    problems = []

    for edge in graph["edges"]:
        if edge["from"] not in blocks:
            problems.append(f"edge from unknown block {edge['from']}")
        if edge["to"] not in known:
            problems.append(f"edge to unknown block {edge['to']}")

    predecessors = defaultdict(set)
    successors = defaultdict(set)
    for edge in graph["edges"]:
        predecessors[edge["to"]].add(edge["from"])
        successors[edge["from"]].add(edge["to"])

    seen, stack = set(), [graph["entry"]]
    while stack:
        current = stack.pop()
        if current in seen or current not in blocks:
            continue
        seen.add(current)
        stack.extend(successors[current])
    orphans = [i for i in blocks if i not in seen]

    for block in graph["blocks"]:
        for phi in block["phis"]:
            for source in phi["sources"]:
                if source["from"] not in predecessors[block["id"]]:
                    problems.append(
                        f"{block['id']} phi v{phi['defines']} sourced from "
                        f"{source['from']}, which is not a predecessor")
                if source["value"] is None:
                    problems.append(
                        f"{block['id']} phi v{phi['defines']} has an unresolved "
                        f"source from {source['from']}")

    back = [e for e in graph["edges"] if e["kind"] == "Back"]
    headers = [b for b in graph["blocks"] if b["label"] == "for_header"]
    if len(back) != len(headers):
        problems.append(f"{len(headers)} loop headers but {len(back)} back edges")

    return {
        "id": graph["id"], "blocks": len(blocks), "edges": len(graph["edges"]),
        "phis": sum(len(b["phis"]) for b in graph["blocks"]),
        "loops": len(headers), "orphans": orphans, "problems": problems,
    }


cfg = json.load(open(sys.argv[1]))
print(f"{cfg['source']['stage_id']}  ({cfg['source']['object']})\n")
failed = 0
for graph in cfg["graphs"]:
    result = check(graph)
    status = "ok" if not result["problems"] and not result["orphans"] else "FAIL"
    print(f"[{status}] {result['id']:<45} {result['blocks']:>3}b {result['edges']:>3}e "
          f"{result['phis']:>3}phi {result['loops']}loop")
    for orphan in result["orphans"]:
        print(f"         unreachable: {orphan}")
    for problem in result["problems"][:8]:
        print(f"         {problem}")
    failed += bool(result["problems"] or result["orphans"])
sys.exit(1 if failed else 0)
