// ESM entry point for Node.js.
//
// The Node build (./stegano.js, CommonJS) instantiates the WebAssembly module
// synchronously when it is loaded, so no initialisation is needed. A no-op
// async default export keeps `import init, { ... } from 'stegano-wasm';
// await init();` working identically in browsers, bundlers and Node.
import stegano from './stegano.js';

export const stegano_embed = stegano.stegano_embed;
export const stegano_extract = stegano.stegano_extract;
export const stegano_estimate_capacity = stegano.stegano_estimate_capacity;
export const stegano_version = stegano.stegano_version;
export const WasmCapacityInfo = stegano.WasmCapacityInfo;

/** No-op: the Node build is ready as soon as it is imported. */
export function initSync(_module) {
    return stegano.__wasm;
}

/** No-op: the Node build is ready as soon as it is imported. */
export default async function init(_moduleOrPath) {
    return stegano.__wasm;
}
