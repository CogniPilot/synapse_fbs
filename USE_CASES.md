# Synapse Use Cases

This file records the use cases that should drive Synapse schema and transport
decisions. A schema change should either support these use cases directly or
explain why a use case is out of scope.

## UC-001: GPS Mission Transfer Over Low-Bandwidth Serial

An operator wants to upload and download a GPS waypoint mission over a
low-bandwidth serial radio link without requiring Zenoh on the link.

Constraints:

- The link may be 57.6 kbps or lower.
- Frames may be lost, duplicated, or retried.
- A mission may be larger than one serial frame.
- The receiver may already have a different mission revision.
- The operator needs to know which item failed if validation fails.

Schema and protocol requirements:

- Mission get/set must support chunks or pages with offset, count, total, and
  an end-of-transfer marker.
- Mission set must include a plan id, revision, or hash so stale writers do
  not silently overwrite a newer mission.
- Replies must include enough error detail to identify the failed item and
  reason class.
- Serial framing must define request/reply ids, retry behavior, duplicate
  handling, fragmentation or paging, CRC details, and versioning.
- The same mission transfer semantics should work over Zenoh queryables and
  over serial request/reply frames.

Success criteria:

- A large mission can be transferred in bounded chunks.
- A lost or duplicated chunk does not corrupt the mission.
- A receiver can reject a stale or invalid mission with actionable detail.
- A bridge can route and decode the transfer from generated catalog metadata.

## UC-002: Parameter Sync Over Low-Bandwidth Serial

An operator wants to inspect and change parameters over a constrained serial
link.

Constraints:

- A vehicle may expose hundreds of parameters.
- Parameter names and text values are variable length.
- A full parameter dump may not fit in one frame.
- Parameter set failures must be diagnosable.

Schema and protocol requirements:

- Parameter get-all must be paged or chunked.
- Replies must report offset, count, total, and completion state.
- Set replies must identify invalid names, unsupported types, rejected values,
  and values actually in effect after the request.
- Command catalog metadata must describe request/reply encoding so serial and
  Zenoh bridges do not hardcode each service.

Success criteria:

- A complete parameter list can be synchronized without an unbounded reply.
- A failed set operation reports enough detail for a ground station to show a
  useful error.

## UC-003: Bezier Trajectory Transfer For High-Speed Quadrotor Flight

An autonomy stack or planner wants to send a time-parameterized Bezier or
polynomial trajectory to a quadrotor controller for differential-flatness-based
high-speed flight, instead of sending a waypoint mission.

Constraints:

- The trajectory may contain multiple polynomial/Bezier segments.
- Each segment has timing and continuity requirements across position,
  velocity, acceleration, and possibly yaw or heading.
- Controllers may consume only a sliding horizon, while planners may replace
  or revise the trajectory at high rate.
- Segment payloads must be bounded for embedded control paths.
- Large plans may still need chunked transfer over constrained links.

Schema and protocol requirements:

- Trajectory messages must be distinct from waypoint mission messages.
- The schema must identify trajectory type, coordinate frame, start time or
  time base, segment duration, segment order, and revision/trajectory id.
- Segment representation must be fixed-layout when used in high-rate control
  paths.
- Chunked transfer must support bounded batches of segments with offset,
  count, total, and revision/hash when sending full plans.
- Streaming trajectory updates over Zenoh should support latest-horizon
  behavior without reusing mission upload semantics.
- The controller must be able to reject unsupported polynomial degree,
  coordinate frame, continuity class, or stale revision with actionable detail.

Success criteria:

- A high-speed quadrotor controller can consume a bounded fixed-layout
  trajectory segment or horizon without allocation in the control loop.
- A planner can replace or extend a trajectory using explicit revision and
  timing semantics.
- Waypoint missions and polynomial trajectories do not share a confusing
  MAVLink-style command-item payload.
- The same trajectory family can support local Zenoh streaming and chunked
  low-bandwidth transfer when needed.

## UC-004: Deterministic Native/QEMU/Zephyr Lockstep Simulation

A simulator drives a controller or Zephyr target in deterministic lockstep.
The simulator publishes time steps, the participant runs until the target boot
time, and the simulator waits for completion before sending the next step.

