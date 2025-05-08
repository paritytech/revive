const fs = require("fs");
const path = require("path");
const { minify } = require("terser");

const SOLJSON_URI =
  "https://binaries.soliditylang.org/wasm/soljson-v0.8.30+commit.ab55807c.js";
const RESOLC_WASM_URI =
  process.env.RELEASE_RESOLC_WASM_URI || "http://127.0.0.1:8080/resolc.wasm";
const RESOLC_WASM_TARGET_DIR = path.join(
  __dirname,
  "../../target/wasm32-unknown-emscripten/release",
);
const RESOLC_JS = path.join(RESOLC_WASM_TARGET_DIR, "resolc.js");
const RESOLC_WEB_JS = path.join(RESOLC_WASM_TARGET_DIR, "resolc_web.js");

const resolcJs = fs.readFileSync(RESOLC_JS, "utf-8");

const packedJsContent = `
if (typeof importScripts === "function") {
  importScripts("${SOLJSON_URI}");

  var moduleArgs = {
    wasmBinary: (function () {
      var xhr = new XMLHttpRequest();
      xhr.open("GET", "${RESOLC_WASM_URI}", false);
      xhr.responseType = "arraybuffer";
      xhr.send(null);
      return new Uint8Array(xhr.response);
    })(),
    soljson: Module
  };
} else {
  console.log("Not a WebWorker, skipping Soljson and WASM loading.");
}

${resolcJs}

createRevive = createRevive.bind(null, moduleArgs);
`;

minify(packedJsContent)
  .then((minifiedJs) => {
    if (minifiedJs.error) {
      console.error("Error during minification:", minifiedJs.error);
      process.exit(1);
    }

    fs.writeFileSync(RESOLC_WEB_JS, minifiedJs.code, "utf-8");
    console.log(`Combined script written to ${RESOLC_WEB_JS}`);
  })
  .catch((err) => {
    console.error("Minification failed:", err);
    process.exit(1);
  });
