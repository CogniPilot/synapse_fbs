# Development and releases

## Supported environment

The supported toolchain runs natively on Linux. Local Rust and C generation
requires:

- Rust 1.85 or newer, including Cargo, rustfmt, and Clippy
- CMake 3.20 or newer
- Ninja
- GNU Make
- Git
- C11 and C++ compilers
- `tar`, `gzip`, and `sha256sum`

Full package and release validation additionally requires Python 3.12 with
`venv` and `pip`, Python `build` 1.5.0, Python `twine` 7.0.0, plus Node 24
and npm.

## Public commands

Build the exact FlatBuffers and FlatCC revisions pinned in `tools.toml`:

```sh
make bootstrap
```

The source checkouts and native compiler builds are cached below
`target/xtask/toolchain`. The bootstrap command verifies both Git commits and
compiler versions on every run.

Run all fast verification:

```sh
make test
```

This performs `cargo fmt --check`, Clippy with warnings denied, the xtask unit
tests, FlatCC schema compilation, BFBS reflection checks, wire compatibility
checks, and catalog smoke tests.

Generate and verify packages for local consumers without creating a tag or
release:

```sh
make local
```

This stages the Rust package at `target/xtask/packages/rust` and the C package
at `target/xtask/packages/c`. It compiles both packages, cross-checks a
C-written MCAP file with the generated Rust reader, and verifies the CMake
FetchContent source override against the generated C directory. The local
command does not publish packages or fetch the release-only FlatBuffers and
MCAP source trees.

The first invocation fetches the pinned generator sources and any missing Cargo
crates. After those inputs are cached, enforce an offline replay of the local
generation path:

```sh
make local-offline
```

Build and verify all release packages:

```sh
make packages
```

Set the release name when validating a tag:

```sh
make packages RELEASE_NAME=v0.10.0
```

Run the complete branch CI sequence:

```sh
make ci
```

The direct xtask entry points remain available. Bootstrap must complete before
running build or CI directly:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- bootstrap
cargo run --locked --manifest-path xtask/Cargo.toml -- build --release-name local
cargo run --locked --manifest-path xtask/Cargo.toml -- ci --release-name local
```

## Generation flow

The source inputs are deliberately separated:

- `fbs/` contains only the authoritative FlatBuffers schemas.
- `templates/{rust,python,js,c,cpp}` contains package skeletons.
- `templates/xtask` contains MiniJinja templates for generated catalogs,
  checksums, BFBS source assets, and smoke programs.
- `tools.toml` contains the package, generator, and runtime pins.

`xtask` asks FlatCC to compile BFBS reflection schemas, then reads reflection
data to validate and generate metadata. It does not parse `.fbs` syntax
itself. The pinned upstream `flatc` remains only for official Rust, Python,
and C++ binding generation. FlatCC generates C bindings and BFBS.

`make bootstrap` creates native Git checkouts at the exact pinned commits,
builds `flatc` and `flatcc` with CMake and Ninja, and leaves the FlatCC
source tree available for packaging its portable runtime.

The Rust orchestration is split by responsibility: `main.rs` contains the CLI
flow, `protocol.rs` contains declarative routing policy, `schema.rs` adapts
BFBS reflection, `packaging.rs` builds packages, and `support.rs` contains
shared I/O and process helpers.

Generated outputs live below `target/xtask/` and are safe to remove:

- `toolchain`
- `packages/c`
- `packages/rust`
- `packages/python`
- `packages/js`
- `artifacts`
- build, check, smoke, and downloaded-source work directories

## Wire hashes

Schema hashes are the first 128 bits of SHA-256 over the BFBS bytes emitted by
FlatCC on every build. The schema-set hash is derived from those hashes and the
routing catalog. No checksum baseline is committed. Normal Zenoh consumers
compare the schema hash. Constrained endpoints compare the schema-set hash
before exchanging compact frames.

Published wire-type names should remain immutable. An incompatible payload
change gets a new wire type and topic so old and new consumers fail clearly
instead of silently decoding different layouts.

## Package verification

The package command:

1. stages Rust, Python, and JavaScript skeletons;
2. renders every generated text file through MiniJinja;
3. generates official language bindings and BFBS assets;
4. builds and tests the Rust crate;
5. builds, checks, installs, and imports the Python wheel;
6. type-checks, packs, installs, and imports the npm package;
7. assembles C and C++ archives with pinned runtimes;
8. builds CMake FetchContent and find-package consumers;
9. writes and reads real MCAP logs across languages;
10. emits schema and BFBS checksum manifests.

## Version pins

`tools.toml` is the single version manifest for the package version,
FlatBuffers, FlatCC, MCAP implementations, and TypeScript. Git commits are
pinned where an upstream source tree is compiled or packaged. Generated
bindings and their runtimes remain in lockstep.

## Release workflow

Pushes and pull requests run `make ci`. A tag matching `v*.*.*` runs the
release workflow. The tag must match `package.version` in `tools.toml`.

The release publishes:

- the staged Rust crate through crates.io trusted publishing;
- the Python wheel and source distribution through PyPI trusted publishing;
- the npm tarball through npm trusted publishing;
- C and C++ tarballs plus checksum files on the GitHub Release.

The repository and workflow identities must be registered with each package
registry before tagging. A failed publication can be retried safely because the
workflow checks which package versions already exist before publishing.

## CI action maintenance

CI uses Ubuntu 24.04, native system packages, Python 3.12, the Rust toolchain
action, and the Node 24 setup action. Validate workflow edits with a native
`actionlint` installation:

```sh
actionlint
```
