# Fuzz targets

[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) targets for the compiler: the two language
front ends and the shared backend. They check one property: **no input makes Unfold panic,
overflow its stack, or hang**. Malformed source must become a reported failure. The `link_resolve`
target also checks that a graph built from facts that extracted always validates.

| Target | Input | What runs |
| --- | --- | --- |
| `typescript_extract` | Arbitrary bytes | TypeScript extraction as `.ts` and `.tsx` (default and legacy-target options), the facts-cache round trip, and highlighting |
| `python_extract` | Arbitrary bytes | Python extraction under every grammar version (3.12–3.14), the round trip, and highlighting |
| `link_resolve` | Up to six NUL-separated files | Extraction, then linking, resolution, entrance detection and assembly, and validation of the whole mixed program (see below) |
| `facts_decode` | Arbitrary bytes | Decoding a facts-cache entry, then linking and resolving whatever decodes, as a TypeScript file and as a Python file |

In `link_resolve`, the first byte of each chunk picks what the rest of the chunk is, taken modulo 8:

| Value | File |
| --- | --- |
| 0 | `f<i>.ts` |
| 1 | `f<i>.tsx` |
| 2 | `f<i>.py` |
| 3 | `pkg/f<i>.py` |
| 4 | `src/f<i>.mts` |
| 5 | `package.json` |
| 6 | `tsconfig.json` |
| 7 | `pyproject.toml` |

`<i>` is the chunk's position, so `import "./f1"` in chunk 0 can reach chunk 1. A chunk can't
start with byte 0 (that byte separates chunks), so write 8 for a `.ts` file.

The fuzz crate is its own workspace, and the root `Cargo.toml` excludes it, so stable builds never
compile it.

## Running

You need a nightly toolchain and cargo-fuzz:

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
```

From this folder:

```sh
cargo +nightly fuzz list
cargo +nightly fuzz run typescript_extract -O -- -max_total_time=60 -timeout=10
cargo +nightly fuzz run link_resolve -O -- -max_len=8192 -timeout=10
```

Seed the corpora from real code to reach deeper paths faster. Each target reads `corpus/<target>/`,
and `corpus/` is ignored by git. For example:

```sh
mkdir -p corpus/typescript_extract corpus/python_extract
find ../testdata/playgrounds/ts-orders -name '*.ts' -exec cp {} corpus/typescript_extract/ \;
find ../testdata/playgrounds/py-orders -name '*.py' -exec cp {} corpus/python_extract/ \;
```

`facts_decode` gets further from well-formed cache entries. Write the encoded facts of every
playground source to `corpus/facts_decode/` with:

```sh
cargo +nightly run --release --example facts_seeds
```

When an input crashes, libFuzzer writes it to `artifacts/<target>/`. Replay or shrink it with:

```sh
cargo +nightly fuzz run <target> -O artifacts/<target>/crash-…
cargo +nightly fuzz tmin <target> -O artifacts/<target>/crash-…
```

Turn every crash you fix into a regression test in the owning crate's `tests/`.

The nightly workflow (`.github/workflows/nightly.yml`, job `fuzz`) runs each target for two
minutes on Linux.
