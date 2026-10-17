# stegano-wasm

WebAssembly build of [stegano](https://github.com/scttfrdmn/stegano), a Rust
library for steganography in scientific data formats. It embeds an optionally
encrypted payload into FASTA/FASTQ, VCF, SAM/BAM, PNG/TIFF, DICOM, WAV, FITS or
arbitrary binary carriers, and extracts it again. Everything runs locally; no
data leaves the page or process.

One package works in browsers, bundlers and Node.js (ESM and CommonJS), and
ships TypeScript declarations.

```sh
npm install stegano-wasm
```

## Usage

### Browser / bundler (Vite, webpack 5, Rollup, esbuild, ...)

```js
import init, { stegano_embed, stegano_extract, stegano_estimate_capacity } from 'stegano-wasm';

await init(); // fetches stegano_bg.wasm next to the module (new URL(..., import.meta.url))

const carrier = new Uint8Array(await (await fetch('image.png')).arrayBuffer());
const payload = new TextEncoder().encode('secret message');

const stego = await stegano_embed(carrier, payload, 'password', '', '', '');
const out = await stegano_extract(stego, 'password', '', '');
console.log(new TextDecoder().decode(out)); // 'secret message'
```

If your setup serves the `.wasm` from somewhere else, pass its URL or bytes:
`await init({ module_or_path: '/assets/stegano_bg.wasm' })`. The file is
exported as `stegano-wasm/web/stegano_bg.wasm` (for example, Vite's
`import wasmUrl from 'stegano-wasm/web/stegano_bg.wasm?url'`).

### Node.js, ESM

```js
import init, { stegano_embed, stegano_extract } from 'stegano-wasm';
import { readFile, writeFile } from 'node:fs/promises';

await init(); // no-op in Node: the module is loaded synchronously on import

const stego = await stegano_embed(await readFile('reads.fastq'), Buffer.from('hi'), 'pw', '', '', '');
await writeFile('reads.stego.fastq', stego);
```

### Node.js, CommonJS

```js
const { stegano_embed, stegano_extract, stegano_version } = require('stegano-wasm');
// Ready immediately, no init() needed.
```

Under Node the `node` export condition selects a build that loads the
`.wasm` synchronously from disk. To use the browser build in Node anyway (for
example in tests), import `stegano-wasm/web` and pass the bytes to `init`,
because Node's `fetch` cannot read `file:` URLs:

```js
import init from 'stegano-wasm/web';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
await init({ module_or_path: readFileSync(fileURLToPath(import.meta.resolve('stegano-wasm/web/stegano_bg.wasm'))) });
```

## API

All byte arguments accept a `Uint8Array` (a Node `Buffer` works too). In each
string option, `''` selects the library default, and an unknown name causes a
rejection or throw rather than being ignored.

### `stegano_embed(carrier, payload, password, stealth, ec_level, cipher): Promise<Uint8Array>`

Returns the modified carrier. The output can be longer than the input for
SAM, VCF and DICOM carriers.

| Argument   | Values                                                      | Default (`''`) |
|------------|-------------------------------------------------------------|----------------|
| `password` | any string; `''` embeds **without encryption**              | none           |
| `stealth`  | `'low'` \| `'medium'` \| `'high'` \| `'maximum'`            | `'high'`       |
| `ec_level` | `'none'` \| `'low'` \| `'medium'` \| `'high'` \| `'maximum'`| `'none'`       |
| `cipher`   | `'aes256gcm'` \| `'chacha20'`                               | `'aes256gcm'`  |

### `stegano_extract(carrier, password, ec_level, cipher): Promise<Uint8Array>`

Returns the embedded payload. It rejects if no payload is present, the
password is wrong or the data was tampered with (authenticated encryption).
The cipher is detected from the payload header automatically. `ec_level`
matters only for payloads embedded **without** a password: if error
correction was used at embed time, pass any value other than `'none'` (the
level itself is read from the payload).

### `stegano_estimate_capacity(carrier): WasmCapacityInfo`

This call is synchronous. The returned object has `total_bits`,
`recommended_bits` and `format()`, which returns the detected format name, for
example `'PNG'`, `'FASTA'` or `'BINARY'`. Call `.free()` when you are done to
release the WASM-side object right away (otherwise a finalizer releases it
later).

### `stegano_version(): string`

Returns the library version. It is the same as the npm package version.

### `init(moduleOrPath?)` (default export)

Loads the WebAssembly module. You must await it once before calling anything
else in browsers and bundlers. In Node it does nothing, but it is safe to call,
so the same code runs everywhere.

## Supported carrier formats

The format is detected **from the content** (magic bytes, then format
probing), because there is no file name:

| Format | Detected by |
|--------|-------------|
| FASTA / FASTQ | `>` / `@` record headers |
| VCF | `##fileformat=VCF` |
| SAM / BAM | `@HD`/`@SQ`/`@PG` header lines, BAM magic |
| PNG / TIFF | PNG signature, `II*\0` / `MM\0*` |
| DICOM | `DICM` preamble |
| WAV | `RIFF....WAVE` |
| FITS | `SIMPLE  =` |
| anything else | generic binary embedding (`BINARY`) |

The CLI and the Rust, Python and Go APIs offer more options, such as
per-format strategy selection.

## Performance note: password-based encryption

With a password, the key is derived with **Argon2id (64 MiB memory, 3
passes)**, so both embed and extract do one deliberately expensive key
derivation and grow the WASM memory by about 64 MiB. In Node 26 on an Apple
Silicon laptop, each password embed or extract of a small payload took about
**0.1 s**. Expect several times that on low-end phones or in some browsers.
Without a password, operations are fast (milliseconds, depending on the
carrier size). The calls are `async`, but the work runs on the calling thread.
To keep a UI responsive, run them in a Web Worker.

## Security

When the password is empty, the payload is hidden but **not encrypted**. See
[SECURITY.md](https://github.com/scttfrdmn/stegano/blob/main/SECURITY.md)
before you rely on stegano for confidentiality.

## License

Apache-2.0. Source code and issues are at
<https://github.com/scttfrdmn/stegano>.
