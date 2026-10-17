// Web (browser/bundler) build, exercised in Node: 'stegano-wasm/web' →
// web/stegano.js, initialised with the .wasm bytes (Node's fetch() cannot load
// file: URLs, so the default `await init()` only works in browsers/bundlers).
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import init, * as api from 'stegano-wasm/web';
import helpers from './helpers.cjs';

const require = createRequire(import.meta.url);
const { version } = require('stegano-wasm/package.json');

const wasmPath = fileURLToPath(import.meta.resolve('stegano-wasm/web/stegano_bg.wasm'));
await init({ module_or_path: readFileSync(wasmPath) });
try {
    const n = await helpers.runSuite('web', api, version);
    console.log(`import from 'stegano-wasm/web' (init with bytes): ${n} checks passed`);
} catch {
    process.exit(1);
}
