#!/usr/bin/env python3
"""Reference lowering from a newyork IR stage artifact to the viewer's cfg.json.

This is a prototype standing in for the Rust that will live behind
`resolc-fv graph`. It exists so the lowering can be exercised against real
stage artifacts and the viewer built before the Rust side compiles; the
cfg.json files it writes double as the snapshots the Rust port must reproduce.

The input is a structured IR: regions nested inside If/Switch/For, with values
passed in through `inputs` and out through a region's `yields`. The output is a
basic-block CFG with explicit phis, built the way `crates/newyork/src/to_llvm.rs`
builds one, so the result can be checked against the `.unoptimized.ll` dump.
"""

import json
import re
import sys
from pathlib import Path

SCHEMA = 1

# Statement variants that end a block by leaving the function or the program.
TERMINATORS = {
    "Return", "Revert", "Stop", "Invalid", "SelfDestruct",
    "PanicRevert", "ErrorStringRevert", "CustomErrorRevert",
}

# Payload fields rendered as a trailing comment rather than as an argument,
# because they annotate the operation instead of feeding it.
ANNOTATION_FIELDS = {"region", "static_slot", "kind"}


def snake(name):
    """`CallDataLoad` -> `calldataload`, matching the pretty printer's spelling."""
    return re.sub(r"(?<!^)(?=[A-Z])", "", name).lower()


def render_type(node):
    if node == "Void":
        return "void"
    if isinstance(node, dict):
        if "Int" in node:
            return node["Int"].lower()
        if "Ptr" in node:
            return "ptr " + node["Ptr"].lower()
    return str(node)


def render_value(node):
    """A use site: `{"id": 3, "value_type": …}` -> `v3`."""
    if isinstance(node, dict) and "id" in node:
        return f"v{node['id']}"
    return render_expression(node)


def render_expression(node):
    """An expression in the pretty printer's Yul-like syntax."""
    if node is None:
        return "_"
    if isinstance(node, str):
        return f"{snake(node)}()"
    if isinstance(node, (int, float)):
        return str(node)
    if not isinstance(node, dict):
        return json.dumps(node)
    if "id" in node and "value_type" in node:
        return f"v{node['id']}"

    (variant, payload), = node.items()
    if variant == "Literal":
        return payload["value"]
    if variant == "Var":
        return f"v{payload}"
    if variant == "Call":
        arguments = ", ".join(render_value(a) for a in payload["arguments"])
        return f"fn{payload['function']}({arguments})"

    if not isinstance(payload, dict):
        return f"{snake(variant)}({render_expression(payload)})"

    arguments, annotations = [], []
    for field, value in payload.items():
        if field in ANNOTATION_FIELDS:
            if value is not None:
                annotations.append(f"{field}={value}")
        elif field in ("value_type", "to"):
            annotations.append(render_type(value))
        elif isinstance(value, list):
            arguments.append("[" + ", ".join(render_value(v) for v in value) + "]")
        else:
            arguments.append(render_value(value))

    text = f"{snake(variant)}({', '.join(arguments)})"
    if annotations:
        text += f" /* {', '.join(annotations)} */"
    return text


def render_statement(statement):
    """One non-control-flow statement as a single line."""
    if isinstance(statement, str):
        return f"{snake(statement)}()"
    (variant, payload), = statement.items()
    if variant == "Let":
        bindings = ", ".join(f"v{b}" for b in payload["bindings"])
        return f"let {bindings} := {render_expression(payload['value'])}"
    if variant == "Expression":
        return render_expression(payload)
    if variant in ("ExternalCall", "Create"):
        rest = {k: v for k, v in payload.items() if k != "result"}
        call = render_expression({variant: rest})
        return f"let v{payload['result']} := {call}"
    return render_expression(statement)


def collect_uses(node, out):
    """Every value id read by a statement, for the dataflow overlay and search."""
    if isinstance(node, dict):
        if "id" in node and "value_type" in node:
            out.append(node["id"])
            return
        for value in node.values():
            collect_uses(value, out)
    elif isinstance(node, list):
        for value in node:
            collect_uses(value, out)


def statement_defs(statement):
    if isinstance(statement, str):
        return []
    (variant, payload), = statement.items()
    if variant == "Let":
        return list(payload["bindings"])
    if variant in ("ExternalCall", "Create") and isinstance(payload, dict):
        return [payload["result"]]
    return []