Constraints:

- The simulator and target may restart independently.
- Stale status messages may be observed after a restart or replay.
- There may be multiple participants or simulation domains.
- The payload should remain fixed-layout for shared memory and fast paths.

Schema and protocol requirements:

- Lockstep tick and status messages must include a run or epoch id in addition
  to sequence.
- Status must distinguish ready, running, completed, reset acknowledgement,
  missed tick, and error states.
- Error status must carry compact detail.
- Topic scope must allow host-driven SIL/HIL/QEMU setups without accidental
  bridge filtering.
- Completion acknowledgement is a protocol status topic; liveliness only
  reports endpoint presence.

Success criteria:

- A stale completion from a previous run cannot satisfy a new tick.
- Multiple participants can report independent completion.
- The fixed-layout payload size is known in the generated topic catalog.

## UC-005: High-Rate Estimation And Control Between Chips

Flight-control components exchange high-rate state, sensor, and actuator data
between processors or threads using shared memory or a zero-copy ring buffer.

Constraints:

- Runtime payloads must be bounded and fixed-layout.
- Consumers may overlay generated structs on aligned memory.
- Allocation and table traversal are not acceptable in the fast path.

Schema and protocol requirements:

- Runtime topics should use FlatBuffers `struct` payloads.
- Generated catalog entries must include payload size and encoding.
- Shared-memory transports must guarantee payload alignment, or receivers must
  copy into aligned storage before overlaying structs.
- Variable-size topics must be clearly marked and easy to filter out.

Success criteria:

- Core telemetry, estimate, command, actuator, and lockstep topics can be
  routed by id and payload size.
- Table-encoded topics do not accidentally enter fixed-overlay paths.

## UC-006: Raw Motion Capture And Derived External Odometry

A Qualisys bridge receives raw motion-capture data, logs source-like marker and
6DOF body samples, and publishes a derived odometry-style estimator input for
indoor flight at 240 Hz.

Constraints:

- Raw mocap data may include labeled 3D markers, unlabeled 3D markers,
  six-degree-of-freedom rigid bodies, residuals, frame numbers, and quality
  counters.
- Raw mocap logging/tooling may be variable-size.
- The estimator path must be bounded and fixed-layout.
- Motion-capture systems may provide pose only; bridge-side filtering may
  compute linear and angular velocity.
- Multiple tracked rigid bodies or mocap sources may exist in the same lab.

Schema and protocol requirements:

- Raw mocap frames should preserve source-like marker and 6DOF body data
  without per-frame strings.
- The estimator input must be a fixed-layout external odometry topic, not a
  variable-size capture-system frame table.
- The odometry payload must include timestamp, source id, rigid-body id,
  position, attitude, linear velocity, angular velocity, validity flags, and
  compact producer status. It must not carry default uncertainty fields in the
  high-rate control path.
- Derived mocap state estimation uses a 12D tangent state, not a default
  acceleration-augmented state: attitude perturbation, velocity, position, and
  body angular velocity. Uncertainty is optional and uses a separate covariance
  topic so high-rate consumers can ignore it.
- Frame identifiers are not carried in high-rate payloads: pose and linear
  velocity are ENU, angular velocity is body FLU, and bridges transform before
  publishing.
- The odometry topic must be multi-instance so consumers can subscribe to one
  tracked body without decoding all mocap traffic.
- The catalog must expose payload size and fixed encoding so DMA/shared-memory
  transports can safely route it.

Success criteria:

- A vehicle can consume 240 Hz external odometry samples without allocation or
  table traversal in the estimator path.
- The same odometry payload can be routed over Zenoh, framed serial, or direct
  memory transfer.
- Raw mocap can be logged and replayed without forcing estimators to consume a
  variable-size capture-system frame.
- Skeletons, analog data, force plates, gaze vectors, and capture-system
  metadata are not stabilized unless a separate use case justifies them.

## UC-007: Zenoh Multi-Vehicle Telemetry And Command

A ground station communicates with one or more vehicles over Zenoh.

Constraints:

- Vehicle identity comes from the Zenoh key namespace.
- Streams use pub/sub; commands and transfers use queryables.
- Some command services may have multiple providers if component routing is
  supported.

