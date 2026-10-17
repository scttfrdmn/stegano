// Node ESM entry point: import from 'stegano-wasm' → node/index.mjs.
// Uses the same `await init()` pattern as the browser build.
import { createRequire } from 'node:module';
import init, * as api from 'stegano-wasm';
import helpers from './helpers.cjs';

const require = createRequire(import.meta.url);
const { version } = require('stegano-wasm/package.json');

await init();
try {
    const n = await helpers.runSuite('import', api, version);
    console.log(`import from 'stegano-wasm': ${n} checks passed`);
} catch {
    process.exit(1);
}
