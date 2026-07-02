# Synapse FlatBuffers Roadmap

This roadmap records the protocol decisions made on 2026-07-01 (see REVIEW.md
for the analysis behind them) and the phased plan to land them. Synapse is
pre-adoption: backward compatibility with 0.1.x is explicitly a non-goal, so
all wire/schema decisions land at once in 0.2.0.

## Decision record

| # | Decision | Choice |
|---|---|---|
| D1 | Frame conventions | Layered: raw sensor topics carry sensor-native conventions (GNSS COG/yaw clockwise-from-north); estimates and commands are strictly REP-103 ENU/FLU; displays format freely. Never mixed within a layer. |
| D2 | Quaternion | Hamilton, `w x y z`; rotates FLU body vectors into ENU world: `v_enu = q ⊗ v_flu ⊗ q⁻¹`. Quaternion is the only attitude representation on the wire (Euler deleted). |
| D3 | Canonical encoding | Bare payload struct bytes for fixed-layout topics (identical bytes in shared memory, Zenoh values, radio frames, log messages). Thin root tables remain for variable-size topics, `Frame`, and MCAP logging. Little-endian is a protocol requirement. |
| D4 | Zenoh keys | `[<namespace>/]synapse/v1/topic/<topic_name>[/<instance>]`. Version segment is the evolution mechanism; instance segment for multi-instance sensors; vehicle/swarm/site namespace is a deployment prefix (never hardcoded in firmware). `synapse/v1/cmd/…`, `synapse/v1/meta/…`, `synapse/v1/live/…` reserved. |
| D5 | Semantics | Schema owns core enums (fix type, severity, command results, charge state, field-validity bits, type masks). Vehicle-specific taxonomies (flight modes, vehicle types) stay producer-defined pending discovery records. |
| D6 | Commands | Zenoh queryables on `synapse/v1/cmd/<name>`; reply payload is `CommandResultData`. Streaming setpoints stay pub/sub. `confirmation` retransmission counters are removed (transport owns retry/correlation). |
| D7 | Validity | One schema-defined flags bitmask per message. No sentinels; zero-means-absent only where zero is physically impossible. |
| D8 | Logging | MCAP container: schema = `.bfbs` (`flatbuffer` encoding), channel topic = canonical Zenoh key, message = table-wrapped payload. `log.fbs` retired. FCs stream index-less MCAP; post-flight recover/reindex. |
| D9 | Precision policy | 0.1–1 mm-class position precision is the ceiling; byte-aligned scaled integers; no sub-byte packing. float32 for local/body quantities, scaled ints for global coordinates and compact telemetry. |
| D10 | Right-sizing | GNSS accuracies → ushort mm (saturating); battery current → deci-amps; pack voltage → centi-volts; 16 monitored cells; vertical velocity + GNSS time + used/visible satellites added to GnssFix. |
| D11 | Scope hygiene | MotorOutput dropped (ActuatorFeedback covers it); math types move to `synapse.types`; optical-flow and mocap topics join TopicId/union/catalog; `Frame` gets `file_identifier "SYFR"`. |

## Phase 1 — Protocol 0.2.0 (this repo, landing now)

