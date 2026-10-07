"use strict";

// Exposed on `window.cfgviz` so the graph can be driven from the console and
// smoke-tested outside a browser; see smoke.js.

// ---------------------------------------------------------------------------
// The CFG is derived, not read: the newyork IR stores structured regions, and
// `lower.py` turns them into blocks and typed edges the way `to_llvm.rs` does.
// This page renders that lowering. Edge kind carries the meaning, so it owns
// the saturated colours; everything else stays neutral.
// ---------------------------------------------------------------------------

const EDGE_STYLE = {
  True:        { color: "var(--edge-true)",      label: "true"  },
  False:       { color: "var(--edge-false)",     label: "false" },
  Case:        { color: "var(--region-case)" },
  Default:     { color: "var(--region-case)",    label: "default", dash: "4 3" },
  Fallthrough: { color: "var(--edge-plain)" },
  Back:        { color: "var(--edge-back)",      label: "loop" },
  Break:       { color: "var(--edge-nonlocal)",  label: "break",    dash: "5 3" },
  Continue:    { color: "var(--edge-nonlocal)",  label: "continue", dash: "5 3" },
  Leave:       { color: "var(--edge-nonlocal)",  label: "leave",    dash: "5 3" },
  Terminate:   { color: "var(--edge-terminate)", dash: "2 3" },
  // Not a CFG edge: a call leaves this graph entirely, and the overlay that
  // draws it is built from `graph.calls` rather than from `graph.edges`.
  Call:        { color: "var(--edge-call)",      label: "call",     dash: "1 3.5" },
};

const LEGEND = [
  ["True", "true"], ["False", "false"], ["Back", "back edge"],
  ["Break", "break / continue / leave"], ["Terminate", "terminate"],
  ["Call", "call"],
];

const REGION_COLOR = {
  "Root": "var(--region-root)",
  "Block": "var(--region-block)",
  "If.then": "var(--region-then)",
  "If.else": "var(--region-else)",
  "If.else.implicit": "var(--region-else)",
  "Switch.case": "var(--region-case)",
  "Switch.default": "var(--region-case)",
  "Switch.default.implicit": "var(--region-case)",
  "For": "var(--region-loop)",
  "For.body": "var(--region-loop)",
};

// Blocks the lowering invents, which have no statement of their own in the IR.
const SYNTHETIC = new Set([
  "if_join", "switch_join", "else_implicit", "default_implicit",
  "for_header", "for_post", "for_exit", "exit",
]);

const CHAR_W = 6.95;      // IBM Plex Mono advance at 11.5px
const LINE_H = 15;
const HEAD_H = 20;
const PAD_X = 9;
const PAD_Y = 7;
const MAX_W = 560;
const MAX_LINES = 14;
const CALL_H = 36;

const state = {
  index: null, chain: null, chainData: null,
  stageId: null, graphId: null,
  detail: true, selected: null, query: "", highlight: null,
  view: { x: 0, y: 0, k: 1 },
};

const svg = document.getElementById("canvas");
const SVGNS = "http://www.w3.org/2000/svg";
let root = null;