Schema and protocol requirements:

- Topic keys and command keys must be canonical and parseable by generated
  helpers.
- Commands should not also be executable as pub/sub topics.
- Command retry and idempotency semantics must be explicit; Zenoh correlation
  and timeout are not a complete retry protocol.
- Liveliness token key shapes must be documented for vehicles, bridges,
  simulators, command providers, and lockstep participants.
- Catalog metadata should expose routing class, encoding, and recommended QoS
  class when that policy is standardized.

Success criteria:

- A GCS can discover vehicle streams and command services by key conventions.
- A bridge can route command requests without MAVLink-style target ids in the
  payload.
- Commands are not accidentally executed through a topic subscription path.

## UC-008: Variable-Size Metadata And Logging

Tools consume text status, metadata, and logs.

Constraints:

- These messages are naturally variable-size.
- They are useful over Zenoh and in MCAP logs.
- They are not suitable for fixed shared-memory overlay.

Schema and protocol requirements:

- Variable-size topics must be table-encoded and cataloged as such.
- Fixed-layout transports must be able to filter them by catalog metadata.
- Logging should use root tables and reflection schemas for tool compatibility.

Success criteria:

- Tools can decode table topics correctly.
- Embedded fast paths can reject or route around table topics without
  hardcoded topic names.

## UC-009: Constrained Radio Telemetry

A vehicle sends low-rate operator telemetry over a constrained radio or
satellite link.

Constraints:

- Bandwidth may be very low and latency may be high.
- Control should not depend on aggregate display messages.
- Operators still need position, battery, mode, link, and mission progress.

Schema and protocol requirements:

- Compact aggregate telemetry, such as `GcsStatus`, should remain bounded and
  fixed-layout.
- Field precision should match display and supervision needs.
- The aggregate must be clearly documented as display-oriented and not a
  control input.

Success criteria:

- A useful operator status packet fits at low update rates.
- Higher-fidelity topics remain available when bandwidth allows.

## UC-010: Bridge And Tool Generation

A bridge or tooling package consumes only release artifacts and generated
catalogs to route and decode Synapse traffic.

Constraints:

- The bridge should not hardcode topic names, command ids, payload sizes, or
  encodings.
- It may bridge between Zenoh, serial, logs, and in-process APIs.

Schema and protocol requirements:

- Topic catalog entries must include id, key, payload type, payload size,
  schema file, encoding, scope, and multi-instance behavior.
- Command catalog entries must include id, key, request/reply types,
  request/reply encoding, and fixed sizes when applicable.
- Helpers must support lookup by id, name, key, and parsed namespaced keys for
  both topics and commands.

Success criteria:

- A bridge can dispatch every stable topic and command using generated
  metadata alone.
- Invalid keys are rejected consistently across Rust, Python, JavaScript, and C
  helpers.

## UC-011: Maintenance-Gated Firmware Update

A ground station stages and activates a firmware image without exposing image
writes through flight-control topics.

Constraints:

- Updates are permitted only while producer-defined maintenance gates hold.
- Images are larger than a practical command payload and must be chunked.
- Chunks may be retried, duplicated, or arrive after an interrupted transfer.
- The receiver must verify target compatibility and image integrity before
  activation.

Schema and protocol requirements:

- Capability, status, prepare, chunk, commit, and abort operations are bounded
  queryable commands under canonical `cmd/firmware_*` keys.
- An update id and chunk index make identical chunk retries idempotent; a
  conflicting retry must be rejected.
- Prepare and commit carry full-image integrity metadata, while each chunk
  carries its own integrity value.
- Replies provide a command result, state, diagnostic detail, and progress.
- `FirmwareProgress` is an optional table topic for UI and logging; command
  replies and status queries remain authoritative.
- Controller tuning uses `ParamSetRequest`; it does not require a
  firmware- or gain-specific parameter schema.

Success criteria:

- A receiver can safely resume, reject, abort, or complete a staged update.
- Generated catalogs expose every firmware command and the optional progress
  topic without hand-maintained routing names.
- Firmware transfer and ordinary control traffic remain separate protocol
  surfaces.
