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


def check(graph, graph_ids):
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

    # A call records the graph it leaves for. That graph has to exist, or the
    # overlay draws an edge to nowhere — and a called function cannot have been
    # dropped from the object while a caller still names it.
    for call in graph.get("calls", []):
        if call["from"] not in blocks:
            problems.append(f"call from unknown block {call['from']}")
        if call["graph"] not in graph_ids:
            problems.append(f"call to unknown graph {call['graph']}")

    back = [e for e in graph["edges"] if e["kind"] == "Back"]
    headers = [b for b in graph["blocks"] if b["label"] == "for_header"]
    if len(back) != len(headers):
        problems.append(f"{len(headers)} loop headers but {len(back)} back edges")

    return {
        "id": graph["id"], "blocks": len(blocks), "edges": len(graph["edges"]),
        "phis": sum(len(b["phis"]) for b in graph["blocks"]),
        "loops": len(headers), "calls": len(graph.get("calls", [])),
        "orphans": orphans, "problems": problems,
    }


cfg = json.load(open(sys.argv[1]))
print(f"{cfg['source']['stage_id']}  ({cfg['source']['object']})\n")
failed = 0
graph_ids = {graph["id"] for graph in cfg["graphs"]}
for graph in cfg["graphs"]:
    result = check(graph, graph_ids)
    status = "ok" if not result["problems"] and not result["orphans"] else "FAIL"
    print(f"[{status}] {result['id']:<45} {result['blocks']:>3}b {result['edges']:>3}e "
          f"{result['phis']:>3}phi {result['loops']}loop {result['calls']}call")
    for orphan in result["orphans"]:
        print(f"         unreachable: {orphan}")
    for problem in result["problems"][:8]:
        print(f"         {problem}")
    failed += bool(result["problems"] or result["orphans"])
sys.exit(1 if failed else 0)
