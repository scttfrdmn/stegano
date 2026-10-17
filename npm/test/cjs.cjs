'use strict';
// Node CommonJS entry point: require('stegano-wasm') → node/stegano.js.
const { runSuite } = require('./helpers.cjs');

const api = require('stegano-wasm');
const { version } = require('stegano-wasm/package.json');

runSuite('require', api, version).then(
    (n) => console.log(`require('stegano-wasm'): ${n} checks passed`),
    () => process.exit(1),
);
