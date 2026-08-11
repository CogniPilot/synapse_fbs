# synapse_fbs

[![CI](https://github.com/CogniPilot/synapse_fbs/actions/workflows/ci.yml/badge.svg)](https://github.com/CogniPilot/synapse_fbs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/synapse_fbs)](https://crates.io/crates/synapse_fbs)
[![PyPI](https://img.shields.io/pypi/v/synapse-fbs)](https://pypi.org/project/synapse-fbs/)
[![npm](https://img.shields.io/npm/v/@cognipilot/synapse-fbs)](https://www.npmjs.com/package/@cognipilot/synapse-fbs)

FlatBuffers schemas and generated language packages for Synapse.

This repository is the schema source of truth for vehicle state, sensor,
control, transport, and transfer messages. The checked-in source stays small:
FlatBuffers schemas live in `fbs/`, package inputs live in `templates/`, and a
pinned Nix toolchain generates and verifies every release artifact.

## Design

Synapse uses one semantic message set across shared memory, Zenoh, constrained
radio links, and MCAP logs. Runtime telemetry and control payloads favor
fixed-layout FlatBuffers structs so the same little-endian bytes can move
between processors and transports without re-serialization.

Raw sensor topics preserve source conventions. Estimates and commands use ENU
world vectors, FLU body vectors, SI units, and Hamilton quaternions in `w x y
z` order. Validity is explicit through schema-defined flags; sentinel values
are not part of the wire contract.

Zenoh keys are short and deployment-oriented:

```text
[<namespace>/]<topic_key>[/<instance>]
[<namespace>/]cmd/<command_name>
```

Examples include `cub1/odom`, `cub1/imu/0`, and
`cub1/cmd/firmware_prepare`. Keys do not identify the binary schema. Normal
Zenoh values carry the generated encoding, wire type, and transitive schema
hash; constrained links compare the generated catalog hash before exchanging
numeric topic IDs and bare payloads.

See [architecture](docs/architecture.md), [design use
cases](docs/USE_CASES.md), and the normative [`synapse/1` MCAP
profile](docs/MCAP.md) for the full contracts.

## Repository layout

- `fbs/` — authoritative `.fbs` schemas.
- `templates/` — Rust, Python, JavaScript, C, C++, and xtask MiniJinja inputs.
- `xtask/` — FlatCC-reflection-driven generation and verification.
- `docs/` — architecture, package, development, MCAP, and use-case details.
- `flake.nix` — pinned tool versions and the public build/test commands.

Generated files are written only beneath `target/xtask/`:

- `target/xtask/packages/{rust,python,js}`
- `target/xtask/artifacts/synapse_fbs-{c,cpp}.tar.gz`
- `target/xtask/check` and other temporary verification trees

FlatCC generates C bindings and every BFBS reflection schema consumed by
`xtask`. The pinned upstream `flatc` is used only for official Rust, Python,
and C++ binding generation. All generated text owned by `xtask` is rendered
from MiniJinja templates.

## Build and test

Nix is the supported interface on `x86_64-linux` and `aarch64-linux`. The same
commands are used locally and by GitHub Actions.

Run formatting, lint, schema validation, compatibility checks, and catalog
smoke tests:

```sh
nix run .#test
```

Build and verify all Rust, Python, JavaScript, C, and C++ packages:

```sh
nix run .#packages
```

Run both exactly as CI does:

```sh
nix run .#ci
```

`nix run .#build` is an alias for `nix run .#packages`. `nix develop` provides
the same pinned toolchain plus `synapse-fbs-test`, `synapse-fbs-packages`, and
`synapse-fbs-ci` commands. The shell prints this list when it starts.

To test a change without publishing, run `nix run .#packages`, then consume the
staged output directly:

```sh
pip install target/xtask/packages/python/dist/*.whl
npm install ./target/xtask/packages/js
```

Rust projects can set a path dependency to `target/xtask/packages/rust`. C and
C++ projects can extract an archive from `target/xtask/artifacts/` and add its
root to `CMAKE_PREFIX_PATH`. More examples are in [package
usage](docs/packages.md).

## Published packages

- Rust: [`synapse_fbs`](https://crates.io/crates/synapse_fbs)
- Python: [`synapse-fbs`](https://pypi.org/project/synapse-fbs/)
- JavaScript: [`@cognipilot/synapse-fbs`](https://www.npmjs.com/package/@cognipilot/synapse-fbs)
- C and C++: generated archives attached to [GitHub
  releases](https://github.com/CogniPilot/synapse_fbs/releases)

Each package includes the generated topic catalog. Packages that support MCAP
also include the `synapse/1` reader/writer surface and the matching BFBS schema
assets. Installation, CMake targets, and local staged-package examples are in
[package usage](docs/packages.md).

## Schema changes

Edit only the authoritative files in `fbs/`, then run:

```sh
nix run .#test
```

`xtask` asks FlatCC to compile BFBS and uses reflection to validate topic IDs,
the `SynapseMessage` union, command request/reply types, fixed payload sizes,
field offsets, and generated wire hashes. It does not contain a second
`.fbs` parser or documentation compiler.

Wire hashes are computed from FlatCC reflection on every build and are not
committed. Consumers compare them at runtime. If a published payload changes
incompatibly, introduce a new wire type and topic.

Never hand-edit generated package trees under `target/`.

## Releases

Version pins and the package version live in `flake.nix`. A semantic version
tag such as `v0.8.0` must match `package.version`; the release workflow rejects
a mismatch before publishing.

Release CI builds the same package set as `nix run .#packages`, then publishes
the Rust crate, Python wheel and source distribution, npm package, and C/C++
archives. See [development and releases](docs/development.md) for artifact and
trusted-publishing details.

## Documentation

- [Architecture and wire conventions](docs/architecture.md)
- [Package usage](docs/packages.md)
- [Development and releases](docs/development.md)
- [Normative MCAP profile](docs/MCAP.md)
- [Design use cases](docs/USE_CASES.md)
- [Documentation index](docs/README.md)