1. **Schema rework** (`fbs/`)
   - `types.fbs`: `synapse.types` namespace for math types; quaternion
     convention documented; `AttitudeEuler` deleted; core enums
     (`GnssFixType`, `Severity`, `CommandResultCode`, `BatteryChargeState`,
     `BatteryFunction`, `MissionState`, `GeoAltitudeFrame`, bit constants for
     validity masks); `TopicId` renumbered contiguously with
     OpticalFlow/OpticalFlowVelocity/MocapFrame/MocapDefinition/GcsStatus
     added and MotorOutput removed.
   - `sensors.fbs`: InertialSample loses derived pressure fields
     (raw-layer purity); GnssFix gains `time_unix_us`, `velocity_up_cm_s`,
     `satellites_used`, right-sized accuracies, receiver-native COG/yaw,
     schema-defined flags; 16-cell battery block; deci-amp current.
   - `state.fbs`: VehicleHealth (centi-volt pack voltage, deci-amp current,
     error-counter fossils dropped, armed/failsafe → flags); AttitudeEstimate
     quaternion-only; LocalPositionEstimate flags + reset counters;
     GlobalPositionEstimate schema-defined flags; NavigationTarget one angle
     convention (cdeg).
   - `control.fbs`: ManualControl (widened axes mask, sentinels removed,
     throttle semantics defined, switch bools → flags); Attitude/RateCommand
     scalar thrust + schema-defined masks; VehicleCommand/GeoCommand lose
     `confirmation`, gain enum types; CommandResult uses `CommandResultCode`;
     MotorOutput removed.
   - `optical_flow.fbs`: `pixel_flow` → `flow_rad` (integrated radians),
     quality 0–100.
   - `transport.fbs`: union matches new TopicId set; `file_identifier
     "SYFR"`; encoding-profile comments.
   - `telemetry.fbs` (new): `GcsStatusData` ~36 B low-rate aggregate for
     LoRa/satcom-class links.
   - `transfer.fbs` (new): mission/parameter queryable request/reply
     messages (`ParamValue`, `ParamGetRequest/Reply`, `ParamSetRequest/Reply`,
     `MissionItem`, `MissionGetRequest/Reply`, `MissionSetRequest/Reply`).
   - `log.fbs` deleted (D8); `sil.fbs` timestamp normalized to µs.
2. **Keys, catalog, tooling** (`xtask/`)
   - Key prefixes `synapse/v1/{topic,cmd,meta,live}`; per-topic
     `multi_instance` (payload carries an `id`) with instance key segments.
   - Catalog metadata: `payload_size` (computed from struct layout), `scope`
     (`onchip` / `vehicle` / `any`), `encoding` (`struct` / `table`),
     `multi_instance`; commands listed with request/reply types.
   - Namespace-aware `parse_key()` helpers in Rust/Python/JS/C catalogs
     (anchor on `synapse/v1`, return namespace + topic + instance).
   - `xtask check` fast command: schema parse, doc-comment validation,
     unit-suffix lint, TopicId↔union consistency, struct size report.
   - Release guard: git tag must equal `PACKAGE_VERSION`.
3. **Packaging/CI**
   - PyPI trusted publishing (OIDC) replaces the API token.
   - C++ archive gains `synapse_fbs::cpp` CMake target.
   - C archive gains `zephyr/module.yml` for west manifests.
   - `PACKAGE_VERSION` → 0.2.0.
4. **Docs**
   - README rewritten around the decision record (encoding profiles,
     multi-vehicle keys, queryable commands, MCAP logging, conventions
     layering, serial framing sketch).

## Phase 2 — Protocol services (this repo, after 0.2.0)

- Compact serial framing spec hardening: `[sync][len:u16][topic_id:u16]
  [seq:u8][flags:u8][bare struct][crc16]` — documented in Phase 1 README,
  promoted to a conformance doc with test vectors here.
- Discovery/definition records for producer-defined taxonomies
  (FlightModeDefinition etc., the MocapDefinition pattern generalized).
- Late-joiner conventions: liveliness tokens on `synapse/v1/live/<ns>`,
  last-value queryables for HomeReference/TimeReference/MissionProgress.
- `.bfbs`-reflection-driven catalog/docs generation replacing the textual
  schema parser; auto-generated size/offset tables in docs; frozen-ABI size
  regression check in CI.
- Static-assert ABI header generation for C consumers (offset/sizeof checks
  for shared-memory use).

## Phase 3 — Ecosystem (companion repos)

- **flatros2**: generated `synapse_msgs` ROS 2 mirrors emitted from these
  schemas; `rclcpp::TypeAdapter` adapters; data-driven Zenoh↔ROS bridge from
  `topics.json` + `.bfbs`; convenience conversions to `nav_msgs`/
  `sensor_msgs` for RViz/Foxglove.
- **MCAP writers**: Zephyr streaming writer (index-less) + companion/GCS
  recover-and-index tooling; Foxglove live decode via `.bfbs`.
- **GCS profile**: rate-policy gateway driven by catalog `scope`/rate
  metadata; GcsStatus-only fallback profile for sub-10 kbps links.
- Mission/param service reference implementation over queryables.
