# Synapse FlatBuffers — In-Depth Design Review

> **Status:** the decisions arising from this review were made and landed in
> the 0.2.0 rework; see [ROADMAP.md](ROADMAP.md) for the decision record.
> This document is kept as the analysis behind them.

**Date:** 2026-07-01
**Scope:** all schemas in `fbs/`, the xtask build/catalog/docs pipeline, package
skeletons (`rust/`, `python/`, `js/`, `c/`, `cpp/`), CI/release workflows, Zenoh
topic naming, and the README's stated motivation.
**Wire sizes** below are computed from FlatBuffers struct layout rules
(declaration order, natural alignment, struct padded to max member alignment —
8 bytes here because every payload leads with a `ulong` timestamp).

---

## 1. Executive summary

The core design is sound and I would defend it: one schema source of truth,
fixed-layout structs as the payload ABI, thin table wrappers for FlatBuffers
roots, a generated topic catalog, pinned toolchain, and ROS kept at the edge.
The repo hygiene (mandatory doc comments enforced by xtask, pinned `flatc`,
smoke-tested artifacts) is well above average.

The highest-leverage findings, in priority order:

1. **Do not fork the message set per transport.** Keep one semantic message
   set; vary the *encoding and rate policy* per link instead (§2). The only
   new schema work OTA needs is a small telemetry-aggregate message and a
   compact serial framing, not parallel subsets.
2. **Define semantics in the schema, not "producer-defined".** ~30 fields
   defer meaning (flight modes, fix types, result codes, bitmasks) to the
   producer. This is the single biggest gap between the README's interop
   promise and what the schemas deliver (§5.1).
3. **There is no schema-evolution story.** Structs are frozen ABI; the only
   version signal is `FrameHeader.protocol_version`, which bare-Zenoh
   publishes don't carry. Put a major version in the Zenoh key prefix now,
   while it's still free (§7.2).
4. **Quaternion and Euler conventions are underspecified** (rotation
   direction, Euler sequence). This is the classic source of sign bugs; one
   comment block in `types.fbs` fixes it (§4.3).
5. **Bit-optimality is already close to right.** A handful of fields are
   over- or under-provisioned (GNSS accuracies, battery current/voltage
   ranges, redundant Euler + quaternion, duplicated thrust vector). Detailed
   per-field analysis in §6; most messages need no change.
6. **The catalog misses topics and can't match namespaced keys.**
   OpticalFlow/Mocap topics aren't in `TopicId` or the union at all, and
   `topic_by_key("veh1/synapse/topic/x")` returns `None`, defeating the
   documented namespacing pattern (§7.1, §7.3).
7. **Packaging is solid with a few real gaps:** README claims PyPI trusted
   publishing but the workflow uses an API token; nothing asserts the git tag
   matches `PACKAGE_VERSION`; the C++ archive ships no CMake target while the
   README implies CMake consumption of both archives; no Zephyr
   `module.yml` (§8).
8. **Command/response has no correlation token.** `CommandResult` cannot be
   matched to a specific command when two commands with the same `command_id`
   are in flight. Both structs have free padding bytes — the fix costs zero
   wire bytes (§6, control.fbs).

