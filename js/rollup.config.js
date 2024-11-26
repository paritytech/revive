const babel = require('@rollup/plugin-babel');
const copy = require('rollup-plugin-copy');

const outputDirCJS = 'dist/revive-cjs';
const outputDirESM = 'dist/revive-esm';

module.exports = {
  input: ['src/resolc.js'],
  output: [
    {
      dir: outputDirCJS,
      format: 'cjs',
      exports: 'auto',
    },
    {
      dir: outputDirESM,
      format: 'esm',
    },
  ],
  plugins: [
    babel({
      exclude: 'node_modules/**',
      presets: ['@babel/preset-env'],
      babelHelpers: 'inline',
    }),
    copy({
      targets: [
        { src: 'src/resolc.wasm', dest: outputDirCJS },
        { src: 'src/resolc.wasm', dest: outputDirESM },
      ],
    }),
  ],
};
