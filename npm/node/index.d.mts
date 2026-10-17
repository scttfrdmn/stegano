// Types for the Node.js ESM entry point (./index.mjs).
export {
    stegano_embed,
    stegano_extract,
    stegano_estimate_capacity,
    stegano_version,
    WasmCapacityInfo,
} from './stegano.js';

/** Accepted for API compatibility with the web build; ignored in Node. */
export function initSync(module?: unknown): unknown;

/**
 * Accepted for API compatibility with the web build. In Node the module is
 * instantiated on import, so this resolves immediately.
 */
export default function init(moduleOrPath?: unknown): Promise<unknown>;
