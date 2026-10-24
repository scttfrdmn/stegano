# stegano documentation

Documentation for stegano v0.8.1, a Rust library and CLI that embeds an
optionally encrypted payload in genomics, imaging, astronomy and audio files.
Start with the project [README](../README.md). Read
[the threat model and limitations](../SECURITY.md#threat-model-and-limitations)
before using stegano for anything that matters.

| Document | Contents |
|----------|----------|
| [GETTING_STARTED.md](GETTING_STARTED.md) | Installing from git, CLI quick start (all `embed`/`extract` options, password sources), first library programs, including the streaming API. |
| [API_REFERENCE.md](API_REFERENCE.md) | Core types, `EmbedParams` builders and defaults, ciphers, handlers, `stream`, `carrier`, `validation`, Cargo features, and the C, Go, Python and npm bindings. |
| [FORMATS.md](FORMATS.md) | Per format: strategies and their `format_specific` keys, capacity, exactly what changes in the file and how visible it is, external validators, and the streaming matrix. |
| [WIRE_FORMAT.md](WIRE_FORMAT.md) | Byte layout of the encoded payload (wire format v2): header, Argon2id, AEAD, Reed-Solomon framing. |
| [VALIDATION_GUIDE.md](VALIDATION_GUIDE.md) | The statistical tests and LSB detectors (pairs of values, sample pairs analysis) and how to read their scores. |
| [archive/](archive/README.md) | Superseded and historical documents. Not accurate for the current release. |

For the complete API, run `cargo doc --open`. Binding-specific setup is in
[go/README.md](../go/README.md), [npm/README.md](../npm/README.md) and
[include/stegano.h](../include/stegano.h).

## Performance

There are no published benchmark figures. To measure on your own hardware:

```sh
cargo bench --features benchmarks               # embedding and validation suites
cargo bench --features benchmarks,experimental  # also the experimental ML suite
```

The `gpu`, `ml` and `monitoring` modules, behind the `experimental` feature
(implied by `gpu`, `metal` and `cuda`), are experimental and unsupported. The
embedding path does not use them, and `EmbedParams::use_gpu` / `--gpu` has no
effect on embedding.

Two figures have been measured and are recorded in the CHANGELOG for 0.7.0.
Streaming embed and extract on 1–2 GB FASTQ, VCF and BAM files peaked at about
70 MB RSS, most of it Argon2id's 64 MiB. A 1 GB FASTQ through the CLI took
about 71 MB streaming and 5.19 GB with `--no-stream`. Argon2id key derivation
runs once per embed or extract and is much slower in unoptimised builds.
