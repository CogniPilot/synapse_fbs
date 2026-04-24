# synapse_fbs

FlatBuffers schemas and generated language bindings for Synapse.

This repository is intentionally small:

- `fbs/*.fbs`: canonical FlatBuffers schemas
- `c/`: C release archive skeleton, published as `synapse_fbs-c.tar.gz`
- `cpp/`: C++ release archive skeleton, published as `synapse_fbs-cpp.tar.gz`
- `rust/`: Rust package skeleton, published as `synapse_fbs`
- `python/`: Python package skeleton, published as `synapse-fbs`
- `tools.lock`: pinned package, generator, and runtime versions

Generated Rust and Python package trees are intentionally not written into the
source tree. CI copies each package skeleton into `target/xtask/packages/`,
renders `.jinja` templates there, and generates bindings from
`fbs/synapse_all.fbs` before building release packages.

Generation is version-locked from `tools.lock`. CI builds a vendored `flatc`
from `flatbuffers-build = "=0.2.4+flatc-25.12.19"` and verifies that the
compiler reports `flatc version 25.12.19`. The Rust package depends on
`flatbuffers = "=25.12.19"` and the Python package depends on
`flatbuffers==25.12.19` so generated code and runtimes stay in lockstep. CI
also builds pinned FlatCC and publishes generated C and C++ archives for
downstream CMake consumers.

ROS packages should consume this repository as a dependency or git submodule and
generate their own ROS interfaces/adapters outside this repo.

## Rust

Use the published crate:

```toml
synapse_fbs = "0.1"
```

After a local xtask build, use the staged crate:

```toml
synapse_fbs = { path = "../synapse_fbs/target/xtask/packages/rust" }
```

## Python

Install the published package:

```sh
pip install synapse-fbs
```

After a local xtask build, install the staged wheel:

```sh
pip install target/xtask/packages/python/dist/*.whl
```

## Local Build

Run the same Rust task that CI runs:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- ci
```

The task builds pinned `flatc` and FlatCC, stages Rust/Python packages under
`target/xtask/packages/`, creates the C/C++ tarballs under
`target/xtask/artifacts/`, and smoke-tests the C archive through CMake
`FetchContent`.

## C Archives

Release CI publishes `synapse_fbs-c.tar.gz` and `synapse_fbs-c.tar.gz.sha256`.
This is not a separate package or west module. Downstream firmware projects
should fetch the archive where they need it, for example:

```cmake
include(FetchContent)

FetchContent_Declare(
  synapse_fbs
  URL https://github.com/CogniPilot/synapse_fbs/releases/download/v0.1.0/synapse_fbs-c.tar.gz
  URL_HASH SHA256=<release sha256>
  DOWNLOAD_EXTRACT_TIMESTAMP TRUE
)
FetchContent_MakeAvailable(synapse_fbs)

target_link_libraries(app PRIVATE synapse_fbs::c)
```

For Zephyr, put the same `FetchContent_Declare` before linking your application
target, then link `synapse_fbs::c` into `app`. Link
`synapse_fbs::flatcc_runtime` only when using generated builders, verifiers, or
JSON helpers. Reader accessors are header-only.

## Releases

CI generates bindings and builds both packages on pull requests and branch
pushes.

Pushing a tag like `v0.1.0` publishes:

- staged `target/xtask/packages/rust/` to crates.io using `CARGO_REGISTRY_TOKEN`
- staged `target/xtask/packages/python/dist/` to PyPI using trusted publishing
- GitHub Release assets:
  - Python wheel and sdist
  - Rust `.crate` source package
  - C++ generated header tarball with matching FlatBuffers C++ runtime headers
  - C generated header tarball with matching FlatCC headers and runtime sources

The generated C archive is intentionally generic. Downstream firmware projects
that need it should fetch the release tarball directly from their own CMake
using a versioned URL and `URL_HASH SHA256=...`.