class GraphBuilder:
    """Lowers one Block (an object's code, or a function body) to blocks and edges."""

    def __init__(self, graph_id, kind, name):
        self.graph_id = graph_id
        self.kind = kind
        self.name = name
        self.blocks = {}
        self.block_order = []
        self.edges = []
        self.regions = []
        self.sinks = {}
        self.calls = []
        self.loops = []          # stack of {"post":…, "exit":…} for Break/Continue
        self.exit_block = None   # function exit, the target of Leave
        self.counter = 0

    # -- construction helpers ------------------------------------------------

    def new_block(self, hint, region):
        identifier = f"bb{self.counter}_{hint}"
        self.counter += 1
        self.blocks[identifier] = {
            "id": identifier, "region": region, "label": hint,
            "phis": [], "statements": [], "terminator": None,
        }
        self.block_order.append(identifier)
        return identifier

    def new_region(self, kind, parent):
        identifier = f"r{len(self.regions)}"
        self.regions.append({"id": identifier, "parent": parent, "kind": kind})
        return identifier

    def edge(self, source, target, kind, label=None):
        """`lower_statements` stops at a terminator, so every caller is a live block."""
        self.edges.append({"from": source, "to": target, "kind": kind, "label": label})

    def sink(self, kind):
        """Terminators of the same kind share one node; splitting them is a view option."""
        identifier = f"sink.{kind.lower()}"
        self.sinks.setdefault(identifier, {"id": identifier, "kind": kind, "merged": 0})
        self.sinks[identifier]["merged"] += 1
        return identifier

    def emit(self, block, statement):
        uses = []
        collect_uses(statement, uses)
        variant = statement if isinstance(statement, str) else next(iter(statement))
        self.blocks[block]["statements"].append({
            "text": render_statement(statement),
            "kind": variant,
            "defs": statement_defs(statement),
            "uses": uses,
        })

    def phi(self, block, defines, sources):
        self.blocks[block]["phis"].append({"defines": defines, "sources": sources})

    # -- the walk ------------------------------------------------------------

    def lower_statements(self, statements, block, region):
        """Returns the block control reaches afterwards, or None if flow ended."""
        for statement in statements:
            if block is None:
                break                       # unreachable tail after a terminator
            block = self.lower_statement(statement, block, region)
        return block

    def lower_region(self, region_node, region_kind, parent_region):
        """A Region: statements plus the values it yields to its parent.

        The entry edge is the caller's to draw, since only the caller knows
        whether it is a `True`, `False`, `Case` or `Default` edge.
        """
        region = self.new_region(region_kind, parent_region)
        entry = self.new_block(region_kind.replace(".", "_"), region)
        tail = self.lower_statements(region_node["statements"], entry, region)
        yields = [v["id"] for v in region_node.get("yields", [])]
        return entry, tail, yields

    def lower_statement(self, statement, block, region):
        if isinstance(statement, str):
            if statement in TERMINATORS:
                self.blocks[block]["terminator"] = {"kind": statement}
                self.edge(block, self.sink(statement), "Terminate")
                return None
            self.emit(block, statement)
            return block

        (variant, payload), = statement.items()

        if variant in TERMINATORS:
            self.blocks[block]["terminator"] = {"kind": variant, "text": render_statement(statement)}
            self.edge(block, self.sink(variant), "Terminate")
            return None

        if variant == "If":
            return self.lower_if(payload, block, region)
        if variant == "Switch":
            return self.lower_switch(payload, block, region)
        if variant == "For":
            return self.lower_for(payload, block, region)
        if variant == "Block":
            entry, tail, yields = self.lower_region(payload, "Block", region)
            self.edge(block, entry, "Fallthrough")
            self.blocks[block]["terminator"] = {"kind": "Br", "target": entry}
            return tail

        if variant == "Break":
            target = self.loops[-1]["exit"]
            self.blocks[block]["terminator"] = {"kind": "Break"}
            self.edge(block, target, "Break")
            self.loops[-1]["break_sources"].append((block, [v["id"] for v in payload["values"]]))
            return None
        if variant == "Continue":
            target = self.loops[-1]["post"]
            self.blocks[block]["terminator"] = {"kind": "Continue"}
            self.edge(block, target, "Continue")
            self.loops[-1]["continue_sources"].append((block, [v["id"] for v in payload["values"]]))
            return None
        if variant == "Leave":
            self.blocks[block]["terminator"] = {"kind": "Leave"}
            self.edge(block, self.exit_block, "Leave")
            self.leave_sources.append((block, [v["id"] for v in payload["return_values"]]))
            return None

        # A call inside a Let is an edge to another graph, drawn as an overlay.
        if variant == "Let" and isinstance(payload.get("value"), dict) and "Call" in payload["value"]:
            self.calls.append({"from": block, "function": payload["value"]["Call"]["function"]})

        self.emit(block, statement)
        return block

    def lower_if(self, payload, block, region):
        self.blocks[block]["terminator"] = {
            "kind": "CondBr", "condition": payload["condition"]["id"],
        }
        then_entry, then_tail, then_yields = self.lower_region(
            payload["then_region"], "If.then", region)
        self.edge(block, then_entry, "True")

        if payload["else_region"] is not None:
            else_entry, else_tail, else_yields = self.lower_region(
                payload["else_region"], "If.else", region)
        else:
            # An absent else yields the inputs unchanged — it is still a block,
            # and without it the join phis lose an incoming edge.
            else_region = self.new_region("If.else.implicit", region)
            else_entry = self.new_block("else_implicit", else_region)
            else_tail, else_yields = else_entry, [v["id"] for v in payload["inputs"]]
        self.edge(block, else_entry, "False")

        outputs = payload["outputs"]
        if then_tail is None and else_tail is None:
            return None                     # both arms diverge: no join

        join = self.new_block("if_join", region)
        for tail, yields in ((then_tail, then_yields), (else_tail, else_yields)):
            if tail is not None:
                self.blocks[tail]["terminator"] = {"kind": "Br", "target": join}
                self.edge(tail, join, "Fallthrough")
        for index, output in enumerate(outputs):
            sources = [
                {"from": tail, "value": yields[index] if index < len(yields) else None}
                for tail, yields in ((then_tail, then_yields), (else_tail, else_yields))
                if tail is not None
            ]
            self.phi(join, output, sources)
        return join

    def lower_switch(self, payload, block, region):
        self.blocks[block]["terminator"] = {
            "kind": "Switch", "scrutinee": payload["scrutinee"]["id"],
        }
        arms = []
        for case in payload["cases"]:
            entry, tail, yields = self.lower_region(case["body"], "Switch.case", region)
            self.edge(block, entry, "Case", case["value"])
            arms.append((tail, yields))

        if payload["default"] is not None:
            entry, tail, yields = self.lower_region(payload["default"], "Switch.default", region)
        else:
            default_region = self.new_region("Switch.default.implicit", region)
            entry = self.new_block("default_implicit", default_region)
            tail, yields = entry, [v["id"] for v in payload["inputs"]]
        self.edge(block, entry, "Default")
        arms.append((tail, yields))

        live = [(tail, yields) for tail, yields in arms if tail is not None]
        if not live:
            return None
        join = self.new_block("switch_join", region)
        for tail, _ in live:
            self.blocks[tail]["terminator"] = {"kind": "Br", "target": join}
            self.edge(tail, join, "Fallthrough")
        for index, output in enumerate(payload["outputs"]):
            self.phi(join, output, [
                {"from": tail, "value": yields[index] if index < len(yields) else None}
                for tail, yields in live
            ])
        return join

    def lower_for(self, payload, block, region):
        loop_region = self.new_region("For", region)
        header = self.new_block("for_header", loop_region)
        post = self.new_block("for_post", loop_region)
        exit_block = self.new_block("for_exit", loop_region)

        self.blocks[block]["terminator"] = {"kind": "Br", "target": header}
        self.edge(block, header, "Fallthrough")

        # Header phis: the loop-carried names, entered from the preheader and
        # re-entered from the latch.
        initial = [v["id"] for v in payload["initial_values"]]
        for index, variable in enumerate(payload["loop_variables"]):
            self.phi(header, variable, [
                {"from": block, "value": initial[index] if index < len(initial) else None},
                {"from": post, "value": None},   # patched once the latch yields are known
            ])

        header_tail = self.lower_statements(payload["condition_statements"], header, loop_region)
        self.blocks[header_tail]["terminator"] = {
            "kind": "CondBr", "text": render_expression(payload["condition"]),
        }

        frame = {"post": post, "exit": exit_block, "break_sources": [], "continue_sources": []}
        self.loops.append(frame)
        body_entry, body_tail, body_yields = self.lower_region(
            payload["body"], "For.body", loop_region)
        self.loops.pop()
        self.edge(header_tail, body_entry, "True")
        self.edge(header_tail, exit_block, "False")

        # Latch phis: the body's normal exit merged with every `continue` site.
        sources = []
        if body_tail is not None:
            self.blocks[body_tail]["terminator"] = {"kind": "Br", "target": post}
            self.edge(body_tail, post, "Fallthrough")
            sources.append((body_tail, body_yields))
        sources.extend(frame["continue_sources"])
        for index, variable in enumerate(payload["post_input_variables"]):
            self.phi(post, variable, [
                {"from": source, "value": values[index] if index < len(values) else None}
                for source, values in sources
            ])

        post_tail = self.lower_statements(payload["post"]["statements"], post, loop_region)
        post_yields = [v["id"] for v in payload["post"]["yields"]]
        if post_tail is not None:
            self.blocks[post_tail]["terminator"] = {"kind": "Br", "target": header}
            self.edge(post_tail, header, "Back")
            for index, phi in enumerate(self.blocks[header]["phis"]):
                if index < len(post_yields):
                    phi["sources"][1] = {"from": post_tail, "value": post_yields[index]}

        # Exit phis: the loop-carried names on condition failure, plus `break` sites.
        exit_sources = [(header_tail, list(payload["loop_variables"]))] + frame["break_sources"]
        for index, output in enumerate(payload["outputs"]):
            self.phi(exit_block, output, [
                {"from": source, "value": values[index] if index < len(values) else None}
                for source, values in exit_sources
            ])
        return exit_block

    # -- entry point ---------------------------------------------------------

    def build(self, body, return_values=None):
        root = self.new_region("Root", None)
        entry = self.new_block("entry", root)
        # Only a function needs an exit block: `Leave` is function-only in Yul,
        # and an object's code always ends in return/revert/stop, so an exit
        # block there would be unreachable by construction.
        self.exit_block = self.new_block("exit", root) if self.kind == "Function" else None
        self.leave_sources = []
        tail = self.lower_statements(body["statements"], entry, root)
        exit_sources = list(self.leave_sources)
        if tail is not None and self.exit_block is not None:
            self.edge(tail, self.exit_block, "Fallthrough")
            self.blocks[tail]["terminator"] = self.blocks[tail]["terminator"] or {
                "kind": "Br", "target": self.exit_block}
            exit_sources.append((tail, return_values or []))
        if self.exit_block is None:
            exit_sources = []
        for index, output in enumerate(return_values or []):
            self.phi(self.exit_block, output, [
                {"from": source, "value": values[index] if index < len(values) else None}
                for source, values in exit_sources
            ])
        return {
            "id": self.graph_id, "kind": self.kind, "name": self.name,
            "entry": entry, "exit": self.exit_block,
            "regions": self.regions,
            "blocks": [self.blocks[i] for i in self.block_order],
            "edges": self.edges,
            "sinks": list(self.sinks.values()),
            "calls": self.calls,
        }


