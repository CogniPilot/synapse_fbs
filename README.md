# synapse_fbs

FlatBuffers schemas and generated language bindings for Synapse.

This repository is the schema source of truth for Synapse messages. It keeps the
checked-in source small and uses CI to generate the language bindings and release
artifacts from the pinned toolchain in `tools.lock`.

Published Synapse FlatBuffers documentation: <https://cognipilot.github.io/synapse_fbs/>

Synapse schemas use [ROS REP-0103](https://www.ros.org/reps/rep-0103.html)
conventions by default: SI units where practical, ENU for local/world vectors,
and FLU for body-frame vectors. Compact integer fields use explicit unit
suffixes when a scaled representation is chosen for precision or wire
efficiency.

## Motivation

Synapse messages are designed for vehicles that exchange state, sensor, and
control data in real time. Fixed-layout payloads support shared-memory message
passing between chips, while scaled integer fields preserve required precision
without wasting bytes on high-rate or over-the-air links where latency, range,
and reliability matter.

The schemas also need to be easy to consume outside embedded firmware. The
published npm, Python, Rust, C, and C++ artifacts keep browser tools, cloud
services, developer scripts, and vehicle software on the same schema source.

## Zenoh

Synapse is designed to work naturally with Zenoh. The normal pattern is to
publish each topic's FlatBuffers root table directly on a stable Zenoh key
expression and let the key expression identify the stream. That keeps messages
small, avoids redundant envelope fields, and lets subscribers use native Zenoh
selectors.

Release artifacts include a generated topic catalog with canonical key
expressions under `synapse/topic/<topic_name>`, plus helper functions for
looking up a topic by `TopicId`, root table name, or key. Deployments can prefix
those keys with a vehicle, swarm, or site namespace without changing the
message payload.

The optional `Frame` envelope is mainly for serial or other raw byte-stream
links that need an explicit Synapse container. Zenoh deployments should usually
publish typed topic values directly and reserve `TopicId` for bridges, logs,
serial frames, or compact routing tables.

The release artifacts make this practical across platforms: Rust, Python, C,
and C++ packages provide generated bindings, while the npm package ships schema
assets and reflection schemas for browser tools, dashboards, and gateways.

## Topic Catalog

The generated catalog is the source of truth for bridge and routing metadata:
`TopicId`, canonical Zenoh key, FlatBuffers root table, fixed-layout payload
struct, schema file, and a short description.

JavaScript:

```js
import { keyForTopic, topicById } from '@cognipilot/synapse-fbs';

const key = keyForTopic('VehicleHealth');
const payloadType = topicById(1)?.payloadType;
```

Python:

```py
from synapse import topic_catalog

key = topic_catalog.key_for_topic("VehicleHealth")
payload_type = topic_catalog.topic_by_id(1).payload_type
```

Rust:

```rust
let key = synapse_fbs::topic_catalog::key_for_topic("VehicleHealth");
let topic = synapse_fbs::topic_catalog::topic_by_id(1);
```

C and C++ archives include `topics.json` and `include/synapse/topic_catalog.h`:

```c
#include <synapse/topic_catalog.h>

const synapse_topic_info_t *topic = synapse_topic_by_key("vehicle_health");
```

## ROS And FlatROS

ROS messages are local integration types, not the Synapse over-the-air format.
They are intentionally more general and can be too bulky for constrained radio
links. Synapse should remain the compact FlatBuffers protocol for vehicles,
shared memory, Zenoh, logs, and serial frames.

ROS 2 integration should happen at the edge through bridge nodes that translate
selected Synapse topics into ROS concepts for visualization, autonomy stacks,
simulation, rosbag tooling, and operator workflows. The generated topic catalog
gives those bridges stable keys and type metadata without hand-maintained lookup
tables.

The planned flatros2 path is to provide a ROS workspace/release archive that
depends on `flatros2`, consumes the Synapse FlatBuffers schemas and catalog, and
offers adapter nodes for selected topics. That archive should preserve Synapse
payloads as FlatBuffers at the protocol boundary and only translate into ROS
message views where ROS tooling needs them.

## Schema Design Priorities

Fixed memory layout is the default for protocol payloads. Runtime telemetry,
state, command, and control samples should use FlatBuffers `struct` definitions
so adapters can share predictable native layouts and avoid allocation where the
target language/runtime allows it.

This is especially important for chip-to-chip communication over shared memory:
the fixed struct payload should be usable as the shared ABI, while serialized
FlatBuffers tables remain available for transports and languages that need root
objects.

Use FlatBuffers `table`, `string`, or vector fields only when the data is
naturally variable-size, optional, or needs FlatBuffers root/union behavior.
Common exceptions are thin root wrappers around fixed structs, transport
envelopes, log records, text status, schema metadata, and definition records
that consumers cache instead of processing in the control loop.

## Contents

- `fbs/types.fbs`: shared fixed structs, topic IDs, units, and frame conventions.
- `fbs/sensors.fbs`: GNSS, inertial, air data, and power telemetry.
- `fbs/state.fbs`: vehicle health, estimates, mission progress, and navigation status.
- `fbs/control.fbs`: manual input, setpoints, commands, actuators, and loop metrics.
- `fbs/transport.fbs`: optional multiplexed frame and message union.
- `fbs/{mocap,optical_flow,log,sil}.fbs`: focused support schemas.
- `fbs/all.fbs`: aggregate include used by package generation.
- `topics.json` / topic catalog helpers: generated topic IDs, canonical keys,
  root tables, and fixed-layout payload metadata in release artifacts.
- `bfbs/*.bfbs`: generated FlatBuffers reflection schemas included in C/C++
  release archives.
- `rust/`: Rust package skeleton, published as the `synapse_fbs` crate.
- `python/`: Python package skeleton, published as the `synapse-fbs` package.
- `js/`: JavaScript/TypeScript schema-assets package skeleton, published as the
  `@cognipilot/synapse-fbs` npm package.
- `c/`: C release archive skeleton, published as `synapse_fbs-c.tar.gz`.
- `cpp/`: C++ release archive skeleton, published as `synapse_fbs-cpp.tar.gz`.
- `xtask/`: reproducible local and CI build driver.
- `tools.lock`: pinned package, generator, and runtime versions.

Generated Rust, Python, and JavaScript package trees are intentionally not
committed. The `xtask` build stages package skeletons under
`target/xtask/packages/`, renders `.jinja` templates, and generates bindings
from `fbs/all.fbs` before building release packages.

## Version Pins

Generation is version-locked from `tools.lock`. CI builds a vendored `flatc`
from `flatbuffers-build = "=0.2.4+flatc-25.12.19"` and verifies that the
compiler reports `flatc version 25.12.19`. The Rust package depends on
`flatbuffers = "=25.12.19"` and the Python package depends on
`flatbuffers==25.12.19` so generated code and runtimes stay in lockstep. CI
also builds pinned FlatCC, uses pinned `mdbook` for schema documentation, and
publishes generated C and C++ archives for downstream CMake consumers.

## Rust

Add the published crate to `Cargo.toml`:

```toml
synapse_fbs = "0.1.8"
```

After a local `xtask` build, use the staged crate directly:

```toml
synapse_fbs = { path = "../synapse_fbs/target/xtask/packages/rust" }
```

## Python

Install the published package:

```sh
pip install synapse-fbs
```

After a local `xtask` build, install the staged wheel:

```sh
pip install target/xtask/packages/python/dist/*.whl
```

## JavaScript / TypeScript

Install the published npm package:

```sh
npm install @cognipilot/synapse-fbs
```

Unlike the Rust and Python packages, the npm package ships schema assets
(`fbs/*.fbs` plus generated `bfbs/*.bfbs` reflection schemas) rather than
generated bindings, and has no `flatbuffers` runtime dependency. The npm
`flatbuffers` release cadence does not track the pinned `flatc` version, so JS
consumers generate their own bindings from the shipped schemas or decode via the
reflection schemas. After a local `xtask` build, the staged package lives under
`target/xtask/packages/js`.

## C and C++ Archives

Release CI publishes generated C and C++ archives for downstream CMake
consumers. Firmware projects should fetch the release archive directly where
they need it instead of vendoring generated files:

```cmake
include(FetchContent)

FetchContent_Declare(
  synapse_fbs
  URL https://github.com/CogniPilot/synapse_fbs/releases/download/v0.1.8/synapse_fbs-c.tar.gz
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

## Local Build

Run the same Rust task that CI runs:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- ci
```

The task builds pinned `flatc` and FlatCC, stages Rust/Python/JavaScript packages under
`target/xtask/packages/`, creates the C/C++ tarballs under
`target/xtask/artifacts/`, includes pinned `bfbs/*.bfbs` reflection schemas and
`bfbs.sha256` manifests in those archives, and smoke-tests the C archive through
CMake `FetchContent`.

Generate the static schema documentation locally:

```sh
cargo install mdbook --version "$(awk -F= '/^MDBOOK_VERSION=/{print $2}' tools.lock)" --locked
cargo run --locked --manifest-path xtask/Cargo.toml -- docs --version 0.1.8 --out-dir target/xtask/docs
```

The docs are generated from `fbs/*.fbs` into an mdBook site with sidebar
navigation, search, selectable themes, and version selection. The generated
site copies the source schemas alongside the HTML and infers unit/scale notes
from field suffixes such as `_enu_`, `_flu_`, `_deg_e7`, `_mm`, `_cm_s`, `_ca`,
`_cdeg`, `_dpermille`, and `_milli`.

## Releases

CI generates bindings and builds all packages on pull requests and branch
pushes.

Pushing a tag like `v0.1.8` publishes:

- staged `target/xtask/packages/rust/` to crates.io using `CARGO_REGISTRY_TOKEN`
- staged `target/xtask/packages/python/dist/` to PyPI using trusted publishing
- staged `target/xtask/packages/js/` to npm using `NPM_TOKEN`
- GitHub Release assets:
  - Python wheel and sdist
  - Rust `.crate` source package
  - C++ generated header tarball with matching FlatBuffers C++ runtime headers
    plus `bfbs/*.bfbs` reflection schemas
  - C generated header tarball with matching FlatCC headers, runtime sources,
    and `bfbs/*.bfbs` reflection schemas

The generated C archive is intentionally generic. Downstream firmware projects
that need it should fetch the release tarball directly from their own CMake
using a versioned URL and `URL_HASH SHA256=...`.

## Schema Docs

The docs workflow publishes versioned schema documentation to the `gh-pages`
branch. Pushes to `main` update `/main/`; release tags like `v0.1.8` update
`/0.1.8/`. The root docs URL redirects to `/main/`, and the mdBook version
selector links to published release docs:
<https://cognipilot.github.io/synapse_fbs/>.
