# synapse_fbs

[![CI](https://github.com/CogniPilot/synapse_fbs/actions/workflows/ci.yml/badge.svg)](https://github.com/CogniPilot/synapse_fbs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/synapse_fbs)](https://crates.io/crates/synapse_fbs)

FlatBuffers schemas and generated C and Rust packages for Synapse.

This repository is the schema source of truth for vehicle state, sensor,
control, transport, and transfer messages. The checked-in source stays small:
FlatBuffers schemas live in `fbs/`, package inputs live in `templates/`, and a
Rust `xtask` generates and verifies every release artifact.

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

- `fbs/`: authoritative `.fbs` schemas.
- `templates/`: Rust, C, and xtask MiniJinja inputs.
- `xtask/`: FlatCC-reflection-driven generation and verification.
- `docs/`: architecture, package, development, MCAP, and use-case details.

Generated files are written only beneath `target/xtask/`:

- `target/xtask/packages/rust`
- `target/xtask/artifacts/synapse_fbs-c.tar.gz`
- `target/xtask/check` and other temporary verification trees

FlatCC generates C bindings and every BFBS reflection schema consumed by
`xtask`. Upstream `flatc` generates the Rust bindings. All generated
text owned by `xtask` is rendered from MiniJinja templates.

## Build and test

CI uses Ubuntu 24.04 as its reproducible baseline. Local builds are not tied
to a particular Ubuntu release, but require Rust and Cargo with edition 2024
support, CMake, C and C++ compilers, Git, gzip, GNU tar, and `sha256sum`. On
Ubuntu and Debian systems, install the native build tools with:

```sh
sudo apt-get update
sudo apt-get install --yes build-essential cargo cmake git gzip tar
```

Build FlatBuffers 25.12.19 and FlatCC from their pinned upstream source
revisions. Set `SYNAPSE_FBS_FLATC` and `SYNAPSE_FBS_FLATCC` to the resulting
executables, and set `SYNAPSE_FBS_FLATCC_SOURCE` to the FlatCC checkout. The
complete source-build commands are in [development and
releases](docs/development.md).

Run formatting, linting, schema validation, compatibility checks, and package
verification directly:

```sh
cargo fmt --check --manifest-path xtask/Cargo.toml
cargo clippy --locked --manifest-path xtask/Cargo.toml --all-targets -- -D warnings
cargo run --locked --manifest-path xtask/Cargo.toml -- check
cargo run --locked --manifest-path xtask/Cargo.toml -- wire-check
cargo run --locked --manifest-path xtask/Cargo.toml -- ci
```

To build release artifacts without the complete verification pass, run:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- build
```

Rust projects can set a path dependency to `target/xtask/packages/rust`. C
projects can extract the archive from `target/xtask/artifacts/` and add its root
to `CMAKE_PREFIX_PATH`. More examples are in [package usage](docs/packages.md).

## Published packages

- Rust: [`synapse_fbs`](https://crates.io/crates/synapse_fbs)
- C: a generated archive attached to [GitHub
  releases](https://github.com/CogniPilot/synapse_fbs/releases)

Each package includes the generated topic catalog and matching schema assets.
Packages that support MCAP also include the `synapse/1` reader/writer surface.
Installation, CMake targets, and local staged-package examples are in [package
usage](docs/packages.md).

## Schema changes

Edit only the authoritative files in `fbs/`, then run:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- check
cargo run --locked --manifest-path xtask/Cargo.toml -- wire-check
```

`xtask` asks FlatCC to compile BFBS and uses reflection to validate topic IDs,
the `SynapseMessage` union, command request/reply types, fixed payload sizes,
field offsets, and generated wire hashes. It does not contain a second `.fbs`
parser or documentation compiler.

Wire hashes are computed from FlatCC reflection on every build. Consumers
compare them at runtime. If a published payload changes incompatibly,
introduce a new wire type and topic.

Never hand-edit generated package trees under `target/`.

## Releases

A stable semantic version tag such as `v0.10.0` is the release version. The
workflow derives `0.10.0` from the tag for the Rust crate and C archive,
so there is no checked-in package version to bump before tagging.

Release CI verifies the same package set as the local `ci` command, then
publishes the Rust crate and attaches the C archive to the GitHub
release. See [development and releases](docs/development.md) for details.

## Documentation

- [Architecture and wire conventions](docs/architecture.md)
- [Package usage](docs/packages.md)
- [Development and releases](docs/development.md)
- [Normative MCAP profile](docs/MCAP.md)
- [Design use cases](docs/USE_CASES.md)
- [Documentation index](docs/README.md)
