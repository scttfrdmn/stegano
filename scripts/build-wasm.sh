#!/usr/bin/env bash
# Build WebAssembly bindings for browser (ESM) and Node.js (CJS) targets.
#
# Requirements:
#   cargo install wasm-pack                    # wasm-pack >= 0.13
#   rustup target add wasm32-unknown-unknown   # needs a rustup toolchain;
#                                              # Homebrew's rustc has no wasm32 std
#
# Usage:
#   ./scripts/build-wasm.sh           # both targets
#   ./scripts/build-wasm.sh web       # ESM / browser only
#   ./scripts/build-wasm.sh nodejs    # CJS / Node.js only
#
# Output:
#   pkg-web/    — stegano.js, stegano.d.ts, stegano_bg.wasm, package.json
#   pkg-nodejs/ — stegano.js, stegano.d.ts, stegano_bg.wasm, package.json

set -euo pipefail

TARGET="${1:-all}"
FEATURES="--no-default-features --features wasm"

# Run from the crate root regardless of the caller's cwd.
cd "$(dirname "$0")/.."

# Prefer the rustup-managed toolchain: a Homebrew/distro rustc earlier on PATH
# lacks the wasm32 standard library and fails with "can't find crate for core".
if [ -x "${CARGO_HOME:-$HOME/.cargo}/bin/rustup" ]; then
    export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:${PATH}"
fi

command -v wasm-pack >/dev/null 2>&1 || {
    echo "error: wasm-pack not found (cargo install wasm-pack)" >&2
    exit 1
}
if command -v rustup >/dev/null 2>&1; then
    rustup target list --installed | grep -qx wasm32-unknown-unknown ||
        rustup target add wasm32-unknown-unknown
else
    echo "warning: rustup not found; assuming rustc has the wasm32-unknown-unknown target" >&2
fi

build_target() {
    local target="$1"
    local outdir="pkg-${target}"
    echo "==> Building wasm target=${target} → ${outdir}/"
    wasm-pack build \
        --target "${target}" \
        --out-dir "${outdir}" \
        --out-name "stegano" \
        ${FEATURES}

    # Patch the generated package.json with stegano-specific metadata.
    node - "${outdir}/package.json" <<'NODE'
const fs = require('fs');
const path = process.argv[2];
const pkg = JSON.parse(fs.readFileSync(path, 'utf8'));
pkg.name = "stegano-wasm";
pkg.description = "High-performance scientific data steganography — WebAssembly bindings";
pkg.keywords = ["steganography", "genomics", "scientific", "wasm", "bioinformatics"];
pkg.author = "Scott Friedman <scttfrdmn@gmail.com>";
pkg.repository = { type: "git", url: "https://github.com/scttfrdmn/stegano" };
pkg.license = "Apache-2.0";
fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + '\n');
console.log('Patched', path);
NODE
    echo "    Done: ${outdir}/"
}

case "${TARGET}" in
    web)    build_target web ;;
    nodejs) build_target nodejs ;;
    all)
        build_target web
        build_target nodejs
        ;;
    *)
        echo "Usage: $0 [web|nodejs|all]" >&2
        exit 1
        ;;
esac

echo ""
echo "Build complete."
echo "  Browser (ESM):  pkg-web/"
echo "  Node.js (CJS):  pkg-nodejs/"
echo ""
echo "Publish with:"
echo "  cd pkg-web && npm publish --access public"
