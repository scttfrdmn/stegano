#!/usr/bin/env bash
# Build the WebAssembly bindings and assemble the `stegano-wasm` npm package.
#
# Requirements:
#   cargo install wasm-pack                    # wasm-pack >= 0.13
#   rustup target add wasm32-unknown-unknown   # needs a rustup toolchain;
#                                              # Homebrew's rustc has no wasm32 std
#   node / npm                                 # only for the smoke tests
#
# Usage:
#   ./scripts/build-wasm.sh           # build both targets and assemble pkg/
#   ./scripts/build-wasm.sh web       # only the ESM / browser build → pkg-web/
#   ./scripts/build-wasm.sh nodejs    # only the CJS / Node.js build → pkg-nodejs/
#
# Output of the default mode (one npm package containing both builds):
#   pkg/package.json      generated from npm/package.template.json
#   pkg/README.md         from npm/README.md
#   pkg/LICENSE, NOTICE
#   pkg/web/              wasm-pack --target web    (ESM; call `await init()`)
#   pkg/node/             wasm-pack --target nodejs (CJS; loads .wasm synchronously)
#                         + index.mjs / index.d.mts ESM wrapper (no-op init)
#
# Test with:   node npm/test/run.mjs          (packs pkg/ and runs smoke tests)
# Publishing happens from CI on `v*` tags (.github/workflows/npm-publish.yml).

set -euo pipefail

TARGET="${1:-all}"

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

# wasm-pack build for one target into pkg-<target>/ (no package.json; the
# combined package gets its own).
build_target() {
    local target="$1"
    local outdir="pkg-${target}"
    echo "==> Building wasm target=${target} → ${outdir}/"
    rm -rf "${outdir}"
    wasm-pack build \
        --target "${target}" \
        --out-dir "${outdir}" \
        --out-name stegano \
        --no-pack \
        --release \
        --no-default-features --features wasm
}

assemble() {
    local version
    version="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n1)"
    [ -n "${version}" ] || { echo "error: could not read version from Cargo.toml" >&2; exit 1; }

    echo "==> Assembling pkg/ (stegano-wasm@${version})"
    rm -rf pkg
    mkdir -p pkg/web pkg/node

    for f in stegano.js stegano.d.ts stegano_bg.wasm; do
        cp "pkg-web/${f}" "pkg/web/${f}"
        cp "pkg-nodejs/${f}" "pkg/node/${f}"
    done
    cp npm/node/index.mjs npm/node/index.d.mts npm/node/package.json pkg/node/
    cp npm/README.md pkg/README.md
    cp LICENSE NOTICE pkg/

    node -e '
        const fs = require("fs");
        const [template, out, version] = process.argv.slice(1);
        const pkg = JSON.parse(fs.readFileSync(template, "utf8"));
        pkg.version = version;
        fs.writeFileSync(out, JSON.stringify(pkg, null, 2) + "\n");
    ' npm/package.template.json pkg/package.json "${version}"

    echo "    Done: pkg/"
}

case "${TARGET}" in
    web)    build_target web ;;
    nodejs) build_target nodejs ;;
    all)
        build_target web
        build_target nodejs
        assemble
        echo ""
        echo "Package assembled in pkg/. Smoke-test it with:"
        echo "  node npm/test/run.mjs"
        ;;
    *)
        echo "Usage: $0 [web|nodejs]   (no argument: build both and assemble pkg/)" >&2
        exit 1
        ;;
esac
