# @cognipilot/synapse-fbs

Canonical Synapse FlatBuffers schemas and generated reflection assets for the
JavaScript and TypeScript ecosystem.

The schema source of truth lives in `fbs/`. CI stages this package under
`target/xtask/packages/js`, copies the schemas, generates the matching `.bfbs`
reflection schemas, then publishes it to npm. To build locally from the
repository root:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- ci
```

## What this package ships

- `fbs/*.fbs` — canonical Synapse schemas.
- `bfbs/*.bfbs` — generated FlatBuffers reflection schemas (same assets bundled
  in the C/C++ release archives).
- `schema.sha256` / `bfbs.sha256` — content hashes for the shipped assets.

Runtime protocol payloads prioritize fixed memory layout. Telemetry, state,
command, and control samples are modeled as FlatBuffers structs where possible
so chip-to-chip shared-memory transports can use the payload layout directly.
Tables, strings, and vectors are reserved for root wrappers, metadata, logs, or
naturally variable-size data.

Unlike the Rust and Python packages, this package does **not** ship generated
language bindings and does **not** depend on the `flatbuffers` runtime. The npm
`flatbuffers` release cadence does not track the pinned `flatc` version, so
runtime lockstep cannot be guaranteed here. JavaScript and TypeScript consumers
generate their own bindings from the shipped schemas, or decode messages using
the shipped reflection schemas.

## Usage

```js
import { fbsDir, bfbsDir, schemaFiles, schemaPath } from '@cognipilot/synapse-fbs';
import { readFileSync } from 'node:fs';

// Resolve and read the canonical Synapse log schema.
const logSchema = readFileSync(schemaPath('log.fbs'), 'utf8');

// Or point a code generator at the shipped schema directory.
// flatc --ts -I <fbsDir> <fbsDir>/all.fbs
```

Individual assets are also directly importable via subpath exports:

```js
import logSchemaUrl from '@cognipilot/synapse-fbs/fbs/log.fbs';
```
