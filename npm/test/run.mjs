#!/usr/bin/env node
// Smoke tests for the stegano-wasm npm package, run against the packed tarball
// exactly as a consumer would install it.
//
//   node npm/test/run.mjs                    # npm pack pkg/, then test
//   node npm/test/run.mjs --tarball X.tgz    # test an existing tarball
//
// Steps: pack → install into a fresh temp project → run
//   cjs.cjs  require('stegano-wasm')          (node/stegano.js)
//   esm.mjs  import init, {...} 'stegano-wasm' (node/index.mjs)
//   web.mjs  import init 'stegano-wasm/web'   (web/stegano.js, init with bytes)
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';

const args = process.argv.slice(2);
const ti = args.indexOf('--tarball');
const keep = args.includes('--keep');

const work = mkdtempSync(join(tmpdir(), 'stegano-wasm-smoke-'));
let ok = false;
try {
    let tarball;
    if (ti >= 0) {
        tarball = resolve(args[ti + 1]);
    } else {
        const out = execFileSync(npm, ['pack', '--json', '--pack-destination', work], {
            cwd: join(repo, 'pkg'),
            encoding: 'utf8',
        });
        const [info] = JSON.parse(out);
        tarball = join(work, info.filename);
        console.log(`packed ${info.name}@${info.version}: ${info.entryCount} files, ` +
            `${(info.size / 1024).toFixed(0)} KiB packed, ${(info.unpackedSize / 1024).toFixed(0)} KiB unpacked`);
    }

    const app = join(work, 'app');
    mkdirSync(app);
    writeFileSync(join(app, 'package.json'), JSON.stringify({ name: 'smoke', private: true }, null, 2));
    execFileSync(npm, ['install', '--no-audit', '--no-fund', '--loglevel=error', tarball], {
        cwd: app,
        stdio: 'inherit',
    });
    for (const f of readdirSync(here)) {
        if (/\.(c|m)?js$/.test(f) && f !== 'run.mjs') copyFileSync(join(here, f), join(app, f));
    }

    for (const t of ['cjs.cjs', 'esm.mjs', 'web.mjs']) {
        console.log(`\n== ${t}`);
        execFileSync(process.execPath, [t], { cwd: app, stdio: 'inherit' });
    }
    ok = true;
    console.log('\nAll stegano-wasm smoke tests passed.');
} catch (e) {
    console.error(`\nstegano-wasm smoke tests FAILED${e.status ? ` (exit ${e.status})` : `: ${e.message}`}`);
    process.exitCode = 1;
} finally {
    if (keep) console.log(`kept ${work}`);
    else rmSync(work, { recursive: true, force: true });
}
if (!ok) process.exitCode = 1;
