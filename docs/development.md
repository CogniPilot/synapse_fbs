# Development and releases

## Tool requirements

CI uses Ubuntu 24.04 as its reproducible baseline. Development and packaging
are not tied to a particular Ubuntu release. A local environment needs Rust
and Cargo with edition 2024 support, CMake, C and C++ compilers, Git, gzip, GNU
tar, and `sha256sum`.

On Ubuntu and Debian systems, install the required native packages directly:

```sh
sudo apt-get update
sudo apt-get install --yes build-essential cargo cmake git gzip tar
```

Build the pinned upstream FlatBuffers 25.12.19 compiler:

```sh
git init /path/to/flatbuffers
git -C /path/to/flatbuffers remote add origin https://github.com/google/flatbuffers.git
git -C /path/to/flatbuffers fetch --depth 1 origin 7e163021e59cca4f8e1e35a7c828b5c6b7915953
git -C /path/to/flatbuffers checkout --detach FETCH_HEAD
cmake -S /path/to/flatbuffers -B /path/to/flatbuffers/out \
  -DCMAKE_BUILD_TYPE=Release \
  -DFLATBUFFERS_BUILD_TESTS=OFF
cmake --build /path/to/flatbuffers/out --target flatc --parallel
export SYNAPSE_FBS_FLATC=/path/to/flatbuffers/out/flatc
```

FlatCC is also built from a pinned source checkout. The C archive embeds its
runtime sources, so retain the checkout after building:

```sh
git init /path/to/flatcc
git -C /path/to/flatcc remote add origin https://github.com/dvidelabs/flatcc.git
git -C /path/to/flatcc fetch --depth 1 origin d17e324e7e595272da486c5b9b20e848b78ba9ba
git -C /path/to/flatcc checkout --detach FETCH_HEAD
cmake -S /path/to/flatcc -B /path/to/flatcc/out \
  -DCMAKE_BUILD_TYPE=Release \
  -DFLATCC_TEST=OFF \
  -DFLATCC_INSTALL=OFF
cmake --build /path/to/flatcc/out --target flatcc_cli --parallel
export SYNAPSE_FBS_FLATCC=/path/to/flatcc/bin/flatcc
export SYNAPSE_FBS_FLATCC_SOURCE=/path/to/flatcc
```

`xtask` resolves `flatc` and `flatcc` from PATH. The
`SYNAPSE_FBS_FLATC` and `SYNAPSE_FBS_FLATCC` environment variables can select
specific executables. `SYNAPSE_FBS_FLATCC_SOURCE` must identify the FlatCC
source tree.

## Public commands

Format and lint the Rust tooling:

```sh
cargo fmt --check --manifest-path xtask/Cargo.toml
cargo clippy --locked --manifest-path xtask/Cargo.toml --all-targets -- -D warnings
```

Validate schemas and generated catalogs:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- check
```

Check the compatibility baseline:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- wire-check
```

Build and verify all release packages:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- ci
```

Build artifacts without the complete verification pass:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- build
```

Pass `--release-name v0.10.0` after the command to reproduce a tagged release
locally. The task runner derives package version `0.10.0` from that tag. Builds
without a release name use development version `0.0.0`.

## Generation flow

The source inputs are deliberately separated:

- `fbs/` contains only the authoritative FlatBuffers schemas.
- `templates/{rust,c}` contains package skeletons.
- `templates/xtask` contains MiniJinja templates for generated catalogs,
  checksums, BFBS source assets, and smoke programs.

The task runner enforces this template-directory allowlist for every command.
Adding another generated binding language requires an explicit policy change.

`xtask` asks FlatCC to compile BFBS reflection schemas, then reads reflection
data to validate and generate metadata. It does not parse `.fbs` syntax itself.
Upstream `flatc` generates Rust bindings; FlatCC generates C bindings and BFBS.

The FlatCC source tree is used only when the portable C archive needs FlatCC
runtime source files.

The Rust orchestration is split by responsibility: `main.rs` contains the CLI
flow, `protocol.rs` contains declarative routing policy, `schema.rs` adapts BFBS
reflection, `packaging.rs` builds packages, and `support.rs` contains shared
I/O and process helpers.

Generated outputs live below `target/xtask/` and are safe to remove:

- `packages/rust`
- `artifacts`
- build, check, smoke, and downloaded-source work directories

## Wire identities

Each public `schema_hash` is a full 64-character SHA-256 identity over the named
wire type and its complete transitive dependency closure. The identity input is
a normalized, length-framed transcript derived from the include-expanded BFBS,
so unrelated type additions do not change an existing wire-type identity.

The separate `schema_artifact_sha256` and embedded `bfbs_sha256` values identify
the exact compiled BFBS bytes. `SCHEMA_SET_IDENTITY` covers catalog version,
routing, key and instance grammar, value-encoding literals, and all topic and
command wire contracts. `SCHEMA_PACKAGE_CONTRACT_IDENTITY` additionally covers
the complete generated schema package contract, including BFBS artifacts,
rooted catalog presentation, descriptions, and logging-profile literals.

The 32-character `LEGACY_SCHEMA_SET_HASH_128` and per-file legacy fields retain
the previous BFBS-prefix behavior for historical compatibility only. New Zenoh
consumers compare the full type identity, and constrained endpoints compare the
full schema-set identity before exchanging compact frames.

Published wire-type names should remain immutable. An incompatible payload
change gets a new wire type and topic so old and new consumers fail clearly
instead of silently decoding different layouts.

Use `wire-check --update` only when intentionally accepting a compatible
schema addition or a reviewed compatibility-policy exception. Commit the
resulting `compatibility/wire-schema.toml` change with the schema change.

## Package verification

The `ci` command:

1. stages the Rust and C package inputs;
2. renders generated text through MiniJinja;
3. generates language bindings and BFBS assets;
4. builds and tests the Rust crate;
5. assembles the C archive with its required runtime;
6. builds CMake `FetchContent` and `find_package` consumers;
7. exercises the MCAP implementations;
8. emits schema and BFBS checksum manifests.

## Release workflow

Pushes and pull requests run the direct Cargo verification commands. A stable
semantic version tag matching `v*.*.*` runs the release workflow and supplies
the version for every generated package. No separate version file or source
constant needs to be changed before tagging.

The release publishes:

- the staged Rust crate through crates.io trusted publishing;
- a C tarball plus its checksum file on the GitHub release.

The repository and workflow identity must be registered with crates.io before
tagging. A failed publication can be retried safely because the workflow checks
whether the crate version already exists before publishing.

## CI action maintenance

GitHub Actions invokes Cargo, the FlatBuffers tools, CMake, and the native
compilers directly. Validate workflow edits with an installed `actionlint`:

```sh
actionlint
```