const el = (tag, attrs = {}, text) => {
  const node = document.createElementNS(SVGNS, tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  if (text !== undefined) node.textContent = text;
  return node;
};

// ---------------------------------------------------------------------------
// Block contents
// ---------------------------------------------------------------------------

function phiText(phi) {
  const sources = phi.sources
    .map((s) => `${shortId(s.from)}:v${s.value}`)
    .join(", ");
  return `v${phi.defines} = φ(${sources})`;
}

function shortId(blockId) {
  return String(blockId).replace(/^bb(\d+)_.*/, "b$1").replace(/^sink\./, "");
}

function terminatorText(terminator) {
  if (!terminator) return null;
  switch (terminator.kind) {
    case "CondBr": return terminator.text
      ? `cond ${terminator.text}`
      : `cond v${terminator.condition}`;
    case "Switch": return `switch v${terminator.scrutinee}`;
    case "Br": return null;                       // the edge already says it
    case "Break": case "Continue": case "Leave": return terminator.kind.toLowerCase();
    default: return terminator.text || terminator.kind.toLowerCase();
  }
}

function blockLines(block, detail) {
  const lines = block.phis.map((p) => ({ text: phiText(p), cls: "phi" }));
  if (detail) {
    for (const statement of block.statements) {
      lines.push({ text: statement.text, cls: statement.kind === "Call" ? "call" : "" });
    }
  } else if (block.statements.length) {
    lines.push({ text: `${block.statements.length} statements`, cls: "more" });
  }
  const terminator = terminatorText(block.terminator);
  if (terminator) lines.push({ text: terminator, cls: "term" });

  if (lines.length > MAX_LINES) {
    const head = lines.slice(0, MAX_LINES - 1);
    head.push({ text: `… ${lines.length - MAX_LINES + 1} more lines`, cls: "more" });
    return head;
  }
  return lines;
}

function measure(block, detail) {
  const lines = blockLines(block, detail);
  const widest = lines.reduce((w, l) => Math.max(w, l.text.length), 0);
  const headWidth = (block.label.length + String(block.id).length + 4) * 6.3;
  const width = Math.min(MAX_W, Math.max(150, Math.max(widest * CHAR_W, headWidth) + PAD_X * 2));
  const height = HEAD_H + lines.length * LINE_H + PAD_Y * 2;
  return { lines, width, height };
}

function truncate(text, width) {
  const limit = Math.floor((width - PAD_X * 2) / CHAR_W);
  return text.length > limit ? text.slice(0, Math.max(1, limit - 1)) + "…" : text;
}

// ---------------------------------------------------------------------------
// Calls
// ---------------------------------------------------------------------------

// A function is named two ways: by the index the IR calls it with (`fn0`,
// which is also the tail of its graph id) and by the Yul name the source
// wrote. The viewer shows both everywhere a callee appears, because a reader
// holding only one of them cannot match a call site to a function graph.

function functionTag(graph) {
  if (!graph || graph.kind !== "Function") return null;
  const index = graph.function ?? Number(String(graph.id).split("fn:").pop());
  return Number.isFinite(index) ? `fn${index}` : null;
}

function calleeGraphId(graph, call) {
  // The lowering writes the target id; derive it for data bundled before that.
  if (call.graph) return call.graph;
  return graph.id.split("/").slice(0, -1).concat(`fn:${call.function}`).join("/");
}

// One call-out per distinct callee, carrying the blocks that call it.
function callOverlay(graph) {
  const stage = state.chainData?.[state.stageId];
  const byTarget = new Map();
  for (const call of graph.calls || []) {
    const target = calleeGraphId(graph, call);
    if (!byTarget.has(target)) {
      const resolved = stage?.graphs.find((g) => g.id === target);
      byTarget.set(target, {
        id: `call.fn${call.function}`,
        target,
        name: call.name || resolved?.name || null,
        tag: `fn${call.function}`,
        present: Boolean(resolved),
        sites: new Map(),
      });
    }
    const callout = byTarget.get(target);
    callout.sites.set(call.from, (callout.sites.get(call.from) || 0) + 1);
  }
  const callouts = [...byTarget.values()];
  for (const callout of callouts) {
    const total = [...callout.sites.values()].reduce((n, c) => n + c, 0);
    callout.label = callout.name || callout.tag;
    callout.meta = `${callout.name ? callout.tag + " · " : ""}`
      + `${total} call site${total === 1 ? "" : "s"}`
      + (callout.present ? "" : " · not in this stage");
    callout.width = Math.max(
      140, Math.max(callout.label.length * 7.1, callout.meta.length * 6.1) + 22);
  }
  return callouts;
}

// ---------------------------------------------------------------------------
// Layout and render
// ---------------------------------------------------------------------------

function currentGraph() {
  const stage = state.chainData?.[state.stageId];
  if (!stage) return null;
  return stage.graphs.find((g) => g.id === state.graphId) || stage.graphs[0];
}

function matches(block, query) {
  if (!query) return false;
  const value = /^v(\d+)$/.exec(query.trim());
  if (value) {
    const id = Number(value[1]);
    return block.statements.some((s) => s.defs.includes(id) || s.uses.includes(id))
      || block.phis.some((p) => p.defines === id || p.sources.some((s) => s.value === id));
  }
  const needle = query.trim().toLowerCase();
  return block.statements.some((s) => s.text.toLowerCase().includes(needle))
    || block.label.toLowerCase().includes(needle);
}

function render() {
  const graph = currentGraph();
  svg.textContent = "";
  if (!graph) return;

  const regionKind = new Map(graph.regions.map((r) => [r.id, r.kind]));
  const layout = new dagre.graphlib.Graph({ multigraph: true });
  layout.setGraph({ rankdir: "TB", ranksep: 46, nodesep: 26, edgesep: 10, marginx: 24, marginy: 24 });
  layout.setDefaultEdgeLabel(() => ({}));

  const boxes = new Map();
  for (const block of graph.blocks) {
    const box = measure(block, state.detail);
    boxes.set(block.id, box);
    layout.setNode(block.id, { width: box.width, height: box.height });
  }
  for (const sink of graph.sinks) {
    const width = Math.max(96, sink.kind.length * 8 + 44);
    layout.setNode(sink.id, { width, height: 28 });
  }

  // The call overlay rides along in the same layout so callees land below the
  // blocks that reach them, but it stays out of `graph.edges`: a call is not a
  // control-flow edge, and the stats and the validator both count on that.
  const callouts = callOverlay(graph);
  const overlayEdges = [];
  for (const callout of callouts) {
    layout.setNode(callout.id, { width: callout.width, height: CALL_H });
    for (const [from, count] of callout.sites) {
      overlayEdges.push({
        from, to: callout.id, kind: "Call",
        label: count > 1 ? `call ×${count}` : "call",
      });
    }
  }
  const drawnEdges = graph.edges.concat(overlayEdges);

  drawnEdges.forEach((edge, index) => {
    if (layout.hasNode(edge.from) && layout.hasNode(edge.to)) {
      layout.setEdge(edge.from, edge.to, { kind: edge.kind }, `e${index}`);
    }
  });
  dagre.layout(layout);

  root = el("g");
  const defs = el("defs");
  for (const [kind, style] of Object.entries(EDGE_STYLE)) {
    const marker = el("marker", {
      id: `arrow-${kind}`, viewBox: "0 0 8 8", refX: 7, refY: 4,
      markerWidth: 5, markerHeight: 5, orient: "auto-start-reverse",
    });
    marker.appendChild(el("path", { d: "M0,0 L8,4 L0,8 z", style: `fill:${style.color}` }));
    defs.appendChild(marker);
  }
  svg.appendChild(defs);
  svg.appendChild(root);

  const edgeLayer = el("g");
  const nodeLayer = el("g");
  root.appendChild(edgeLayer);
  root.appendChild(nodeLayer);

  const query = state.query;
  const hits = new Set(query ? graph.blocks.filter((b) => matches(b, query)).map((b) => b.id) : []);
  const dimming = query && hits.size > 0;

  // -- edges
  drawnEdges.forEach((edge, index) => {
    const laid = layout.edge({ v: edge.from, w: edge.to, name: `e${index}` });
    if (!laid || !laid.points) return;
    const style = EDGE_STYLE[edge.kind] || EDGE_STYLE.Fallthrough;
    const points = laid.points;
    const d = points.map((p, i) => (i === 0 ? `M${p.x},${p.y}` : `L${p.x},${p.y}`)).join(" ");
    const path = el("path", {
      d, class: "edge" + (dimming ? " dim" : ""), style: `stroke:${style.color}`,
      "marker-end": `url(#arrow-${edge.kind})`,
    });
    if (style.dash) path.setAttribute("stroke-dasharray", style.dash);
    edgeLayer.appendChild(path);

    const text = edge.label || style.label;
    if (text && !dimming) {
      const middle = points[Math.floor(points.length / 2)];
      edgeLayer.appendChild(el("text", {
        x: middle.x + 5, y: middle.y - 3, class: "edge-label", style: `fill:${style.color}`,
      }, text));
    }
  });

  // -- sinks
  for (const sink of graph.sinks) {
    const node = layout.node(sink.id);
    if (!node) continue;
    const group = el("g", { transform: `translate(${node.x - node.width / 2},${node.y - node.height / 2})` });
    group.appendChild(el("rect", {
      class: "sink-box", width: node.width, height: node.height, rx: 14,
    }));
    group.appendChild(el("text", {
      x: node.width / 2, y: node.height / 2 + 4, class: "sink-label", "text-anchor": "middle",
    }, `${sink.kind.toLowerCase()} ×${sink.merged}`));
    nodeLayer.appendChild(group);
  }

  // -- call-outs
  for (const callout of callouts) {
    const node = layout.node(callout.id);
    if (!node) continue;
    const group = el("g", {
      transform: `translate(${node.x - node.width / 2},${node.y - node.height / 2})`,
      style: callout.present ? "cursor:pointer" : "cursor:default",
    });
    group.appendChild(el("rect", {
      class: "call-box" + (callout.present ? "" : " missing") + (dimming ? " dim" : ""),
      width: node.width, height: CALL_H, rx: 6,
    }));
    group.appendChild(el("text", { x: 11, y: 16, class: "call-label" }, callout.label));
    group.appendChild(el("text", { x: 11, y: 28, class: "call-meta" }, callout.meta));
    group.appendChild(el("title", {}, callout.present
      ? `${callout.label} (${callout.tag}) — open ${callout.target}`
      : `${callout.label} (${callout.tag}) — no graph for it in this stage`));
    if (callout.present) group.addEventListener("click", () => showGraph(callout.target));
    nodeLayer.appendChild(group);
  }

  // -- blocks
  for (const block of graph.blocks) {
    const node = layout.node(block.id);
    const box = boxes.get(block.id);
    const group = el("g", {
      transform: `translate(${node.x - box.width / 2},${node.y - box.height / 2})`,
      style: "cursor:pointer",
    });
    group.addEventListener("click", () => select(block.id));

    const classes = ["node-box"];
    if (SYNTHETIC.has(block.label)) classes.push("synthetic");
    if (state.selected === block.id) classes.push("selected");
    if (dimming && !hits.has(block.id)) classes.push("dim");
    group.appendChild(el("rect", {
      class: classes.join(" "), width: box.width, height: box.height, rx: 5,
    }));
    group.appendChild(el("path", {
      class: "node-head", d:
        `M0,5 a5,5 0 0 1 5,-5 h${box.width - 10} a5,5 0 0 1 5,5 v${HEAD_H - 5} h-${box.width} z`,
    }));
    const stripe = REGION_COLOR[regionKind.get(block.region)] || "var(--region-root)";
    group.appendChild(el("line", {
      class: "stripe", x1: 2.5, y1: 6, x2: 2.5, y2: box.height - 6,
      style: `stroke:${stripe}`,
    }));
    group.appendChild(el("text", { x: PAD_X, y: 14, class: "node-label" }, block.label));
    group.appendChild(el("text", {
      x: box.width - PAD_X, y: 14, class: "node-meta", "text-anchor": "end",
    }, shortId(block.id)));

    box.lines.forEach((line, index) => {
      const cls = ["node-line", line.cls].filter(Boolean);
      if (hits.has(block.id) && query && line.text.toLowerCase().includes(query.trim().toLowerCase())) {
        cls.push("hit");
      }
      group.appendChild(el("text", {
        x: PAD_X, y: HEAD_H + PAD_Y + index * LINE_H + 11, class: cls.join(" "),
      }, truncate(line.text, box.width)));
    });

    nodeLayer.appendChild(group);
  }

  const size = layout.graph();
  svg.dataset.width = size.width;
  svg.dataset.height = size.height;

  document.getElementById("stats").textContent =
    `${graph.blocks.length} blocks · ${graph.edges.length} edges · ` +
    `${graph.blocks.reduce((n, b) => n + b.phis.length, 0)} phis · ` +
    `${graph.edges.filter((e) => e.kind === "Back").length} loops` +
    (graph.calls?.length ? ` · ${graph.calls.length} calls` : "") +
    (hits.size ? ` · ${hits.size} matching` : "");

  applyView();
}

function applyView() {
  if (root) root.setAttribute("transform",
    `translate(${state.view.x},${state.view.y}) scale(${state.view.k})`);
}

function fit() {
  const width = Number(svg.dataset.width) || 1;
  const height = Number(svg.dataset.height) || 1;
  const rect = svg.getBoundingClientRect();
  const k = Math.min(rect.width / width, rect.height / height, 1.4) * 0.94;
  state.view = { k, x: (rect.width - width * k) / 2, y: (rect.height - height * k) / 2 };
  applyView();
}

// ---------------------------------------------------------------------------
// Inspector
// ---------------------------------------------------------------------------

function regionPath(graph, regionId) {
  const byId = new Map(graph.regions.map((r) => [r.id, r]));
  const path = [];
  let current = byId.get(regionId);
  while (current) {
    path.unshift(current.kind);
    current = current.parent ? byId.get(current.parent) : null;
  }
  return path;
}

function select(blockId) {
  state.selected = blockId;
  render();
  renderInspector();
}

function showGraph(graphId) {
  state.graphId = graphId;
  state.selected = null;
  renderGraphs();
  render();
  fit();
  renderInspector();
}

function renderInspector() {
  const panel = document.getElementById("inspector");
  const graph = currentGraph();
  panel.textContent = "";
  if (!graph) return;

  const block = graph.blocks.find((b) => b.id === state.selected);
  if (!block) {
    const head = document.createElement("div");
    head.className = "insp-head";
    const tag = functionTag(graph);
    head.innerHTML = `<p class="insp-title">${escapeHtml(graph.name)}</p>`
      + `<p class="insp-sub">${tag ? `Function · called as <b>${tag}</b>` : "Object code"} ·`
      + ` ${graph.blocks.length} blocks</p>`;
    panel.appendChild(head);

    if (graph.signature) {
      // Two different numbers, and they disagree on purpose. `observed` is
      // counted from the call sites in this stage's graphs; `call_count` is IR
      // metadata that only the inline pass recomputes, so it reads 0 on every
      // stage before the first inline — which used to make a live function
      // look dead. Lead with the observed count and say when the field is
      // behind it rather than quietly showing either one alone.
      const observed = graph.signature.observed_call_sites;
      const declared = graph.signature.call_count;
      const behind = observed !== undefined && declared !== undefined
        && declared !== observed;
      const group = document.createElement("div");
      group.className = "insp-group";
      group.innerHTML = `<h3>Signature</h3><dl class="kv">`
        + `<dt>parameters</dt><dd>${escapeHtml(graph.signature.parameters.join(", ")) || "none"}</dd>`
        + `<dt>returns</dt><dd>${escapeHtml(graph.signature.returns.join(", ")) || "none"}</dd>`
        + `<dt>call sites</dt><dd>${observed ?? "—"}</dd>`
        + `<dt>call_count</dt><dd>${declared ?? "—"}`
        + `${behind ? ` <span class="note">not recomputed yet</span>` : ""}</dd>`
        + `<dt>size estimate</dt><dd>${graph.signature.size_estimate ?? "—"}</dd></dl>`
        + (behind
          ? `<p class="footnote">The IR carries <code>call_count</code> as metadata and `
            + `only the inline pass refreshes it, so it lags the call sites actually `
            + `present in this stage.</p>`
          : "");
      panel.appendChild(group);
    }
    const hint = document.createElement("div");
    hint.className = "insp-group";
    hint.innerHTML = `<h3>Reading the graph</h3><p class="empty">`
      + `Select a block to see its phis, statements and the regions it sits inside. `
      + `A dashed outline marks a block the lowering invented — joins, implicit `
      + `else arms, loop headers and latches have no statement of their own in the IR.</p>`;
    panel.appendChild(hint);
    return;
  }

  const head = document.createElement("div");
  head.className = "insp-head";
  head.innerHTML = `<p class="insp-title">${block.label}</p>`
    + `<p class="insp-sub">${block.id}${SYNTHETIC.has(block.label) ? " · synthesized by the lowering" : ""}</p>`;
  panel.appendChild(head);

  const crumbs = document.createElement("div");
  crumbs.className = "insp-group";
  crumbs.innerHTML = `<h3>Region nesting</h3><div class="breadcrumb">`
    + regionPath(graph, block.region).map((k) => `<span class="crumb">${k}</span>`).join("")
    + `</div>`;
  panel.appendChild(crumbs);

  if (block.phis.length) {
    const group = document.createElement("div");
    group.className = "insp-group";
    group.innerHTML = `<h3>Phis</h3><pre class="code">`
      + block.phis.map((p) => `<span class="ln phi">${phiText(p)}</span>`).join("")
      + `</pre>`;
    panel.appendChild(group);
  }

  const body = document.createElement("div");
  body.className = "insp-group";
  const terminator = terminatorText(block.terminator);
  body.innerHTML = `<h3>Statements <span class="count">${block.statements.length}</span></h3>`
    + (block.statements.length || terminator
      ? `<pre class="code">`
        + block.statements.map((s) =>
          `<span class="ln${s.kind === "Call" ? " call" : ""}">${escapeHtml(s.text)}</span>`).join("")
        + (terminator ? `<span class="ln term">${escapeHtml(terminator)}</span>` : "")
        + `</pre>`
      : `<p class="empty">No statements — control only.</p>`);
  panel.appendChild(body);

  const callees = block.statements.filter((s) => s.callee).map((s) => s.callee);
  if (callees.length) {
    const group = document.createElement("div");
    group.className = "insp-group";
    group.innerHTML = `<h3>Calls <span class="count">${callees.length}</span></h3>`;
    const chips = document.createElement("div");
    chips.className = "chips";
    // One chip per callee, not per call site, so two calls to the same
    // function read as one destination called twice.
    const distinct = new Map();
    for (const callee of callees) {
      if (!distinct.has(callee.graph)) distinct.set(callee.graph, { callee, count: 0 });
      distinct.get(callee.graph).count += 1;
    }
    for (const { callee, count } of distinct.values()) {
      const target = state.chainData?.[state.stageId]?.graphs
        .find((g) => g.id === callee.graph);
      const chip = document.createElement("button");
      chip.className = "chip call";
      chip.innerHTML = `${escapeHtml(callee.name || `fn${callee.function}`)}`
        + `<span class="idx">fn${callee.function}${count > 1 ? ` ×${count}` : ""}</span>`;
      chip.title = target
        ? `Open ${callee.graph}`
        : `${callee.graph} is not a graph in this stage`;
      chip.disabled = !target;
      if (target) chip.addEventListener("click", () => showGraph(callee.graph));
      chips.appendChild(chip);
    }
    group.appendChild(chips);
    panel.appendChild(group);
  }

  const defs = new Set();
  const uses = new Set();
  for (const statement of block.statements) {
    statement.defs.forEach((d) => defs.add(d));
    statement.uses.forEach((u) => uses.add(u));
  }
  block.phis.forEach((p) => defs.add(p.defines));

  for (const [title, values] of [["Defines", defs], ["Uses", uses]]) {
    const group = document.createElement("div");
    group.className = "insp-group";
    const sorted = [...values].sort((a, b) => a - b);
    group.innerHTML = `<h3>${title} <span class="count">${sorted.length}</span></h3>`;
    if (!sorted.length) {
      group.insertAdjacentHTML("beforeend", `<p class="empty">none</p>`);
    } else {
      const chips = document.createElement("div");
      chips.className = "chips";
      for (const value of sorted) {
        const chip = document.createElement("button");
        chip.className = "chip" + (state.query === `v${value}` ? " on" : "");
        chip.textContent = `v${value}`;
        chip.title = `Highlight every block touching v${value}`;
        chip.addEventListener("click", () => {
          const term = `v${value}`;
          state.query = state.query === term ? "" : term;
          document.getElementById("search").value = state.query;
          render();
          renderInspector();
        });
        chips.appendChild(chip);
      }
      group.appendChild(chips);
    }
    panel.appendChild(group);
  }
}

function escapeHtml(text) {
  return String(text).replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
}

// ---------------------------------------------------------------------------
// Rails
// ---------------------------------------------------------------------------

function renderPasses() {
  const list = document.getElementById("pass-list");
  list.textContent = "";
  const stages = state.chain.stages;
  const changed = stages.filter((s) => s.changed).length;
  document.getElementById("pass-count").textContent = `${changed} of ${stages.length} rewrote the IR`;

  let object = null;
  for (const stage of stages) {
    if (stage.object !== object) {
      object = stage.object;
      const header = document.createElement("div");
      header.className = "object-break";
      header.textContent = object;
      list.appendChild(header);
    }
    const row = document.createElement("button");
    row.className = "row" + (stage.changed ? " changed" : "");
    row.setAttribute("aria-selected", String(stage.id === state.stageId));
    row.innerHTML = `<span class="ord">${stage.id.split(".")[0]}</span>`
      + `<span class="delta"></span>`
      + `<span class="name">${stage.pass}</span>`;
    row.title = stage.changed
      ? `${stage.id} — this pass changed the IR`
      : `${stage.id} — left the IR unchanged`;
    row.addEventListener("click", () => {
      state.stageId = stage.id;
      state.selected = null;
      renderPasses();
      renderGraphs();
      render();
      fit();
      renderInspector();
    });
    list.appendChild(row);
  }
  const selected = list.querySelector('[aria-selected="true"]');
  if (selected) selected.scrollIntoView({ block: "nearest" });
}

function renderGraphs() {
  const list = document.getElementById("graph-list");
  list.textContent = "";
  const stage = state.chainData[state.stageId];
  const graphs = stage.graphs;
  if (!graphs.some((g) => g.id === state.graphId)) {
    // Keep the view on the biggest graph when the previous one is gone,
    // which happens whenever `inline` removes a function.
    state.graphId = graphs.reduce((a, b) => (b.blocks.length > a.blocks.length ? b : a)).id;
  }
  document.getElementById("graph-count").textContent = `${graphs.length}`;

  for (const graph of graphs) {
    const row = document.createElement("button");
    row.className = "row changed";
    row.setAttribute("aria-selected", String(graph.id === state.graphId));
    const tag = functionTag(graph);
    const name = graph.kind === "Function" ? graph.name : graph.id.split("/").slice(-2, -1)[0] + " code";
    row.innerHTML = `<span class="name">${escapeHtml(name)}</span>`
      + (tag ? `<span class="tag">${tag}</span>` : "")
      + `<span class="tag">${graph.blocks.length}b</span>`;
    row.title = graph.id;
    row.addEventListener("click", () => showGraph(graph.id));
    list.appendChild(row);
  }
}

function renderLegend() {
  const legend = document.getElementById("legend");
  legend.textContent = "";
  for (const [kind, label] of LEGEND) {
    const style = EDGE_STYLE[kind];
    const item = document.createElement("span");
    item.className = "legend-item";
    item.innerHTML = `<span class="legend-swatch" style="background:${style.color}"></span>${label}`;
    legend.appendChild(item);
  }
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

async function loadChain(chainId) {
  const chain = state.index.chains.find((c) => c.id === chainId);
  state.chain = chain;
  document.getElementById("contract").textContent = chain.contract;
  const response = await fetch(`data/${chain.file}`);
  if (!response.ok) throw new Error(`data/${chain.file} responded ${response.status}`);
  state.chainData = await response.json();
  // Open on the last stage: the IR codegen actually received.
  state.stageId = chain.stages[chain.stages.length - 1].id;
  state.graphId = null;
  state.selected = null;
  renderPasses();
  renderGraphs();
  render();
  fit();
  renderInspector();
}

async function boot() {
  if (typeof dagre === "undefined") {
    throw new Error("vendor/dagre.min.js did not load \u2014 is it next to index.html?");
  }
  renderLegend();
  const response = await fetch("data/index.json");
  if (!response.ok) throw new Error(`data/index.json responded ${response.status}`);
  state.index = await response.json();
  const picker = document.getElementById("chain");
  for (const chain of state.index.chains) {
    const option = document.createElement("option");
    option.value = chain.id;
    option.textContent = chain.contract;
    picker.appendChild(option);
  }
  picker.addEventListener("change", () => loadChain(picker.value));
  await loadChain(state.index.chains[0].id);
  document.getElementById("loading").remove();
}

// pan and zoom
let dragging = null;
svg.addEventListener("pointerdown", (event) => {
  dragging = { x: event.clientX, y: event.clientY, vx: state.view.x, vy: state.view.y };
  svg.classList.add("panning");
  svg.setPointerCapture(event.pointerId);
});
svg.addEventListener("pointermove", (event) => {
  if (!dragging) return;
  state.view.x = dragging.vx + (event.clientX - dragging.x);
  state.view.y = dragging.vy + (event.clientY - dragging.y);
  applyView();
});
svg.addEventListener("pointerup", (event) => {
  dragging = null;
  svg.classList.remove("panning");
  svg.releasePointerCapture(event.pointerId);
});
svg.addEventListener("wheel", (event) => {
  event.preventDefault();
  const rect = svg.getBoundingClientRect();
  const mx = event.clientX - rect.left;
  const my = event.clientY - rect.top;
  const factor = Math.exp(-event.deltaY * 0.0014);
  const k = Math.min(3, Math.max(0.06, state.view.k * factor));
  state.view.x = mx - (mx - state.view.x) * (k / state.view.k);
  state.view.y = my - (my - state.view.y) * (k / state.view.k);
  state.view.k = k;
  applyView();
}, { passive: false });

document.getElementById("fit").addEventListener("click", fit);
document.getElementById("detail").addEventListener("click", (event) => {
  state.detail = !state.detail;
  event.target.classList.toggle("on", state.detail);
  event.target.textContent = state.detail ? "Full" : "Compact";
  render();
  fit();
});

let searchTimer = null;
document.getElementById("search").addEventListener("input", (event) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    state.query = event.target.value;
    render();
  }, 160);
});
document.getElementById("clear").addEventListener("click", () => {
  state.query = "";
  document.getElementById("search").value = "";
  render();
  renderInspector();
});

document.addEventListener("keydown", (event) => {
  if (event.target.tagName === "INPUT" || event.target.tagName === "SELECT") return;
  const stages = state.chain?.stages;
  if (!stages) return;
  const index = stages.findIndex((s) => s.id === state.stageId);
  let next = index;
  if (event.key === "ArrowDown" || event.key === "j") next = Math.min(stages.length - 1, index + 1);
  else if (event.key === "ArrowUp" || event.key === "k") next = Math.max(0, index - 1);
  else return;
  event.preventDefault();
  state.stageId = stages[next].id;
  state.selected = null;
  renderPasses();
  renderGraphs();
  render();
  fit();
  renderInspector();
});

window.addEventListener("resize", () => fit());
boot().catch((error) => {
  const splash = document.getElementById("loading");
  if (splash) {
    splash.textContent = `Could not load the chain: ${error.message}`;
    splash.style.color = "var(--edge-false)";
  }
  console.error(error);
});

window.cfgviz = {
  state, render, fit, select, showGraph, loadChain, currentGraph,
  measure, blockLines, callOverlay,
};