What's notably good and should not change: scaled-integer lat/lon at `deg_e7`
(the int32 optimum), `GeoCommand` as a precision-preserving command form
(fixes MAVLink's float-args-for-coordinates mistake), the self-describing log
container with embedded `.bfbs`, `TimeReference` as an explicit clock pairing,
and the docs pipeline that fails CI on undocumented fields.

---

## 2. Three transports, one message set?

**Question:** on-chip (Zephyr, shared memory between processors), off-chip
(companion computer ↔ flight controller), over-the-air (long-range GCS) — the
set currently serves all three; should there be subsets per use case?

**Recommendation: no subsets of the semantic messages. One message set, three
*profiles* that differ in encoding, framing, and rate policy.**

Forking per transport is the failure mode MAVLink dialects and ROS
msg-vs-idl duplication both demonstrate: three definitions of "attitude"
drift apart, every bridge becomes a translation layer, and the "same schema
everywhere" motivation dies. The transports differ in *bandwidth and
framing*, not in what an attitude estimate is. Encode that difference in the
catalog and the link layer:

| | On-chip (A) | Off-chip IPC (B) | Over-the-air (C) |
|---|---|---|---|
| Bandwidth | ~free | Mbps+ | 1–200 kbps, lossy |
| Encoding | **bare struct** in shared memory | Zenoh: root table (today) or bare struct | bare struct + compact frame; aggregates on <10 kbps links |
| Discriminator | queue/region identity | Zenoh key | compact frame `topic_id` |
| Rate | native (up to kHz) | native or decimated | policy-limited (0.2–4 Hz) |

Concrete implications:

### 2.1 Make "bare struct" an official encoding

The fixed-layout payloads are already a complete wire format by themselves:
layout is static, so no root table, vtable, or offset is needed to decode
them. Today the README says to publish the root table on Zenoh. The root
wrapper costs roughly 20–24 bytes per message (root uoffset + vtable + table
soffset + alignment) — irrelevant on Ethernet, ~30–50 % overhead on a
40–64-byte payload over a 57.6 kbps radio.

Since the catalog already computes `fixed_layout` per topic, add an
`encoding` concept: a topic's Zenoh value MAY be the bare payload struct when
`fixed_layout` is true, with the key identifying the type. This gives:

- **Use case A:** the struct in shared memory *is* the wire format — chip A's
  writes are chip B's reads and also exactly what a logger or Zenoh bridge
  forwards. Zero serialization anywhere. (Both sides must be little-endian —
  true for all Cortex-M/A/R deployments; state it as a protocol requirement.)
- **Use case C:** 25–40 % of the radio budget back for free.
- Keep the table wrapper for logs, bridges, and any consumer that wants a
  verifiable FlatBuffer; `transport.fbs` `Frame` stays as-is for multiplexed
  byte streams.

Suggested guidance for shared memory specifically: add `force_align`
attributes where DMA or cache-line placement matters, and ship a generated
static-assert header (offsets + sizeof per struct) so both firmware images
verify the ABI at compile time — the `.bfbs` reflection you already generate
contains every offset needed to emit this.

### 2.2 The OTA budget mostly already closes

Typical downlink using bare structs (sizes from §6):

| Topic | Size (B) | Rate (Hz) | B/s |
|---|---|---|---|
| AttitudeEstimate (Euler dropped, §6) | 40 | 4 | 160 |
| GlobalPositionEstimate | 40 | 4 | 160 |
| VehicleHealth | 64 | 1 | 64 |
| PowerStatus | 64 | 1 | 64 |
| GnssFix | 56 | 1 | 56 |
| NavigationTarget + MissionProgress | 72 | 1 | 72 |
| **Total** | | | **~580 B/s ≈ 4.6 kbps** |

With link framing that fits a 57.6 kbps SiK-class radio with a wide margin
and even a 9.6 kbps link marginally. So: **no bit-packing below byte
alignment is warranted anywhere** — the complexity/debuggability cost buys
capacity you don't need. The two OTA-specific artifacts worth adding:

1. **`fbs/telemetry.fbs` — one aggregate status message** (HIGH_LATENCY2
   analog, ~40 B at 0.2–1 Hz) for LoRa/satcom-class links: `time_boot_ms:uint`
   (u32 ms is enough at this layer and drops struct alignment to 4),
   `lat/lon deg_e7`, `alt_msl_m:short`, `yaw_cdeg:ushort`,
   `ground_speed_dm_s:ubyte`, `climb_dm_s:byte`, `battery_pct`, `voltage_cv`,
   `current_da`, `flight_mode`, `failsafe/armed flags`, `link_rssi`,
   `distance_home_dam:ushort`, `error_flags`. One message, quantized for
   display, never used for control.
2. **A compact serial framing spec** for raw byte-stream links: today `Frame`
   wraps the *table* union, so a serial frame pays header struct (24 B) +
   union + table machinery on top of the payload. For constrained links
   define: `[sync][len][topic_id:u16][seq:u8][flags:u8][bare struct][CRC16]`
   (COBS or similar delimiting). ~8 bytes of overhead instead of ~45. Keep
   FlatBuffers `Frame` for logs and generic bridges where self-description
   matters more than bytes.

### 2.3 Tag the catalog instead of splitting schemas

Add per-topic metadata to `topics.json`: `scope` (`onchip` / `ipc` / `ota`),
`nominal_rate_hz`, and `payload_size` (derivable from the `.bfbs`). Gateways
and bridges then enforce policy ("never forward ActuatorCommand off-vehicle",
"GCS profile = these 8 topics at these max rates") from data, not code. This
is the actual answer to "message subsets": subsets exist as *catalog-defined
profiles*, not as schema forks.

### 2.4 Gaps for the GCS use case

The message set covers telemetry and commands well, but "long distance ground
control" needs three protocol flows that have no messages today: mission
upload/download (only `MissionProgress` exists — there is no mission *item*
message), parameter get/set, and log/file transfer. Recommendation: model all
three as Zenoh **queryables** (request/response with timeouts) using
table-based messages — they're definition records consumers cache, which is
exactly the exception `types.fbs` already carves out for tables. Doing
mission transfer as pub/sub is how MAVLink ended up with its notoriously
stateful mission microservice; queryables give you retry/timeout semantics
from the transport.

---

## 3. The ROS stance

**Verdict: the stance is correct, including the sequencing ("could live
within ROS, just not today"). The two premises are essentially right but
worth stating precisely, and the plan can be sharpened.**

### 3.1 The premises, precisely

- **CDR:** CDR is a stream serialization, not an in-memory layout — you
  cannot overlay a received CDR buffer as a C struct in general because
  variable-length fields shift subsequent offsets and alignment is
  stream-relative. For *fixed-size* messages CDR is nearly a packed struct
  and DDS/ROS zero-copy (loaned messages, iceoryx, Zenoh SHM) exists — but it
  requires POD-only message types, which brings you to the real problem:
- **The std message definitions, not the serialization, are the bloat.**
  `std_msgs/Header` carries a dynamic string `frame_id`;
  covariance is `float64[36]`. `nav_msgs/Odometry` is ~720 bytes on the wire
  (two 288-byte f64 covariance blocks alone) vs 240 bytes for
  `OdometryEstimateData`; `sensor_msgs/Imu` is ~320 bytes vs 64 for
  `InertialSampleData`. And any message containing a string is ineligible
  for ROS zero-copy loans. So the README's argument is right; I'd phrase it
  as "ROS common messages are dynamically sized and 8-byte-double-oriented;
  Synapse payloads are fixed-size and precision-budgeted."

### 3.2 How to sharpen the bridge plan

1. **Generate the ROS mirror types from the same schemas.** Add an xtask
   emitter that renders each payload struct to a `.msg` (or `.idl`) in a
   generated `synapse_msgs` package: fixed-size fields only, same names, same
   scaled-integer types. This keeps FlatBuffers as the single source of truth
   while giving ROS users first-class types — and because the mirrors are
   POD, they're eligible for ROS zero-copy loaning. "Living within ROS
   eventually" then costs nothing today: the ROS package is a generated
   artifact of this repo, like the Python wheel.
2. **Use `rclcpp::TypeAdapter` in flatros2** so nodes work directly on the
   generated C++ structs and conversion happens only at the RMW boundary.
   This is the supported ROS mechanism for exactly this "custom in-memory
   type, ROS type on the wire" pattern.
3. **Make the bridge data-driven.** `topics.json` + `.bfbs` reflection is
   enough to route and decode every fixed-layout topic without hand-written
   per-topic code; only the (Synapse ↔ std ROS msg) *convenience* conversions
   (e.g. `OdometryEstimate → nav_msgs/Odometry` for RViz/Foxglove) need
   bespoke code, and only for the handful of topics operators visualize.
4. **Close the two impedance mismatches in the catalog:**
   - **Time:** Synapse is boot-monotonic µs; ROS wants epoch. `TimeReference`
     already exists — document that bridges MUST subscribe to it and how to
     interpolate (and what to do before the first sample arrives).
   - **Frames:** ROS TF needs frame *strings*; `OdometryEstimateData.frame_id`
     is a `ubyte`. Add a frame-id registry (id → name) to the catalog so the
     mapping is generated, not hand-maintained. Same for
     `coordinate_frame` in `LocalPositionCommandData`.
5. **rmw_zenoh coexistence:** ROS 2's Zenoh RMW derives keys from domain +
   topic + type name/hash, so Synapse keys (`synapse/topic/…`) and ROS graph
   keys can share one router without collision — worth stating in the README,
   because "one Zenoh infrastructure, two key namespaces, explicit bridging"
   is a genuinely clean story.

### 3.3 One caution

Don't let the bridge become load-bearing for flight. The stance's strength
is that Synapse works with zero ROS presence; keep autonomy-stack inputs
(offboard setpoints) defined in Synapse terms (`LocalPositionCommand` etc.)
so a ROS-side planner is a *producer of Synapse messages* through the bridge,
not a privileged participant.

---

## 4. Conventions: REP-103 (ENU/FLU) vs aerospace (NED/FRD)

**Recommendation: keep REP-103 as the single canonical wire convention. Do
not mix. Handle the two places aerospace conventions legitimately appear —
raw sensor outputs and operator displays — as layering rules, not as a second
wire convention.**

### 4.1 Why not NED/FRD, given an aerospace team

The honest case for NED/FRD: flight-dynamics literature (Stevens & Lewis,
Etkin), intuitive signs (pitch-up positive with y-right, yaw = compass
heading, gravity +z), PX4/ArduPilot internals, GNSS receivers, and most
estimation papers in aerospace venues. If Synapse were a firmware-internal
format, NED/FRD would be defensible.

But the stated goals — easy ROS integration, browser tools, Foxglove/RViz
visualization, the flatros2 path — all sit in an ENU/FLU world. PX4 is the
cautionary tale here: FRD/NED internally, ENU/FLU at the ROS boundary, and
the `px4_ros_com` frame conversions are a perennial source of sign bugs and
user confusion. Whichever convention you pick, the *other* ecosystem pays a
conversion tax; the question is where you want the tax collected. Given that
Synapse explicitly optimizes for the ROS/tooling edge, collecting it inside
the flight controller (where a small number of experts write the code once)
beats collecting it in every bridge, dashboard, and downstream tool.

Mixing (NED for "aerospace" messages, ENU for "robotics" messages) is
strictly worse than either pure choice — it doubles the documentation burden
and guarantees that someone feeds an NED vector to an ENU consumer. The
current schema is right to refuse it.

### 4.2 The layering rule that resolves the GNSS discomfort

`GnssFixData.course_over_ground_cdeg` / `yaw_cdeg` "zero east, positive
counter-clockwise" means the *driver* must transform what every receiver on
earth reports (clockwise from north) before publishing a message named
"GnssFix". Two problems: raw-sensor topics should be raw (a logged GnssFix
that has been convention-transformed is worse for forensics and receiver
debugging), and every driver author is a fresh opportunity for the sign
error. Recommended rule, stated in `types.fbs`:

- **Raw sensor topics** (`GnssFix`, `RadioControl`, raw IMU fields) carry the
  sensor's native convention, explicitly documented per field
  (`course_over_ground_cdeg` → "clockwise from true north, receiver native").
- **Estimates and commands** (everything the estimator/controller produces or
  consumes) are strictly REP-103 ENU/FLU.
- **Displays** format however operators expect (°T compass heading) — a GCS
  formatting concern, never a wire format.

This matches MAVLink (`GPS_RAW_INT` is receiver-native) and PX4
(`SensorGps.cog_rad` is CW-from-north), and it removes the one place where
your REP-103 purity actively fights the hardware.

### 4.3 The underspecifications that matter more than ENU-vs-NED

These are the actual bug sources; fix them regardless of the convention
debate:

1. **Quaternion semantics** (`types.fbs:62`): "Orientation estimate as a
   unit quaternion" does not say whether it rotates body-frame vectors into
   world frame (passive/active, Hamilton assumed from wxyz order). Specify:
   *"Hamilton convention, w x y z; rotates FLU body-frame vectors into the
   ENU world frame: `v_enu = q ⊗ v_flu ⊗ q⁻¹`."* One sentence, permanent.
2. **Euler sequence** (`AttitudeEuler`): intrinsic ZYX? Specify or —
   better — delete the field entirely (§6, state.fbs): it's 12 redundant
   bytes that can disagree with the quaternion, and gimbal-ambiguous near
   ±90° pitch.
3. **Angle wrap conventions:** `yaw_cdeg` is `ushort` (0..360°) in
   `GnssFixData` but `short` (±180°) in `GlobalPositionEstimateData`. Pick
   one wrap rule for all scaled angles (recommend signed, ±180°, matching
   `atan2`).
4. **Unknown-value sentinels for angles:** `GnssFixData.yaw_cdeg` is "when
   available" — but 0 is a valid yaw (east). Define `0xFFFF` (or
   `INT16_MIN`) as *unknown* for every optional scaled angle, or gate on a
   flags bit. Right now "pointing east" and "no receiver yaw" are the same
   wire value, which is a real defect.
5. Also worth pinning in `types.fbs`: `TimeReferenceData` assumes TAI-free
   Unix time (leap-second smearing? state it), and magnetometer values are
   raw field (not declination-corrected — say so).

---

## 5. Cross-cutting schema issues

### 5.1 "Producer-defined" is the biggest interop gap

Fields whose *meaning* is deferred to the producer: `fields_updated`,
`valid_flags`, `fault_flags`, `fix_type`, `battery_function`, `battery_type`,
`charge_state`, `mode`, `vehicle_type`, `flight_mode`, `system_state`,
`mission_state`, `mission_mode`, `source`, `flags` (×3), `frame_id`,
`child_frame_id`, `estimator_type`, `coordinate_frame`, `type_mask` (×3),
`command_id`, `result_code`, `severity`, `port`, `errors_count0..3`. That's
the semantic layer of the protocol, and none of it is in the schema. The
README promises browser tools, cloud services, and vehicles share "the same
schema source" — but a dashboard cannot render `flight_mode=3` from two
producers without out-of-band agreement, which is exactly the problem MAVLink
solved with its (ugly but effective) enum registry.

While CogniPilot firmware is the only producer this costs nothing — which is
precisely why now is the time to fix it, before third-party producers exist.
Recommend: define FlatBuffers enums/bitmask constants in the schema for at
least the safety- and interop-critical ones — `fix_type`, `severity`
(align with RFC 5424 levels), `result_code` (ACCEPTED / TEMPORARILY_REJECTED
/ DENIED / FAILED / IN_PROGRESS — the MAVLink `MAV_RESULT` set is
battle-tested), `fields_updated` bits, the three `type_mask`s, and
`charge_state`. Leave genuinely vehicle-specific ones (`flight_mode`)
producer-defined but give them a discovery mechanism (a cached definition
table, same pattern as `MocapDefinition`).

### 5.2 Schema evolution needs a stated policy

FlatBuffers structs cannot add, remove, or reorder fields without breaking
every consumer — that's the accepted price of fixed layout, but the repo
never says what happens when (not if) `GnssFixData` needs another field. The
pieces you need already exist: `schema.sha256` is computed and shipped,
`FrameHeader.protocol_version` exists for serial. Recommend:

1. **Version the Zenoh key prefix**: `synapse/v1/topic/<name>` (see §7.2) —
   this is the *only* compatibility signal available to bare-Zenoh consumers,
   and adding it later is itself a breaking change. Do it now.
2. Write the policy in the README: payload struct changes ⇒ major bump ⇒ new
   key prefix; additive new topics ⇒ minor. Because structs are frozen,
   **size arrays for the maximum you'll ever support at design time** — e.g.
   consider 16 battery cells now (14S is common but 16S exists; adding two
   cells later is a wire break, two ushorts today is 4 padded-away bytes).
3. Publish schema identity at runtime: a queryable
   (`synapse/v1/meta/schema`) answering with the `schema.sha256` manifest, so
   mixed-version deployments fail loudly at connect time instead of silently
   misparsing structs.

### 5.3 Inconsistent validity/optionality mechanisms

Four mechanisms coexist: standalone bools (`LocalPositionEstimate`'s four
`*_valid`), bitmasks (`active_mask`), sentinels (`INT16_MAX` in
`ManualControlData` — *in addition to* its `active_axes` mask), and
zero-means-unavailable (`cell0_mv`). Pick a rule: **bitmask per message for
field validity; sentinels never; zero-means-unavailable only where zero is
physically impossible.** Besides consistency this saves bytes:
`LocalPositionEstimateData`'s 4 bools → 1 flags byte frees exactly enough
room to add the missing `xy_reset_counter`/`z_reset_counter` (§6) at
unchanged size.

### 5.4 Unit-suffix system: good idea, incomplete application

The suffix convention plus the docs generator's `unit_scale_note` is a
genuinely nice mechanism. Gaps found: `pixel_flow` (no suffix — is it pixels
or integrated radians? MAVLink's OPTICAL_FLOW_RAD equivalent is radians;
this one *will* cause an integration bug), `thrust` (normalized — suffix
`_norm`?), `residual` (units?), `desired_roll_deg`/`desired_pitch_deg`
(float degrees where every other float angle is radians —
`NavigationTargetData` mixes deg-floats, cdeg-shorts, and rad-floats in one
struct). Also `buttons`, `active_axes` don't hit the docs bitmask heuristic
(`contains("flags")`/`ends_with("_mask")`). Recommend: make xtask *fail* on
any numeric field whose name matches no unit rule — the same enforcement
discipline you already apply to comments — and normalize float angles to
radians everywhere.

### 5.5 Smaller cross-cutting notes

- **Namespace:** `Vec3f`, `Quaternionf`, covariance etc. live in
  `synapse.topic` but aren't topics; `synapse.types` would be cleaner. Wire
  format is unaffected (namespaces don't hit struct layout), and with no
  consumers yet the generated-code rename is free — just do it.
- **`file_identifier`:** `log.fbs` has "SYLG", `sil.fbs` has "SYSI", but
  `transport.fbs` `Frame` (a root type explicitly intended for raw byte
  streams) has none — add "SYFR".
- **Union capacity:** FlatBuffers union discriminators are one byte —
  `SynapseMessage` caps at 255 topics even though `TopicId` is `ushort`.
  Fine, but document it as the `Frame` limit.
- **Unrolled fields vs fixed arrays:** `c0..c20`, `cell0..13`, `chan0..17`,
  `control0..15`, `output0..31` could be `[float:21]`-style fixed arrays —
  supported by flatc C++/Rust and flatcc — but support in Python and
  JS-consumer codegen has historically lagged. Since npm consumers generate
  their own bindings, the unrolled form is the *safe* choice; keep it, but
  add a comment in `types.fbs` saying it's deliberate so nobody "cleans it
  up" into an incompatibility.
- **Log container:** the embedded-`.bfbs` self-describing design is right.
  Consider MCAP as the *container* (FlatBuffers-encoded channels, schemas in
  channel metadata) before the format calcifies — you'd inherit
  Foxglove/PlotJuggler/mcap-cli tooling for free at the cost of container
  control. If you keep the custom container, spec the framing around
  `LogRecord` (length-prefix? CRC? recovery after truncation?) — the schema
  defines records but not the byte stream between them.

---

## 6. Field-by-field review

Sizes are bare-struct wire/ABI sizes. "OK" means the type, unit, range, and
precision are appropriate for the stated 0.1–1 mm-class precision philosophy;
only fields needing comment are listed. General finding first:

**Float32 is the right default for local/body quantities.** f32 gives ~1 mm
absolute resolution at 10 km magnitude (0.25 mm at 2.5 km, 8 mm at 80 km), so
local-frame positions/velocities meet the 0.1–1 mm target everywhere a local
frame is honest, and raw sensor floats (accel, gyro, mag) preserve full
sensor resolution. No message needs float64; none uses it. Scaled ints are
correctly reserved for global coordinates and compact telemetry.

### types.fbs

| Item | Size | Verdict |
|---|---|---|
| `Vec2f`/`Vec3f`/`Quaternionf`/`RateTriplet` | 8/12/16/12 | OK. Quaternion rotation direction must be specified (§4.3). |
| `AttitudeEuler` | 12 | Delete or specify sequence; redundant wherever it appears next to a quaternion. |
| `CovarianceUpperTriangle21f` | 84 | OK for on-chip/log. Element order comment ("row-major upper triangle") is good; state the state-vector order (x,y,z,rot?) explicitly. |
| `TopicId` | — | Missing OpticalFlow, OpticalFlowVelocity, MocapFrame, MocapDefinition (§7.1). |

### sensors.fbs

**`InertialSampleData` — 64 B.** Floats justified (raw sensor, on-chip,
kHz-rate; never OTA). Issues: `pressure_altitude_m` is a *derived* quantity
in a raw-sensor message (producer's atmosphere model — delete; consumers
derive); `differential_pressure_hpa` belongs to AirData, not the IMU sample;
`fields_updated` bits must be schema-defined for multi-rate fusion to be
portable (bit0 accel … bit4 diff-pressure). Removing the two pressure floats
takes the struct to 56 B and cleans the layering.

**`AirDataData` — 48 B.** OK overall. `temperature_cdeg` at 0.01 °C exceeds
any static-air sensor's accuracy but the i16 is the right size anyway — fine.
AoA/sideslip as float rad: OK (estimator products).

**`BatteryCellVoltages14` + `PowerStatusData` — 28 / 64 B.**
- `cellN_mv:ushort` (65.535 V/cell, 1 mV): OK, matches smart-battery
  practice. Consider sizing to 16 cells *now* (frozen struct, §5.2).
- `current_battery_ca:short` → **±327.67 A is insufficient** for large packs
  (200 A+ is routine on 13-inch quads, more on heavy lift). 0.01 A precision
  is wasted there anyway. Switch to deci-amp `current_battery_da:short`
  (±3276.7 A, 0.1 A) — strictly better range/precision trade.
- `energy_consumed_hj`: works, but hectojoules is an eyebrow-raiser; fine
  since the suffix system documents it.
- `remaining_pct:byte` with negative = unknown: OK; document −1 specifically.

**`GnssFixData` — 56 B.** The most OTA-critical raw message; right-size it:
- `latitude/longitude_deg_e7:int` — **optimal, don't touch** (1.11 cm at
  equator; e8 overflows int32; RTK-precision work belongs in the local
  frame).
- `altitude_*_mm:int` — matches the 1 mm philosophy, range ±2147 km. OK.
- `horizontal/vertical/velocity_accuracy_mm(_s):uint` — **over-provisioned**:
  4 bytes each for a 4295 km range. `ushort` mm saturating at 65.5 m (define
  0xFFFF = "≥65 m / unusable") keeps RTK-grade mm resolution and frees
  6 bytes.
- `yaw_accuracy_deg_e5:uint` — 1e-5 deg (0.036 arcsec) resolution for a
  quantity that's 0.5–3° at best: absurd precision. `ushort` cdeg. Frees 2.
- **Missing: vertical velocity.** You carry `velocity_accuracy` but only 2-D
  speed+course; every modern receiver outputs 3-D velocity and climb rate
  matters to the estimator. Add `velocity_up_cm_s:short`.
- **Missing: GNSS time.** A fix without receiver time can't anchor logs or
  do tight time transfer; `time_unix_us:ulong` (or GPS TOW) belongs here,
  not only in `TimeReference`.
- `course_over_ground_cdeg`/`yaw_cdeg`: convention layering (§4.2) and
  unknown-sentinel (§4.3) issues.
- `fix_type` → schema enum (§5.1).
- Net: the right-sizing pays for climb rate + GNSS time inside the same 64 B
  budget the message would otherwise grow into.

### state.fbs

**`VehicleHealthData` — 64 B.** This is effectively your OTA heartbeat; good
scope. `voltage_battery_mv:ushort` caps at 65.535 V — one 16S pack away from
overflow; centivolts (`_cv`, 655 V range, 10 mV) is the better trade.
`errors_count0..3` are MAVLink SYS_STATUS fossils — define or delete.
`drop_rate_comm_cpercent`: OK. The six sensor bitmasks + `_ext` extensions:
schema-define the bits (§5.1).

**`TimeReferenceData` — 16 B.** Exactly right. Add a `source`/uncertainty
field only if you need mixed clock qualities later — as-is, OK.

**`AttitudeEstimateData` — 56 B → 40 B.** Drop `euler_rad` (§4.3): 12 bytes
of redundancy that can disagree with the quaternion, plus the struct's 7
trailing pad bytes shrink. `valid:bool` → flags byte per §5.3.

**`LocalPositionEstimateData` — 56 B.** f32 ENU: precision analysis above —
OK. **Missing: reset counters.** PX4 learned the hard way that controllers
must know when the estimator jumps state; `OdometryEstimateData` has
`reset_counter` but this — the message controllers actually consume —
doesn't. Replace the 4 bools with a validity byte and add
`xy_reset_counter`/`z_reset_counter` at unchanged 56 B.

**`GlobalPositionEstimateData` — 40 B.** The best-designed message in the
set: right types, right scales, compact, OTA-ready. `yaw_cdeg:short` vs
GnssFix's `ushort` — unify wrap convention (§4.3). `source`/`flags` →
schema-defined.

**`OdometryEstimateData` — 240 B.** Covariances dominate (168 B). For its
role (estimator output, ROS bridge, logging) f32 upper-triangle is the right
call — don't shrink to std-devs, correlations matter to consumers doing
fusion. Never forward OTA (catalog `scope` tag). `frame_id`/`child_frame_id`
ubyte → catalog registry (§3.2). `quality_pct:byte` OK.

**`EstimatorHealthData` — 48 B.** Innovation ratios as f32 is more precision
than a health metric needs (u16 centi would halve the message) — but it's
low-rate; acceptable as-is. Optional tightening only.

**`MissionProgressData` — 32 B.** OK. The `*_id` CRC-style identifiers are a
good pattern.

**`NavigationTargetData` — 40 B.** Unit chaos: `desired_roll_deg:float`,
`desired_yaw_cdeg:short`, `altitude_error_m:float` in one struct — three
angle representations. Normalize (float angles → rad, or all setpoint angles
→ cdeg shorts, saving 4 B). `distance_to_waypoint_m:ushort` at 1 m / 65 km:
OK for its advisory role.

**`HomeReferenceData` — 64 B.** OK. `attitude` ("map-to-local alignment
when available") needs the quaternion-direction spec more than anywhere else,
plus an unknown-sentinel convention (identity vs invalid).

### control.fbs

**`RadioControlData` — 48 B.** 18 × raw µs ushort: correct raw
representation (SBUS = 16+2, so 18 is the right frozen size). `rssi` 0–100:
define an unknown sentinel (255) and consider that CRSF-class links report
richer link stats — a future `LinkStatus` topic, not a change here.

**`ManualControlData` — 40 B.** `_milli` shorts: good (bit-efficient enough,
clean decimal). Drop the `INT16_MAX` sentinels in favor of `active_axes`
alone (§5.3). **`throttle_milli` "producer-defined for reverse-capable
vehicles" is a safety-relevant ambiguity** — schema-define: full range
[−1000, 1000], negative = reverse/descend, unidirectional vehicles clamp.

**`AttitudeCommandData` — 56 B / `RateCommandData` — 40 B.** Carrying both
scalar `thrust` and `thrust_body_flu:Vec3f` in every setpoint spends 12 B on
a capability (vectored thrust) almost no consumer has, and `type_mask`
arbitration between them is producer-defined. Recommend scalar-only in these
messages (−12 B → 44→48 / 28→32 actually 40→…; net one alignment slot each)
and a separate vector-thrust topic if/when an omnidirectional vehicle needs
it. `type_mask` bits → schema constants.

**`LocalPositionCommandData` — 56 B.** OK; mirror of the estimate message is
good symmetry. `type_mask`/`coordinate_frame` semantics → schema.

**`VehicleCommandData` — 48 B / `GeoCommandData` — 48 B /
`CommandResultData` — 24 B.** `GeoCommand` fixing MAVLink's
lat-lon-in-float32 defect is exactly right. Three fixes:
- **Add a correlation token.** Two in-flight commands with the same
  `command_id` produce unmatchable results. `VehicleCommandData` has 7 pad
  bytes, `GeoCommandData` 5, `CommandResultData` 6 — add `sequence:ushort`
  to all three at **zero wire cost**; define `0` as "no correlation".
- `current`/`autocontinue` as `ubyte` where the schema uses `bool` elsewhere
  — make them bools (MAVLink fossil).
- `result_code` → schema enum (§5.1); consider Zenoh queryables as the
  command transport (§2.4) with these payloads, which also gives you
  timeout/retry without inventing a confirmation protocol
  (`confirmation:ubyte` is another MAVLink fossil that queryables obsolete).

**`ActuatorCommandData` — 80 B / `ActuatorFeedbackData` — 144 B.** f32
normalized controls: correct for the on-chip/IPC control path (no conversion
in the loop); these never cross the air (enforce via catalog scope). The
16-command vs 32-feedback asymmetry is presumably logical-controls vs
physical-outputs — document it. `_milli` shorts would halve them if UART
bandwidth to an IO coprocessor ever matters; not worth it today.

**`PwmSignalOutputsData` — 48 B / `ControlLoopMetricsData` — 24 B.** OK.

**`MotorOutputData` — 40 B.** Redundant with ActuatorFeedback for a specific
vehicle class; fine as a compact convenience, but it's the first instance of
per-vehicle-class message proliferation — hold the line here (the answer to
"hexacopter?" must be ActuatorFeedback, not `MotorOutput6`).

### transport.fbs

**`FrameHeader` — 24 B, `Frame`.** Design intent (optional envelope, Zenoh
publishes bare) is right and well-commented. `TextStatus` as a table: correct
(genuinely variable). `severity` → RFC 5424-aligned enum. For serial links
the union-of-tables framing is heavy — see compact framing, §2.2. Add
`file_identifier "SYFR"`.

### optical_flow.fbs

**`OpticalFlowData` — 56 B / `OpticalFlowVelocityData` — 56 B.**
`pixel_flow` units undefined (§5.4) — this is the single most
likely-to-cause-a-real-bug field in the repo; if it's integrated radians,
name it `flow_rad:Vec2f`. `quality` 0–255 while every other quality/health
field is 0–100 — unify. **Not in `TopicId`/union/catalog** — so flow can't be
routed by bridges at all today (§7.1).

### mocap.fbs

Tables with vectors: correct (variable counts); the Definition/Sample split
with cacheable definitions is a good pattern (it's the discovery mechanism
§5.1 asks for — generalize it). f32 positions over LAN: fine. Not in the
catalog — add topics (`mocap_frame`, `mocap_definition`).

### log.fbs

Solid (§5.5 for container-level notes). `LogFrame.topic_id:uint` vs
`TopicId:ushort` — presumably to allow log-local dynamic topics; document
that or align widths.

### sil.fbs

OK as a sim boundary (table is fine here, not a runtime topic).
`target_boot_time_ns:ulong` is the only nanosecond timestamp in the repo —
either justify (sim determinism at sub-µs?) or make it `_us`.

### Wire-size reference (bare struct, bytes)

| 16–24 | 32–40 | 48 | 56 | 64 | ≥80 |
|---|---|---|---|---|---|
| TimeReference 16, CommandResult 24, ControlLoopMetrics 24, FrameHeader 24 | MissionProgress 32, GlobalPositionEstimate 40, RateCommand 40, ManualControl 40, MotorOutput 40, NavigationTarget 40 | AirData 48, RadioControl 48, VehicleCommand 48, GeoCommand 48, PwmSignalOutputs 48, EstimatorHealth 48 | GnssFix 56, LocalPositionEstimate 56, AttitudeCommand 56, LocalPositionCommand 56, AttitudeEstimate 56 (→40), OpticalFlow 56, OpticalFlowVelocity 56 | InertialSample 64, PowerStatus 64, VehicleHealth 64, HomeReference 64 | ActuatorCommand 80, ActuatorFeedback 144, OdometryEstimate 240 |

Every runtime topic except Odometry fits a single BLE/LoRa-class MTU; the
distribution confirms the design is already OTA-plausible without packing
tricks. Recommend xtask emit this table into the docs automatically from the
`.bfbs` reflection (which also replaces the hand-rolled `.fbs` parser in
`xtask/src/main.rs` with exact, guaranteed-correct layout data, and enables a
CI check asserting sizes never change unintentionally — a frozen-ABI
regression test).

---

## 7. Topic catalog and Zenoh naming

### 7.1 Catalog completeness and consistency checks

- `topic_entries` (xtask/src/main.rs:1326) derives the catalog from `TopicId`
  only. **OpticalFlow, OpticalFlowVelocity, MocapFrame, MocapDefinition have
  no TopicId, no union entry, no key, no catalog row** — they're unroutable
  by every bridge built on `topics.json`. If intentional ("support
  schemas"), say so in the README; I think flow at least is core estimator
  input and belongs in the catalog. Union entries are append-only, so adding
  early is cheap; retrofitting reorders nothing.
- xtask never cross-checks `TopicId` ↔ `SynapseMessage` union membership.
  Today they happen to match (1–28 in order); add the validation — it's a
  five-line check in `topic_entries` and prevents a silent routing gap.
- Nothing asserts TopicId values are unique/contiguous or that the union
  order matches — cheap insurance while the parser exists.

### 7.2 Key expression design

`synapse/topic/<snake_case_name>` is clean. Three changes, all cheaper now
than later:

1. **Version segment: `synapse/v1/topic/<name>`.** The bare-Zenoh consumer
   has *no* other compatibility signal (§5.2). The catalog already carries
   `version: 1` — put it in the key where it actually protects anyone.
   Rolling upgrades then work by running v1 and v2 topics side by side.
2. **Instance segment for multi-instance sensors.** `InertialSampleData.id`,
   `GnssFixData.id`, `PowerStatusData.id` are *inside* the payload, where
   Zenoh selectors can't see them; a subscriber wanting only IMU 0 must
   receive and decode everything. Publish at
   `synapse/v1/topic/inertial_sample/<id>` (keep the payload `id` for
   logs/serial). Wildcards (`.../inertial_sample/*`) preserve
   subscribe-all.
3. **Reserve sibling namespaces now:** `synapse/v1/cmd/…` (queryables for
   commands/mission/param, §2.4), `synapse/v1/meta/…` (schema hash,
   definitions). Cheap to reserve, breaking to retrofit.

Also document the **late-joiner pattern**: Zenoh pub/sub has no retained
messages, so a GCS connecting mid-flight sees no `HomeReference`,
`MocapDefinition`, or `TimeReference` until the next (possibly rare)
publish. Convention needed: low-rate periodic republish, or publishers double
as queryables for last-value, or a zenoh-storage instance — pick one and
write it down; every deployment will hit this in week one.

### 7.3 Helper-function gap: namespaced keys don't match

The README's deployment story is "prefix keys with a vehicle/swarm/site
namespace", but `topic_by_key` (all languages, e.g. the Rust template) only
matches the exact canonical key or bare suffix after stripping leading
slashes — `topic_by_key("quad7/synapse/topic/vehicle_health")` → `None`. So
the catalog helpers fail on exactly the keys the README recommends. Fix: match
any key whose trailing segments equal `synapse/topic/<suffix>` (or, with
§7.2, `synapse/v<N>/topic/<suffix>`), and add a helper that also returns the
extracted namespace prefix. Worth a unit test per language template.

### 7.4 Catalog content additions

Per §2.3/§3.2: `payload_size`, `scope`, `nominal_rate_hz`, `encoding`
(`table` | `struct`), a frame-id registry, and the schema hash. `topics.json`
is versioned (`version: 1`) so these are non-breaking additions — and they
convert the catalog from a name-lookup table into the deployment policy
source the bridges need.

---

## 8. Packaging, release, and deployment

### 8.1 Findings by artifact

**Rust crate.** Exact pin `flatbuffers = "=25.12.19"` guarantees lockstep but
is hostile in dependency graphs: any other crate pinning a different exact
25.x makes the tree unresolvable (same-major semver unification). Options:
(a) keep `=` and accept being hard to co-depend; (b) test a range
`>=25.12.19, <26` in CI and relax. Given generated-code/runtime coupling I'd
try (b) with the CI gate you already have. Cosmetics: `Cargo.toml.jinja`
lacks `readme`, `keywords`, `categories`, `rust-version` — the crates.io page
is currently bare.

**Python package.** Same exact-pin consideration (`flatbuffers==25.12.19`
conflicts with any co-installed package pinning differently; `>=x,<26` is
kinder). **README/workflow mismatch:** README says PyPI uses *trusted
publishing*, but `release.yml` uses `twine` + `PYPI_API_TOKEN`
(`id-token: write` is granted but unused for PyPI). Either fix the README or
— better — switch to `pypa/gh-action-pypi-publish` with OIDC and delete the
long-lived token.

**npm package.** Schema-assets-only with documented rationale: good call,
and `--provenance` publishing is ahead of the curve. `exports` map and
shipped hash manifests are well done.

**C archive.** The FetchContent + `URL_HASH` consumption story and the CMake
smoke test are exactly right. Two additions for the Zephyr use case (your
use case A!): ship a `zephyr/module.yml` (+ minimal Kconfig/CMake glue) in
the archive so it drops into a west manifest as a module, and consider
documenting `FLATCC_PORTABLE` needs for non-POSIX toolchains. Also note the
flatcc runtime is only needed for builders/verifiers — already documented,
good.

**C++ archive.** Ships headers but **no CMakeLists** — unlike the C archive
— while the README groups both as "for downstream CMake consumers" and the
smoke test compiles it with a raw `c++ -I` invocation
(xtask/src/main.rs:2870). Add the same INTERFACE-target treatment
(`synapse_fbs::cpp`) for parity; it's ~10 lines of template.

**Docs pipeline.** mdBook site with enforced comments, unit inference,
version selector, `.fbs` copied alongside: genuinely good. With the `.bfbs`-
driven generation (§6) you can add per-struct size/offset tables — the thing
firmware reviewers will actually look up.

### 8.2 Release workflow

- **No tag ↔ `PACKAGE_VERSION` assertion.** Pushing `v0.2.0` with
  `tools.lock` still at `0.1.8` publishes 0.1.8-versioned packages from a
  v0.2.0 tag and uploads them to a v0.2.0 GitHub release. Add a check in
  `xtask ci` when `release_name` matches `v*`: strip the `v`, compare to
  `PACKAGE_VERSION`, fail loudly.
- **Partial-release recovery:** publish steps run sequentially
  (GH release → crates.io → PyPI → npm); a PyPI failure leaves crates.io
  published, and re-running the job dies at crates.io (publishes aren't
  idempotent). Add already-published guards (`cargo search`/HTTP checks or
  `--skip-existing` for twine) so a re-run converges.
- CI is single-platform (ubuntu) — acceptable for generated artifacts, worth
  a macOS/Windows consumer smoke eventually since wheels/npm are
  platform-independent but path handling in `index.js` etc. isn't tested
  there.
- `parse_schema_docs` is a hand-rolled `.fbs` parser feeding catalog + docs.
  It works because the schema style is disciplined, but it will silently
  mis-parse the first attribute/multi-line construct someone writes. The
  `.bfbs` reflection you already generate is the robust source — migrate
  catalog+docs to it and keep the text parser only for comment extraction.

---

## 9. Prioritized recommendations

Synapse is a new protocol with no deployed consumers: backward compatibility
is a non-goal today, so there is no reason to stage or batch "breaking"
changes — every design decision below should simply be made now, while it
costs nothing. The one place versioning still matters is the *future*: the
key-prefix version segment isn't compat ceremony, it's the mechanism that
keeps post-adoption evolution cheap, and it's cheapest to install while the
answer is just "v1". Priority is therefore ordered by design leverage, not
compatibility impact.

**Now — schema and protocol decisions (highest leverage while unconstrained):**
1. Specify quaternion direction and Euler sequence; delete `AttitudeEuler`
   (§4.3, §6).
2. GNSS raw-layer convention (receiver-native COG), one angle-wrap
   convention, unknown-value sentinels, `throttle_milli` semantics (§4.2,
   §4.3, §5.3).
3. `GnssFixData`: add vertical velocity + GNSS time, right-size accuracies
   (§6).
4. Version segment in Zenoh keys (`synapse/v1/topic/…`) + instance segment
   for multi-instance sensors; write the evolution policy in the README
   (§5.2, §7.2).
5. Schema-defined enums for core semantics: fix_type, severity, result_code,
   fields_updated, type_masks (§5.1).
6. Command `sequence` correlation in VehicleCommand/GeoCommand/CommandResult
   (§6).
7. Validity consolidation (bools → flags bytes); reset counters in
   `LocalPositionEstimateData` (§5.3, §6).
8. Battery current → dA, pack voltage → cV, consider 16 cells; thrust-vector
   removal from Attitude/RateCommand; `NavigationTarget` unit normalization;
   InertialSample layering cleanup; `pixel_flow` units (§6).
9. Add OpticalFlow/Mocap topics to TopicId/union/catalog; move math types to
   `synapse.types` (§7.1, §5.5).

**Now — tooling and release hygiene (independent of schema decisions):**
10. Tag ↔ `PACKAGE_VERSION` release assertion (§8.2).
11. PyPI trusted publishing (or fix the README wording) (§8.1).
12. TopicId ↔ union ↔ catalog consistency checks; unit-suffix lint (§7.1,
    §5.4).
13. Namespaced-key matching in the catalog helpers (§7.3).
14. Zephyr `module.yml` in the C archive; CMake target in the C++ archive
    (§8.1).
15. Catalog metadata additions (`payload_size`, `scope`, `nominal_rate_hz`,
    `encoding`); late-joiner pattern documentation (§7.4, §7.2).

**Roadmap — additive features, any time:**
16. Bare-struct encoding profile + compact serial framing spec (§2.1–2.2).
17. `fbs/telemetry.fbs` low-rate aggregate (§2.2).
18. Mission/param/file transfer as Zenoh queryables (§2.4).
19. Generated `synapse_msgs` ROS mirrors + TypeAdapter-based flatros2 +
    frame-id registry (§3.2).
20. `.bfbs`-driven catalog/docs generation + auto size tables + frozen-ABI
    size regression check (§6, §8.2).
21. Evaluate MCAP as log container; spec the log byte-stream framing (§5.5).
22. Relax runtime pins to tested ranges; crate metadata polish (§8.1).
