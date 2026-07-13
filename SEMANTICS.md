# Synapse schema semantics

Synapse keeps `fbs/*.fbs` as its only message-schema source of truth. The
FlatBuffers compiler emits compact binary reflection schemas (`.bfbs`). The
Rust generator lowers BFBS into a stable, serializable semantic IR and renders
the target-neutral `synapse-schema.json` manifest with MiniJinja.

```text
fbs/*.fbs -> flatc -> compact BFBS -> Rust semantic IR -> synapse-schema.json
                     |
                     +-> existing C/C++/Rust/Python/JavaScript bindings
```

The BFBS representation is authoritative for declarations, resolved types,
field order and IDs, enum values, built-in and custom attributes, offsets,
dimensions, and fixed-struct layout. Semantic metadata does not participate in
the wire-schema hash and therefore does not change serialized bytes.

All BFBS files are generated without `--bfbs-comments`. They retain structural
and semantic attributes but contain no prose documentation, so the exact same
compact assets can be shipped in runtime packages, compiled into firmware, and
written into MCAP Schema records. CI validates every generated BFBS for
accidental comments.

The Rust generator associates source `///` documentation with BFBS declarations
by exact qualified type and field path. BFBS remains authoritative for
structure and layout; `.fbs` remains authoritative for prose. Normal
FlatBuffers and FlatCC language generators consume the commented `.fbs`
sources directly and may retain that documentation in generated code.

## Attribute profile

The profile identifier is `synapse-fbs-semantics/1`. Attributes are declared
once in `fbs/semantic.fbs` and may be attached to fields:

| Attribute | Meaning | Example |
| --- | --- | --- |
| `unit` | Physical unit after applying `scale` | `"m/s"`, `"rad/s"`, `"s"` |
| `min`, `max` | Inclusive semantic bounds in the decoded `unit` | `"-90"`, `"100"` |
| `frame` | Coordinate frame | `"enu"`, `"flu"`, `"wgs84"` |
| `clock` | Clock represented by a time field | `"monotonic_boot"`, `"unix_epoch"` |
| `scale` | Multiplier from encoded value to `unit` | `"1e-6"`, `"1e-7"` |
| `valid_if` | Human/tool-readable validity condition | `"flags.AngularVelocityValid"` |
| `logical_type` | Meaning not captured by storage type | `"quaternion_flu_to_enu"` |

`scale` defines `decoded_value = stored_value * scale`. Bounds apply to the
decoded value, are inclusive, and apply when `valid_if` is satisfied.
Storage-width limits are available separately in the manifest and are not
repeated unless the same bound is also part of the domain contract.

Attributes describe values, not deployment policy. Topic rates, logging
selection, mission capacities, storage paths, and transport routing belong to
firmware or deployment configuration.

## Target-neutral manifest

`synapse-schema.json` contains structs, tables, enums, unions, fields, source
documentation, semantic attributes, scalar widths and signedness, references,
dimensions, field IDs and offsets, variable-size status, fixed-layout size and
alignment, enum/flag values, and union targets.

Variable-size tables remain in the manifest because adapter and service tools
need to understand them. They are not embedded controller snapshots; services
validate and stage variable-size transfers before committing bounded domain
state.

The manifest is an input to future adapter generation. It does not define the
message-independent Modelica domain API. `modelica_models` owns the top-level
`Autopilot` library and records under `Autopilot.Interfaces`. Rumoca emits a
separate model-interface manifest, and an external mapping specification
associates domain fields with Synapse fields. Frame, unit, time, validity,
aggregation, splitting, rejection, and lossy-conversion policy remains explicit
and reviewable.

Regenerate the checked-in manifest inside the pinned development shell:

```sh
nix develop --command cargo run --locked --manifest-path xtask/Cargo.toml -- semantic
```

Full CI regenerates the manifest from pinned FlatBuffers 25.12.19 and fails if
it differs. Releases publish a versioned copy of `synapse-schema.json` with a
SHA-256 checksum. The manifest is rendered with MiniJinja; generation,
validation, versioning, and release assembly are implemented in Rust.