def lower_object(obj, path, graphs):
    prefix = "/".join(path + [obj["name"]])
    graphs.append(GraphBuilder(f"{prefix}/code", "ObjectCode", f"{obj['name']} code")
                  .build(obj["code"]))
    for identifier, function in sorted(obj["functions"].items(), key=lambda kv: int(kv[0])):
        builder = GraphBuilder(f"{prefix}/fn:{identifier}", "Function", function["name"])
        graph = builder.build(function["body"], function.get("return_values", []))
        graph["signature"] = {
            "parameters": [f"v{p[0]}: {render_type(p[1])}" for p in function["parameters"]],
            "returns": [render_type(t) for t in function["returns"]],
            "call_count": function.get("call_count"),
            "size_estimate": function.get("size_estimate"),
        }
        graphs.append(graph)
    for subobject in obj["subobjects"]:
        lower_object(subobject, path + [obj["name"]], graphs)


def lower_stage(path):
    record = json.loads(Path(path).read_text())
    graphs = []
    lower_object(record["ir"], [], graphs)
    return {
        "schema": SCHEMA,
        "source": {
            "file": Path(path).name,
            "stage_id": record["stage_id"],
            "pass": record["pass"],
            "object": record["object"],
            "side_tables": [k for k in ("type_info", "heap_opt", "inline_results", "mem_opt")
                            if k in record],
        },
        "graphs": graphs,
    }


if __name__ == "__main__":
    output = lower_stage(sys.argv[1])
    destination = sys.argv[2] if len(sys.argv) > 2 else "-"
    text = json.dumps(output, indent=1)
    if destination == "-":
        print(text)
    else:
        Path(destination).write_text(text)
        totals = [(g["id"], len(g["blocks"]), len(g["edges"])) for g in output["graphs"]]
        for graph_id, blocks, edges in totals:
            print(f"{graph_id:<45} {blocks:>4} blocks {edges:>4} edges")
