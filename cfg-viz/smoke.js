"use strict";

// Drives app.js over every stage of every bundled chain against a stub DOM.
//
// There is no browser in this environment, so the alternative to this harness
// is shipping the viewer unexercised — which is how the blank-splash bug got
// out. It does not check that anything *looks* right; it checks that the
// render path runs to completion over real data, which is the failure mode a
// static read misses.
//
//   node smoke.js

const fs = require("fs");
const path = require("path");
const vm = require("vm");

const ROOT = __dirname;

// ---------------------------------------------------------------------------
// A DOM stub with just the surface app.js touches.
// ---------------------------------------------------------------------------

class Element {
  constructor(tag) {
    this.tagName = String(tag).toUpperCase();
    this.children = [];
    this.attributes = {};
    this.listeners = {};
    this.style = { setProperty() {} };
    this.dataset = {};
    this._text = "";
    this.classList = {
      _set: new Set(),
      add: (c) => this.classList._set.add(c),
      remove: (c) => this.classList._set.delete(c),
      toggle: (c, on) => (on ? this.classList._set.add(c) : this.classList._set.delete(c)),
      contains: (c) => this.classList._set.has(c),
    };
  }
  setAttribute(name, value) {
    if (value === undefined || value === null) throw new Error(`${this.tagName}: ${name} set to ${value}`);
    this.attributes[name] = String(value);
  }
  getAttribute(name) { return this.attributes[name]; }
  appendChild(child) {
    if (!child) throw new Error(`${this.tagName}.appendChild(${child})`);
    this.children.push(child);
    return child;
  }
  insertAdjacentHTML(_position, html) { this._html = (this._html || "") + html; }
  addEventListener(type, handler) { (this.listeners[type] ||= []).push(handler); }
  removeEventListener() {}
  remove() { this.removed = true; }
  scrollIntoView() {}
  querySelector() { return new Element("div"); }
  querySelectorAll() { return []; }
  getBoundingClientRect() { return { width: 1440, height: 820, left: 0, top: 0 }; }
  setPointerCapture() {}
  releasePointerCapture() {}
  get textContent() { return this._text; }
  set textContent(value) { this._text = String(value); this.children = []; }
  set innerHTML(value) { this._html = String(value); this.children = []; }
  get innerHTML() { return this._html || ""; }
  click() { for (const handler of this.listeners.click || []) handler({ target: this }); }
}

const byId = new Map();
const document = {
  createElement: (tag) => new Element(tag),
  createElementNS: (_ns, tag) => new Element(tag),
  getElementById: (id) => {
    if (!byId.has(id)) byId.set(id, new Element("div"));
    return byId.get(id);
  },
  addEventListener() {},
};

const sandbox = {
  document,
  console,
  setTimeout,
  clearTimeout,
  Math,
  JSON,
  Number,
  String,
  Set,
  Map,
  Object,
  Array,
  Error,
  dagre: require(path.join(ROOT, "vendor", "dagre.min.js")),
  fetch: async (url) => {
    const file = path.join(ROOT, url);
    if (!fs.existsSync(file)) return { ok: false, status: 404, json: async () => ({}) };
    return { ok: true, status: 200, json: async () => JSON.parse(fs.readFileSync(file, "utf8")) };
  },
};
sandbox.addEventListener = () => {};
sandbox.window = sandbox;
sandbox.globalThis = sandbox;

// ---------------------------------------------------------------------------

async function main() {
  const source = fs.readFileSync(path.join(ROOT, "app.js"), "utf8");
  vm.runInNewContext(source, sandbox, { filename: "app.js" });

  // app.js boots itself on load; wait for that to settle.
  await new Promise((resolve) => setTimeout(resolve, 50));

  const api = sandbox.window.cfgviz;
  if (!api) throw new Error("app.js did not expose window.cfgviz");

  const splash = byId.get("loading");
  if (!splash.removed) {
    throw new Error(`the loading splash was never removed (text: ${JSON.stringify(splash.textContent)})`);
  }

  let stages = 0;
  let graphs = 0;
  let blocks = 0;

  for (const chain of api.state.index.chains) {
    await api.loadChain(chain.id);
    for (const stage of chain.stages) {
      api.state.stageId = stage.id;
      const record = api.state.chainData[stage.id];
      if (!record) throw new Error(`${chain.id}: no lowered record for ${stage.id}`);
      for (const graph of record.graphs) {
        api.state.graphId = graph.id;
        api.state.selected = null;
        api.render();
        api.fit();
        // The inspector reads a different slice of each block than the canvas
        // does, so exercise it on a block that actually carries phis and uses.
        const interesting = graph.blocks.find((b) => b.phis.length && b.statements.length)
          || graph.blocks[0];
        api.select(interesting.id);
        api.state.query = "v23";
        api.render();
        api.state.query = "";
        graphs += 1;
        blocks += graph.blocks.length;
      }
      stages += 1;
    }
    console.log(`  ${chain.contract.padEnd(18)} ${chain.stages.length} stages rendered`);
  }

  console.log(`\nok — ${stages} stages, ${graphs} graphs, ${blocks} blocks rendered without error`);
}

main().catch((error) => {
  console.error("\nFAILED:", error.message);
  console.error(error.stack.split("\n").slice(1, 5).join("\n"));
  process.exit(1);
});
