# synapse_fbs

[![CI](https://github.com/CogniPilot/synapse_fbs/actions/workflows/ci.yml/badge.svg)](https://github.com/CogniPilot/synapse_fbs/actions/workflows/ci.yml)
[![Schema docs](https://img.shields.io/badge/docs-GitHub%20Pages-blue)](https://cognipilot.github.io/synapse_fbs/)
[![crates.io](https://img.shields.io/crates/v/synapse_fbs)](https://crates.io/crates/synapse_fbs)
[![PyPI](https://img.shields.io/pypi/v/synapse-fbs)](https://pypi.org/project/synapse-fbs/)
[![npm](https://img.shields.io/npm/v/@cognipilot/synapse-fbs)](https://www.npmjs.com/package/@cognipilot/synapse-fbs)

FlatBuffers schemas and generated language bindings for Synapse.

This repository is the schema source of truth for Synapse messages. It keeps
the checked-in source small and uses CI to generate the language bindings and
release artifacts from the pinned Linux toolchain in `flake.nix`.

## Documentation and Packages

- Schema docs: <https://cognipilot.github.io/synapse_fbs/>
- Main-branch schema docs: <https://cognipilot.github.io/synapse_fbs/main/>
- Latest 0.7 schema docs: <https://cognipilot.github.io/synapse_fbs/0.7/>
- Design use cases: [USE_CASES.md](USE_CASES.md)
- GitHub releases: <https://github.com/CogniPilot/synapse_fbs/releases>
- Rust crate: <https://crates.io/crates/synapse_fbs>
- Rust API docs: <https://docs.rs/synapse_fbs>
- Python package: <https://pypi.org/project/synapse-fbs/>
- JavaScript package: <https://www.npmjs.com/package/@cognipilot/synapse-fbs>

## Motivation

Synapse messages are designed for vehicles that exchange state, sensor, and
control data in real time across three transport regimes:

- **On chip:** message passing between processors over shared memory
  (Zephyr RTOS or similar).
- **Off chip:** IPC between an onboard computer and embedded flight control.
- **Over the air:** long-distance ground-control links where latency, range,
  and reliability matter.

One semantic message set serves all three. Every runtime payload is a
fixed-layout FlatBuffers `struct`, so the same bytes work as a shared-memory
ABI, a Zenoh value, a radio frame payload, and a log record with zero
re-serialization. Scaled integer fields preserve required precision without
wasting bytes: 0.1–1 mm-class position precision is the practical ceiling, so
fields are sized to that and no further, and nothing is packed below byte
alignment. Little-endian byte order is a protocol requirement.

The schemas also need to be easy to consume outside embedded firmware. The
published npm, Python, Rust, C, and C++ artifacts keep browser tools, cloud
services, developer scripts, and vehicle software on the same schema source.

## Conventions

Frame conventions are layered:

- **Raw sensor topics** carry the sensor's native conventions, documented per
  field. For example `GnssFix` course over ground and receiver yaw are
  clockwise from true north, exactly as receivers report them, so logs stay
  faithful to the hardware.
- **Estimate and command topics** follow
  [ROS REP-0103](https://www.ros.org/reps/rep-0103.html): ENU for local/world
  vectors (x east, y north, z up), FLU for body vectors (x forward, y left,
  z up), SI units, angles zero-east positive counter-clockwise. The estimator
  converts once; consumers never mix conventions within a layer.
- **Operator displays** format however pilots expect (compass headings);
  display formatting is never a wire concern.

Quaternions are Hamilton convention, component order `w x y z`, and rotate
body-frame FLU vectors into the world ENU frame. The quaternion is the only
attitude representation on the wire; Euler angles are derived by consumers.

Field validity uses one schema-defined flags bitmask per message — no sentinel
values. Core semantics (GNSS fix type, text severity, command result codes,
battery charge states, validity bits, command type masks) are FlatBuffers
enums in the schema; only genuinely vehicle-specific taxonomies (flight
modes, vehicle types) remain producer-defined.

## Zenoh

Synapse is designed to work naturally with Zenoh.

**Keys.** Keys are for humans: short, typable, and free of protocol
ceremony. Canonical key expressions are:

```text
[<namespace>/]<topic_key>[/<instance>]   # pub/sub topics, e.g. cub1/odom
[<namespace>/]cmd/<command_name>         # queryable commands/transfers
[<namespace>/]meta/...                   # reserved: schema metadata
[<namespace>/]live/...                   # reserved: liveliness tokens
```

Every topic has a curated short key (`health`, `imu`, `gnss`, `odom`,
`external_pose`, ...) recorded in the generated catalog. The topic key is the
last segment — or second-to-last when an instance segment follows — and
everything before it is the deployment namespace, which may be empty or
arbitrarily nested. `cmd`, `meta`, and `live` are reserved segments no topic
key may use, and topic keys never consist solely of digits, so keys parse
unambiguously from the tail. Keys carry no protocol or version tag because
they are convention only: the authoritative wire compatibility signal is the
mandatory Zenoh value contract described below, and consumers never infer a
payload type from the key. Multi-instance sensor topics (`imu`, `gnss`,
`power`) append an instance segment so subscribers can select one sensor
without decoding payloads.

**Multi-vehicle deployments** prepend a namespace from configuration (never
hardcoded in firmware). Namespace segments are short lowercase `snake_case`
names, and the producer owns the namespace it publishes under. Recommended
examples:

```text
health                       # bench: one vehicle, no namespace
cub1/health                  # fleet vehicle "cub1"
cub1/odom                    # compact cub1 odometry estimate without covariance
cub1/pose_raw                # unfiltered position and attitude measurement
cub1/pose                    # filtered position and attitude
cub1/pose_cov                # filtered pose with 6x6 tangent covariance
cub1/twist                   # linear and angular velocity
cub1/twist_cov               # twist with 6x6 covariance
cub1/odom_cov                # odometry state with full 12x12 covariance
cub1/imu/0                   # first IMU instance on cub1
cub2/imu/1                   # second IMU instance on cub2
cub1/cmd/mission_set         # mission upload addressed to cub1
qualisys/mocap               # raw frames from the "qualisys" mocap system
qualisys/mocap_matrix        # legacy raw frames with rigid-body rotation matrices
qualisys/cub1/pose_raw       # raw Qualisys position and attitude for cub1
qualisys/cub1/external_pose  # qualisys measurement of cub1's pose
cub1/external_pose           # bridge output in cub1's own namespace
sim/tick                     # simulator lockstep tick
field_lab/cub1/health        # nested site/vehicle namespaces
```

Infrastructure sources publish under their own namespace — a mocap system
owns `qualisys/...`, a simulator owns `sim/...` — and per-tracked-vehicle
outputs nest a vehicle sub-namespace (`qualisys/cub1/external_pose`). Bridges
write estimator inputs into the namespace of the vehicle that consumes them
(`cub1/external_pose`), so a vehicle's estimator and control stack never
subscribe outside their own namespace.

New mocap publishers use `MocapPoseFrame` on `mocap`, with rigid-body
attitudes encoded as body-FLU-to-ENU quaternions. The released matrix-based
`MocapFrame` contract remains available on `mocap_matrix` for compatibility.

A ground station subscribes to `*/health` for every vehicle at one namespace
level, or `**/health` for arbitrary nesting, and learns which vehicle a
sample came from by the key it arrived on — the namespace replaces
per-message system identifiers. The catalog helpers in every language parse
namespaced keys back into namespace, topic, and instance.

**Required value contract.** Every Synapse Zenoh value carries an encoding and
schema string. Metadata-free values are invalid. The canonical form is:

```text
<media-type>;type=<wire-type>;schema=sha256-128:<per-message-schema-hash>
```

For example:

```text
application/x-synapse-struct;type=synapse.topic.ExternalOdometryData;schema=sha256-128:<32 hex digits>
```

Fixed-layout topics use `application/x-synapse-struct`; root-table topics use
`application/x-flatbuffers`. The hash is the first 128 bits of SHA-256 and
covers only the named wire type and the
types it transitively references; unrelated schema changes do not invalidate
it. Consumers require an exact match and refuse to decode a mismatch. Live
tools throttle repeated mismatch warnings so high-rate publishers cannot flood
logs. The key describes semantic ownership and remains free to use
deployment-friendly names.

The generated Rust, Python, JavaScript, C, and JSON topic catalogs expose
`wire_type` and `schema_hash` on every topic, and request/reply types with
their schema hashes on every command. The current-contract dictionary at
`compatibility/wire-schema.toml` maps every accepted wire-type name — topic
payloads and command request/reply tables alike — to its exact hash. CI
rejects reuse of a name with a changed schema; make a new wire type and topic
for a breaking change. Unknown and retired types are immediately
incompatible. Reviewed additions and removals are applied with
`xtask update-compatibility`.

**Constrained-link profile.** Long-range and low-bandwidth links may omit the
per-value Zenoh metadata only when both endpoints are explicitly configured
with the same generated topic catalog. They compare `SCHEMA_SET_HASH` once
while establishing the link and refuse a mismatch; frames then carry the
numeric topic id and payload. The set hash covers the full catalog contract
the link then relies on — topic ids, topic keys, instance-key grammar,
encodings, wire types with their transitive schema hashes, and command ids
with their request/reply contracts — so agreement implies both endpoints
route, decode, and restore every frame identically. A receiving gateway must
restore the canonical value encoding before forwarding onto a normal Zenoh
network. Public Zenoh subscribers never accept metadata-free values or infer a
contract from a key. This keeps the strict default while avoiding repeated
schema strings on a controlled radio link.

**Pose, twist, and odometry use compact/covariance pairs.** `RawPose` carries
an unfiltered source measurement with no covariance. `Pose` and `Twist`
carry the high-rate geometry, while `PoseWithCovariance` and
`TwistWithCovariance` add 6x6 tangent covariance. `Odometry` combines a
coherent pose/twist estimate with status metadata; `OdometryWithCovariance`
adds the complete 12x12 covariance, including pose-twist cross-correlations.
The nested `synapse.types.Posef` and `Twistf` structs are deliberately
unstamped. Each top-level topic payload carries one `timestamp_us`, and nested
state in one odometry or mocap frame inherits that outer timestamp.

**Mocap has raw and estimator paths.** `MocapPoseFrame` preserves source-like
raw marker and quaternion rigid-body samples for logging and bridge
processing; the released matrix-based `MocapFrame` remains available for
compatibility. Estimators can still consume the legacy `ExternalOdometry`
input contract. Frame ids are not carried in compact per-body payloads: pose
and linear velocity are ENU, angular velocity is body FLU, and bridges
transform before publishing.

**Commands are queryables, not topics.** A GCS issues
`get("cub1/cmd/mission_get", payload)` and receives the matching reply table.
Parameter, mission, trajectory, and firmware transfer all use bounded
request/reply tables. Firmware services use the canonical `cmd/firmware_*`
keys, with optional progress published on `fw`. Streaming setpoints
(`AttitudeCommand`, `RateCommand`, `LocalPositionCommand`,
`TrajectorySegment`) remain pub/sub topics.

**Lockstep simulation uses topics.** A simulator publishes `LockstepTick` on
`tick`; each participant publishes `LockstepStatus` on `tick_status/<id>`.
Strict lockstep waits for a matching `run_id` and completed status sequence
before publishing the next tick. Use Zenoh liveliness tokens under `live/...`
for endpoint presence; the status topic is the protocol acknowledgement, not
discovery.

## Topic Catalog

The generated catalog is the source of truth for bridge and routing metadata:
`TopicId`, canonical key, root table, fixed-layout payload type and byte size,
`scope` (`vehicle` topics never leave the vehicle network; `any` topics may be
bridged subject to rate policy), `encoding`, `multi_instance`, and command
request/reply encoding metadata. It ships as `topics.json` plus language
helpers.

JavaScript:

```js
import { keyForTopic, topicById, parseKey } from '@cognipilot/synapse-fbs';

const key = keyForTopic('VehicleHealth'); // 'health'
const parsed = parseKey('cub1/imu/0');
// parsed.namespace === 'cub1', parsed.topic.name === 'InertialSample',
// parsed.instance === 0
```

Python:

```py
from synapse import topic_catalog

key = topic_catalog.key_for_topic("VehicleHealth")  # "health"
parsed = topic_catalog.parse_key("cub1/imu/0")
```

Rust:

```rust
let key = synapse_fbs::topic_catalog::key_for_topic("VehicleHealth"); // "health"
let parsed = synapse_fbs::topic_catalog::parse_key("cub1/imu/0");
```

C and C++ archives include `topics.json` and `include/synapse/topic_catalog.h`:

```c
#include <synapse/topic_catalog.h>

const char *namespace_start;
size_t namespace_len;
int32_t instance;
const synapse_topic_info_t *topic = synapse_topic_parse_key(
    "cub1/gnss", &namespace_start, &namespace_len, &instance);
```

## Serial Links

Constrained raw byte-stream links should frame bare payload structs directly:

```
[sync][len:u16][topic_id:u16][seq:u8][flags:u8][bare payload struct][crc16]
```

roughly 8 bytes of overhead per message. Two framing rules replace what the
Zenoh transport otherwise provides:

- **Retransmissions reuse the original `seq`** so receivers deduplicate
  retried frames (important for non-idempotent commands when a reply frame is
  lost).
- **Command/transfer request-reply**: frames with the request or reply flags
  bit set carry a `synapse.cmd.CmdId` value in the `topic_id` field instead
  of a `TopicId`, with `seq` correlating a reply to its request. The payload
  is the same request/reply message the Zenoh queryable would carry, so
  parameter, mission, and trajectory transfer work identically over serial.

Link-specific delimiting, integrity, authentication, or encryption belong to
the framing layer, never inside topic payloads. The FlatBuffers `Frame`
envelope in `fbs/transport.fbs` (`file_identifier "SYFR"`) remains for generic
bridges and consumers that need a self-contained FlatBuffers container.

## Telemetry Aggregate

`fbs/telemetry.fbs` defines `GcsStatus`, a 40-byte display-oriented status
aggregate (position, yaw, speeds, battery, mode, link, fix) for LoRa or
satellite-class links at 0.2–1 Hz. It is never used for control. On
SiK-class radios (~57.6 kbps) the normal topic set fits without it: a typical
downlink (attitude and global position at 4 Hz, health, power, GNSS, and
navigation at 1 Hz) is under 5 kbps in bare structs.

## Logging

MCAP is the officially supported Synapse log format. Synapse logs use
[MCAP](https://mcap.dev) as the container: schema records
carry the generated `.bfbs` reflection schemas (`flatbuffer` schema encoding),
channel topics are the canonical Zenoh keys, and messages are the
table-wrapped topic payloads so Foxglove, PlotJuggler, and the `mcap` CLI
decode them directly. Flight controllers stream index-less MCAP and files are
recovered/reindexed post-flight. Release archives include `bfbs/*.bfbs` and
`bfbs.sha256` manifests for exactly this use.

Logging is the deliberate exception to Synapse's fixed-layout priority. Live
telemetry and control samples remain fixed structs for inter-chip, shared
memory, serial-frame, and real-time paths. A log is an append-only chain of
heterogeneous MCAP records, so topic structs are wrapped in their existing
FlatBuffers root tables only after capture and outside the real-time path.
The normative [`synapse/1` MCAP profile](MCAP.md) defines the exact header,
metadata keys, timestamp basis, schema/channel mapping, message encoding,
onboard streaming behavior, and compatibility requirements. Readers must use
the `Schema.name` and BFBS embedded in each log channel rather than
substituting the reader's currently installed schemas; historical BFBS
remains authoritative.

MCAP support is built into the normal release but remains compile-time
optional. Rust hosts enable the `mcap` Cargo feature and use
`synapse_fbs::mcap`; C and Zephyr consumers link `synapse_fbs::mcap` or enable
`CONFIG_SYNAPSE_FBS_MCAP`. Applications explicitly select topics and own their
queues, buffers, logger thread, and storage sink, so disabled logging has no
runtime or linked-code cost.

## ROS and FlatROS

ROS messages are local integration types, not the Synapse wire format. The
common ROS message definitions are dynamically sized (string frame ids,
`float64[36]` covariances), which makes them bulky on constrained links and
ineligible for zero-copy loans, while CDR buffers cannot be overlaid as native
structs. Synapse remains the compact fixed-layout protocol for vehicles,
shared memory, Zenoh, logs, and serial frames.

ROS 2 integration happens at the edge through bridge nodes that translate
selected Synapse topics into ROS concepts for visualization, autonomy stacks,
simulation, and operator workflows. The planned flatros2 path generates a
`synapse_msgs` package of fixed-size ROS mirrors from these schemas, uses
`rclcpp::TypeAdapter` so nodes work on the generated structs directly, and
drives a data-driven bridge from `topics.json` plus the `.bfbs` reflection
schemas. ROS 2's Zenoh RMW prefixes its keys with the numeric domain id and
appends type-name and type-hash segments, while Synapse keys end in a catalog
topic key under non-numeric namespaces, so the two key shapes share one
router without collisions.

## Schema Design Priorities

Fixed memory layout is the default for protocol payloads. Runtime telemetry,
state, command, and control samples use FlatBuffers `struct` definitions so
adapters share predictable native layouts and avoid allocation where the
target language allows it. The fixed struct payload is the shared ABI for
chip-to-chip communication and the wire encoding for Zenoh and radio links;
serialized FlatBuffers tables remain available for transports and consumers
that need root objects.

Use FlatBuffers `table`, `string`, or vector fields only when the data is
naturally variable-size, optional, or needs FlatBuffers root/union behavior:
thin root wrappers around fixed structs, transport envelopes, text status,
and request/reply transfer messages.

Schema validation is enforced by `xtask`: every entity and field must be
documented, quantitative fields must carry a recognized unit suffix, `TopicId`
must be contiguous and mirror the `SynapseMessage` union, and payload struct
sizes are computed and checked on every build.

## Contents

- `fbs/types.fbs`: shared math structs (`synapse.types`), core protocol
  enums, and topic identifiers.
- `fbs/sensors.fbs`: GNSS, inertial, air data, and power telemetry (raw
  layer).
- `fbs/state.fbs`: vehicle health, estimates, external odometry, mission
  progress, and navigation status (estimate layer).
- `fbs/control.fbs`: manual input, setpoints, actuators, and loop
  metrics.
- `fbs/trajectory.fbs`: fixed-layout Bezier and polynomial trajectory
  segments.
- `fbs/telemetry.fbs`: compact ground-control status aggregate.
- `fbs/transport.fbs`: optional multiplexed frame and message union.
- `fbs/transfer.fbs`: parameter, mission, trajectory, and firmware queryable
  request/reply messages.
- `fbs/firmware.fbs`: maintenance-gated firmware capability, staged transfer,
  commit, abort, status, and progress messages.
- `fbs/mocap.fbs`: raw motion-capture marker and 6DOF frame data.
- `fbs/{optical_flow,sim}.fbs`: focused support schemas.
- `fbs/all.fbs`: aggregate include used by package generation.
- `topics.json` / topic catalog helpers: topic IDs, canonical keys, payload
  sizes, scopes, encodings, and command metadata in release artifacts.
- `bfbs/*.bfbs`: generated FlatBuffers reflection schemas included in C/C++
  release archives and the npm package.
- `rust/`, `python/`, `js/`, `c/`, `cpp/`: package skeletons for the published
  artifacts.
- `xtask/`: reproducible local and CI build driver.
- `flake.nix`: pinned Linux build environment and release tool versions.

Generated Rust, Python, and JavaScript package trees are intentionally not
committed. The `xtask` build stages package skeletons under
`target/xtask/packages/`, renders `.jinja` templates, and generates bindings
from `fbs/all.fbs` before building release packages.

## Version Pins

Generation is version-locked from `flake.nix`. CI builds a vendored `flatc`
from `flatbuffers-build = "=0.2.4+flatc-25.12.19"` and verifies that the
compiler reports `flatc version 25.12.19`. The Rust package depends on
`flatbuffers = "=25.12.19"` and the Python package depends on
`flatbuffers==25.12.19` so generated code and runtimes stay in lockstep. CI
also builds pinned FlatCC, uses pinned `mdbook` for schema documentation, and
publishes generated C and C++ archives for downstream CMake consumers.
Release tags must match `package.version` in `flake.nix`; the build fails
otherwise.

## Rust

Add the published crate to `Cargo.toml`:

```toml
synapse_fbs = "0.6"
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
consumers generate their own bindings from the shipped schemas or decode via
the reflection schemas. After a local `xtask` build, the staged package lives
under `target/xtask/packages/js`.

## C and C++ Archives

Release CI publishes generated C and C++ archives for downstream CMake
consumers. Prefer `find_package` for projects that download, extract, or
install the release archive as part of their dependency setup:

```cmake
find_package(synapse_fbs 0.8.0 CONFIG REQUIRED)

target_link_libraries(app PRIVATE synapse_fbs::c)
```

Point `CMAKE_PREFIX_PATH` at the extracted archive root, for example
`synapse_fbs-c/` or `synapse_fbs-cpp/`. The C archive provides
`synapse_fbs::c`, `synapse_fbs::flatcc_runtime`, and `synapse_fbs::print`; the
C++ archive provides `synapse_fbs::cpp`.

For projects that do not have a package/dependency setup, `FetchContent`
remains the simplest direct-from-release path:

```cmake
include(FetchContent)

set(SYNAPSE_FBS_VERSION 0.8.0)

FetchContent_Declare(
  synapse_fbs
  URL https://github.com/CogniPilot/synapse_fbs/releases/download/v${SYNAPSE_FBS_VERSION}/synapse_fbs-c.tar.gz
  URL_HASH SHA256=<release sha256>
  DOWNLOAD_EXTRACT_TIMESTAMP TRUE
)
FetchContent_MakeAvailable(synapse_fbs)

target_link_libraries(app PRIVATE synapse_fbs::c)
```

Use `synapse_fbs-cpp.tar.gz` and `synapse_fbs::cpp` for C++ consumers.

The C archive also carries `zephyr/module.yml`, so west manifest projects can
add it as a Zephyr module. Link `synapse_fbs::flatcc_runtime` only when using
generated builders, verifiers, or JSON helpers — reader accessors are
header-only.

## Local Build

The repository is built from a pinned Linux toolchain in `flake.nix`. The flake
targets `x86_64-linux` and `aarch64-linux`; on non-Linux hosts, use a Linux VM,
container, or WSL environment. Install Nix with flakes enabled, then run
commands through `nix develop` so Cargo, FlatBuffers, FlatCC, mdBook, Node,
Python packaging tools, and GitHub CLI all come from the same pinned
environment CI uses.

Open an interactive development shell:

```sh
nix develop
```

The examples below use the one-off `nix develop --command` form.

Fast schema validation (parse, doc-comment enforcement, unit-suffix lint,
TopicId/union consistency, payload sizes, catalog helper smoke tests):

```sh
nix develop --command cargo run --locked --manifest-path xtask/Cargo.toml -- check
```

Run the same full task that CI runs:

```sh
nix develop --command cargo run --locked --manifest-path xtask/Cargo.toml -- ci
```

The `ci` task builds pinned `flatc` and FlatCC, stages Rust/Python/JavaScript
packages under `target/xtask/packages/`, creates the C/C++ tarballs under
`target/xtask/artifacts/`, includes pinned `bfbs/*.bfbs` reflection schemas
and `bfbs.sha256` manifests in those archives, and smoke-tests the C archive
through CMake `FetchContent`.

Generate the static schema documentation locally:

```sh
nix develop --command cargo run --locked --manifest-path xtask/Cargo.toml -- docs --version 0.7 --out-dir target/xtask/docs
```

The docs are generated from `fbs/*.fbs` into an mdBook site with sidebar
navigation, search, selectable themes, and version selection. The generated
site copies the source schemas alongside the HTML and infers unit/scale notes
from field suffixes such as `_enu_`, `_flu_`, `_deg_e7`, `_mm`, `_cm_s`,
`_da`, `_cv`, `_cdeg`, `_dpermille`, and `_milli`.

## Releases

CI generates bindings and builds all packages on pull requests and branch
pushes.

Pushing a semantic version tag such as `v0.8.0` publishes a GitHub Release and
the language packages. The tag must match `package.version` in `flake.nix`; the
release build fails before publishing if they differ.

- staged `target/xtask/packages/rust/` to crates.io using trusted publishing
- staged `target/xtask/packages/python/dist/` to PyPI using trusted publishing
  (the `synapse-fbs` PyPI project must have this repository's
  `release.yml` workflow registered as a trusted publisher before tagging)
- staged `target/xtask/packages/js/` to npm using trusted publishing
- GitHub Release assets:
  - Python wheel and sdist
  - Rust `.crate` source package
  - C++ generated header tarball with matching FlatBuffers C++ runtime headers
    plus `bfbs/*.bfbs` reflection schemas
  - C generated header tarball with matching FlatCC headers, runtime sources,
    `zephyr/module.yml`, and `bfbs/*.bfbs` reflection schemas

The generated C archive is intentionally generic. Downstream firmware projects
that need it should consume a release tarball with `find_package` or fetch it
directly from their own CMake using a versioned URL and `URL_HASH SHA256=...`.

## Schema Docs

The docs workflow publishes schema documentation to the `gh-pages` branch used
by GitHub Pages. Pushes to `main` update `/main/`; release tags update the
matching minor-version docs, so `v0.8.0` updates `/0.8/`. Only the latest patch
for each published minor line is kept on GitHub Pages. Exact historical docs can
be rebuilt from the corresponding tag.

The root docs URL provides a version selector and forwards browsers to
`/main/`. The mdBook version selector links back to the published release docs:
<https://cognipilot.github.io/synapse_fbs/>.
