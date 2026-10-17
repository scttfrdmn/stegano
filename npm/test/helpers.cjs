'use strict';
// Shared helpers for the stegano-wasm smoke tests: carrier generators, a
// minimal PNG reader for validating the stego output, and the test body that
// runs the same assertions against every entry point.

const assert = require('node:assert/strict');
const zlib = require('node:zlib');

// ---------------------------------------------------------------------------
// PNG
// ---------------------------------------------------------------------------

const CRC_TABLE = (() => {
    const t = new Uint32Array(256);
    for (let n = 0; n < 256; n++) {
        let c = n;
        for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
        t[n] = c >>> 0;
    }
    return t;
})();

function crc32(buf) {
    let c = 0xffffffff;
    for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type, 'ascii'), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(td));
    return Buffer.concat([len, td, crc]);
}

const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

/** A valid 8-bit RGB PNG with a smooth gradient (filter type 0 on every row). */
function makePng(width = 96, height = 96) {
    const ihdr = Buffer.alloc(13);
    ihdr.writeUInt32BE(width, 0);
    ihdr.writeUInt32BE(height, 4);
    ihdr[8] = 8; // bit depth
    ihdr[9] = 2; // colour type: RGB
    ihdr[10] = 0; // compression
    ihdr[11] = 0; // filter
    ihdr[12] = 0; // interlace
    const raw = Buffer.alloc(height * (1 + width * 3));
    let o = 0;
    for (let y = 0; y < height; y++) {
        raw[o++] = 0; // filter: None
        for (let x = 0; x < width; x++) {
            raw[o++] = (x * 255) / (width - 1);
            raw[o++] = (y * 255) / (height - 1);
            raw[o++] = ((x + y) * 7) & 0xff;
        }
    }
    return new Uint8Array(
        Buffer.concat([
            PNG_SIGNATURE,
            chunk('IHDR', ihdr),
            chunk('IDAT', zlib.deflateSync(raw)),
            chunk('IEND', Buffer.alloc(0)),
        ]),
    );
}

/** Parse a PNG enough to prove it is well formed: signature, chunk CRCs, IHDR, inflatable IDAT. */
function parsePng(bytes) {
    const buf = Buffer.from(bytes);
    assert.ok(buf.subarray(0, 8).equals(PNG_SIGNATURE), 'PNG signature');
    let off = 8;
    let ihdr = null;
    const idat = [];
    let sawEnd = false;
    while (off < buf.length) {
        const len = buf.readUInt32BE(off);
        const type = buf.toString('ascii', off + 4, off + 8);
        const data = buf.subarray(off + 8, off + 8 + len);
        const crc = buf.readUInt32BE(off + 8 + len);
        assert.equal(crc, crc32(buf.subarray(off + 4, off + 8 + len)), `CRC of ${type}`);
        if (type === 'IHDR') ihdr = { width: data.readUInt32BE(0), height: data.readUInt32BE(4) };
        if (type === 'IDAT') idat.push(data);
        if (type === 'IEND') sawEnd = true;
        off += 12 + len;
    }
    assert.ok(ihdr && sawEnd && idat.length > 0, 'IHDR, IDAT and IEND present');
    const raw = zlib.inflateSync(Buffer.concat(idat));
    return { ...ihdr, raw };
}

// ---------------------------------------------------------------------------
// Raw / BINARY carrier
// ---------------------------------------------------------------------------

/** Generic bytes that match no known format, so they take the BINARY handler. */
function makeBinary(size = 16384) {
    const out = new Uint8Array(size);
    for (let i = 0; i < size; i++) out[i] = i & 0xff;
    return out;
}

// ---------------------------------------------------------------------------
// Shared test body
// ---------------------------------------------------------------------------

const ms = (t0) => Number(process.hrtime.bigint() - t0) / 1e6;

/**
 * Run the full set of assertions against one loaded API object.
 * `label` names the entry point in the output; `expectedVersion` is the
 * package.json version.
 */
async function runSuite(label, api, expectedVersion) {
    const results = [];
    const check = async (name, fn) => {
        try {
            await fn();
            results.push(name);
            console.log(`  ok   ${label}: ${name}`);
        } catch (e) {
            console.error(`  FAIL ${label}: ${name}\n${e && e.stack ? e.stack : e}`);
            throw e;
        }
    };

    for (const fn of ['stegano_embed', 'stegano_extract', 'stegano_estimate_capacity', 'stegano_version']) {
        assert.equal(typeof api[fn], 'function', `${fn} is exported`);
    }

    await check('stegano_version() equals package version', () => {
        assert.equal(api.stegano_version(), expectedVersion);
    });

    const png = makePng();
    const bin = makeBinary();
    const payload = new TextEncoder().encode(`hello from ${label} — ${'x'.repeat(40)}`);
    const timings = {};

    await check('estimate_capacity (PNG, BINARY)', () => {
        const p = api.stegano_estimate_capacity(png);
        assert.equal(p.format(), 'PNG');
        assert.ok(p.total_bits > 0 && p.recommended_bits > 0 && p.recommended_bits <= p.total_bits);
        p.free();
        const b = api.stegano_estimate_capacity(bin);
        assert.equal(b.format(), 'BINARY');
        assert.ok(b.total_bits > 0);
        b.free();
    });

    for (const [kind, carrier] of [
        ['PNG', png],
        ['BINARY', bin],
    ]) {
        await check(`${kind} roundtrip without password (defaults via "")`, async () => {
            const stego = await api.stegano_embed(carrier, payload, '', '', '', '');
            assert.ok(stego instanceof Uint8Array);
            assert.notDeepEqual(stego, carrier);
            const out = await api.stegano_extract(stego, '', '', '');
            assert.deepEqual(new Uint8Array(out), payload);
            if (kind === 'PNG') {
                const before = parsePng(carrier);
                const after = parsePng(stego);
                assert.equal(after.width, before.width);
                assert.equal(after.height, before.height);
                assert.equal(after.raw.length, before.raw.length);
            }
        });

        for (const cipher of ['aes256gcm', 'chacha20']) {
            await check(`${kind} roundtrip with password (${cipher}, Argon2id)`, async () => {
                let t0 = process.hrtime.bigint();
                const stego = await api.stegano_embed(carrier, payload, 'correct horse', 'high', 'none', cipher);
                const tEmbed = ms(t0);
                t0 = process.hrtime.bigint();
                const out = await api.stegano_extract(stego, 'correct horse', 'none', cipher);
                const tExtract = ms(t0);
                assert.deepEqual(new Uint8Array(out), payload);
                timings[`${kind}/${cipher}`] = { embed_ms: Math.round(tEmbed), extract_ms: Math.round(tExtract) };
                await assert.rejects(api.stegano_extract(stego, 'wrong password', 'none', cipher));
            });
        }
    }

    await check('unknown option names reject', async () => {
        await assert.rejects(api.stegano_embed(bin, payload, '', 'ultra', '', ''));
    });

    console.log(`  timings ${label} (password embed/extract, ms): ${JSON.stringify(timings)}`);
    return results.length;
}

module.exports = { makePng, parsePng, makeBinary, runSuite };
