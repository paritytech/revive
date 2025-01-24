const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const { minify } = require("terser");

const RESOLC_WASM_TARGET_DIR = path.join(
  __dirname,
  "../target/wasm32-unknown-emscripten/release",
);
const RESOLC_WASM = path.join(RESOLC_WASM_TARGET_DIR, "resolc.wasm");
const RESOLC_JS = path.join(RESOLC_WASM_TARGET_DIR, "resolc.js");
const RESOLC_JS_PACKED = path.join(RESOLC_WASM_TARGET_DIR, "resolc_packed.js");

const execShellCommand = (cmd) => {
  return execSync(cmd, {
    encoding: "utf-8",
    maxBuffer: 1024 * 1024 * 100,
  }).trim();
};

const wasmBase64 = execShellCommand(
  `lz4c --no-frame-crc --best --favor-decSpeed "${RESOLC_WASM}" - | tail -c +8 | base64 -w 0`,
);

const wasmSize = fs.statSync(RESOLC_WASM).size;

const miniLz4 = fs.readFileSync(
  path.join(__dirname, "utils/mini-lz4.js"),
  "utf-8",
);
const base64DecToArr = fs.readFileSync(
  path.join(__dirname, "utils/base64DecToArr.js"),
  "utf-8",
);
const resolcJs = fs.readFileSync(RESOLC_JS, "utf-8");

const packedJsContent = `
let moduleArgs = { wasmBinary: (function(source, uncompressedSize) {
  ${miniLz4}
  ${base64DecToArr}
  return uncompress(base64DecToArr(source), uncompressedSize);
})("${wasmBase64}", ${wasmSize}),
};

${resolcJs}

createRevive = createRevive.bind(null, moduleArgs);
`;

minify(packedJsContent)
  .then((minifiedJs) => {
    if (minifiedJs.error) {
      console.error("Error during minification:", minifiedJs.error);
      process.exit(1);
    }

    fs.writeFileSync(RESOLC_JS_PACKED, minifiedJs.code, "utf-8");
    console.log(`Combined script written to ${RESOLC_JS_PACKED}`);
  })
  .catch((err) => {
    console.error("Minification failed:", err);
    process.exit(1);
  });
