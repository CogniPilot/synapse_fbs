# Architecture and wire conventions

Synapse is a compact vehicle protocol shared by firmware, onboard computers,
ground systems, browsers, and offline tools. One message set covers three
transport regimes:

- **On chip:** processors exchange fixed-layout data over shared memory.
- **Off chip:** between an onboard computer and embedded flight control.
- **Over the air:** constrained links trade bandwidth, latency, and range.

## Payload model

Runtime telemetry, state, and control payloads are normally FlatBuffers
`struct` definitions. They have fixed size and alignment, need no allocation,
and can be copied as little-endian bytes between supported targets. Nothing is
packed below byte alignment.

Tables, strings, vectors, and unions are reserved for naturally variable or
optional data, thin root wrappers, request/reply transfers, and generic
transport envelopes. Logging wraps captured fixed structs in their existing
root tables outside the real-time path.

Scaled integers are used where a floating-point field would spend bits without
adding useful sensor precision. Field names and schema comments define the
unit and scale. Consumers must not infer validity from sentinel values; each
message uses schema-defined status or validity fields.

## Frames and coordinates

Raw sensor topics preserve the device convention so recorded data stays
faithful to its source. Estimates and commands use ROS REP-0103 conventions:

- world/local vectors are ENU: x east, y north, z up;
- body vectors are FLU: x forward, y left, z up;
- units are SI unless the field name states a scale;
- angles are zero-east and positive counter-clockwise.

Quaternions use Hamilton convention in `w x y z` component order. They rotate
body-frame FLU vectors into world-frame ENU vectors. Euler angles are derived
by consumers and are not an attitude wire representation.

## Zenoh keys

Canonical key expressions are:

```text
[<namespace>/]<topic_key>[/<instance>]
[<namespace>/]cmd/<command_name>
[<namespace>/]meta/...
[<namespace>/]live/...
```

Topic keys are curated short names such as `health`, `imu`, `gnss`, `odom`,
and `external_pose`. The namespace is deployment configuration and may be
empty or nested. Multi-instance topics append an integer instance.

Examples:

```text
health
cub1/health
cub1/odom
cub1/imu/0
cub2/gnss/1
cub1/cmd/mission_set
qualisys/mocap
qualisys/cub1/external_pose
sim/tick
field_lab/cub1/health
```

Infrastructure publishes under its own namespace. A bridge writes estimator
inputs into the namespace of the vehicle consuming them. Liveliness tokens use
`live/...`; protocol acknowledgements remain ordinary typed messages.

Keys express semantic ownership, not binary compatibility. Consumers never
infer a wire type from a key.

## Required value contract

Every normal Synapse Zenoh value carries an encoding and schema string:

```text
<media-type>;type=<wire-type>;schema=sha256-128:<schema-hash>
```

Fixed structs use `application/x-synapse-struct`; root tables use
`application/x-flatbuffers`. The schema hash is the first 128 bits of SHA-256
over the named wire type and every transitively referenced type. Unrelated
schema edits do not change it.

Generated catalogs expose the encoding, wire type, schema hash, fixed payload
size, scope, canonical key, instance grammar, and command request/reply
contracts. Consumers require an exact match before decoding.

Hashes are derived from the committed schemas through FlatCC reflection on
every build. They are generated data rather than committed source. A published
wire name should not be reused for a different contract.

## Constrained links

Explicitly configured low-bandwidth endpoints may omit repeated Zenoh metadata
after comparing the generated `SCHEMA_SET_HASH`. That hash covers topic IDs,
keys, instance rules, encodings, wire hashes, command IDs, and request/reply
contracts. A mismatch prevents the link from opening.

A minimal byte-stream frame is:

```text
[sync][len:u16][topic_id:u16][seq:u8][flags:u8][bare payload][crc16]
```

Retransmissions retain the original sequence number. Request and reply frames
place a generated command ID in the topic-ID slot and use the sequence number
for correlation. Delimiting, integrity, authentication, and encryption belong
to the link layer rather than topic payloads.

A gateway restoring constrained frames onto a normal Zenoh network must also
restore the canonical value metadata.

## State and motion capture

`RawPose` carries an unfiltered source measurement. `Pose` and `Twist` carry
compact high-rate geometry. Their covariance variants add 6x6 tangent-space
covariance, while `OdometryWithCovariance` carries the complete 12x12 joint
covariance including pose/twist cross terms.

Nested pose and twist structs are unstamped. The enclosing topic supplies the
timestamp. New motion-capture sources publish quaternion-based
`MocapPoseFrame`; the released matrix-based `MocapFrame` remains available for
compatibility.

## Commands and simulation

Bounded parameter, mission, trajectory, and firmware transfers use Zenoh
queryables under `cmd/...`. Streaming setpoints remain pub/sub topics.

Lockstep simulation uses `LockstepTick` and per-participant `LockstepStatus`
topics. Strict mode waits for matching run and sequence completion before the
next tick. Liveliness is discovery, not the lockstep acknowledgement.

## Logging

MCAP is the supported log container. Schema records carry generated BFBS,
channel topics use canonical Zenoh keys, and messages contain root-table-wrapped
payloads. Historical readers use the BFBS embedded in the log rather than
substituting currently installed schemas.

The exact header, metadata, timestamp, channel, recovery, and compatibility
rules are normative in the [`synapse/1` MCAP profile](MCAP.md).

## ROS integration

ROS messages are edge integration types, not the Synapse wire format. Bridge
nodes translate selected topics for visualization, autonomy, and simulation.
Synapse stays compact and fixed-layout across vehicle networks, shared memory,
radio links, and logs.

The generated topic catalog and BFBS reflection schemas make those bridges
data-driven without embedding another `.fbs` compiler.
