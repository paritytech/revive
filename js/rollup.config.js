import babel from '@rollup/plugin-babel';
import copy from 'rollup-plugin-copy';
import resolve from '@rollup/plugin-node-resolve';

const outputDirCJS = 'dist/revive-cjs';
const outputDirESM = 'dist/revive-esm';

export default {
  input: ['src/resolc.js', 'src/worker.js'],  // Adjust this to your main entry file
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
    resolve(),
    copy({
        targets: [
          { src: 'src/resolc.wasm', dest: outputDirCJS },
          { src: 'src/resolc.wasm', dest: outputDirESM },
        ],
      })
  ],
};
