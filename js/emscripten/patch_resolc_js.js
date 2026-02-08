// Emscripten 4.0.12+ wraps the MODULARIZE factory in an async function
// and always returns a Promise, even with WASM_ASYNC_COMPILATION=0.
//
// Since we compile with WASM_ASYNC_COMPILATION=0, there are no actual
// async operations — the Promise resolves immediately. We patch the
// generated resolc.js to restore synchronous behavior. This is required
// because soljson_interface.js calls createRevive() from a synchronous
// C FFI context and cannot await a Promise.
//
// The patch performs three transformations:
// 1. Strips "async" from the factory function declaration
// 2. Removes all "await" keywords (safe: no real async ops)
// 3. Changes "return readyPromise" to "return Module" so the factory
//    returns the Module object directly instead of a Promise wrapper
const fs = require("fs");
const path = require("path");

const RESOLC_JS = path.join(
  __dirname,
  "../../target/wasm32-unknown-emscripten/release/resolc.js",
);

let content = fs.readFileSync(RESOLC_JS, "utf-8");

let modified = false;

// 1. Strip "async" from the factory function declaration.
//    Handles both IIFE wrapper and direct export patterns.
const asyncPatterns = [
  {
    from: "return async function(moduleArg",
    to: "return function(moduleArg",
  },
  {
    from: "async function createRevive(moduleArg",
    to: "function createRevive(moduleArg",
  },
];

for (const { from, to } of asyncPatterns) {
  if (content.includes(from)) {
    content = content.replace(from, to);
    modified = true;
    console.log(`Stripped async: "${from}" -> "${to}"`);
    break;
  }
}

// 2. Remove all "await" keywords.
//    With WASM_ASYNC_COMPILATION=0 all awaited values are not Promises,
//    so removing await just evaluates the expression directly.
const awaitCount = (content.match(/\bawait\s+/g) || []).length;
if (awaitCount > 0) {
  content = content.replace(/\bawait\s+/g, "");
  modified = true;
  console.log(`Removed ${awaitCount} await expression(s)`);
}

// 3. Return Module directly instead of the readyPromise wrapper.
//    Emscripten creates: var readyPromise = new Promise(resolve => { ... });
//    and returns it. We replace the return to give callers the Module object.
if (content.includes("return readyPromise")) {
  content = content.replace("return readyPromise", "return Module");
  modified = true;
  console.log('Changed "return readyPromise" to "return Module"');
}

if (!modified) {
  console.error(
    "Warning: No modifications were made to resolc.js. " +
      "The file may already be patched or the emscripten output format changed.",
  );
  process.exit(1);
}

fs.writeFileSync(RESOLC_JS, content, "utf-8");
console.log("Successfully patched resolc.js for synchronous module creation.");
