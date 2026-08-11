# Package usage

Release artifacts are generated from the same schemas and templates. The
package version and all generator/runtime pins come from `flake.nix`.

## Rust

Install the published crate:

```toml
[dependencies]
synapse_fbs = "0.8"
```

Enable the `mcap` feature for the `synapse/1` reader and writer. After a local
package build, point a project at the staged crate:

```toml
synapse_fbs = { path = "../synapse_fbs/target/xtask/packages/rust" }
```

The crate includes generated bindings, schema sources, BFBS assets, the topic
catalog, value-contract helpers, and the debug decoder.

## Python

Install the package or the optional MCAP surface:

```sh
pip install synapse-fbs
pip install 'synapse-fbs[mcap]'
```

MCAP users import `synapse.mcap.Writer` or `synapse.mcap.Reader`. To test the
locally generated wheel:

```sh
pip install target/xtask/packages/python/dist/*.whl
```

## JavaScript and TypeScript

Install the package:

```sh
npm install @cognipilot/synapse-fbs
```

Logging also uses the maintained MCAP container package:

```sh
npm install @cognipilot/synapse-fbs @mcap/core
```

```js
import { Reader, Writer } from '@cognipilot/synapse-fbs/mcap';
```

The npm package ships `.fbs` and BFBS schema assets plus catalog and MCAP
helpers. It does not pin a JavaScript FlatBuffers runtime; consumers may
generate bindings from the packaged schemas or decode through reflection.

The local package is staged at `target/xtask/packages/js`.

## C and C++

GitHub releases contain self-contained generated archives. Extract one and add
its root to `CMAKE_PREFIX_PATH`:

```cmake
find_package(synapse_fbs 0.8.0 CONFIG REQUIRED)
target_link_libraries(app PRIVATE synapse_fbs::c)
```

The C archive exports:

- `synapse_fbs::c`
- `synapse_fbs::flatcc_runtime`
- `synapse_fbs::print`
- `synapse_fbs::mcap`

The C++ archive exports `synapse_fbs::cpp` and `synapse_fbs::mcap`.

Projects without a dependency setup can fetch a release directly:

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

Use `synapse_fbs-cpp.tar.gz` and `synapse_fbs::cpp` for C++.

The C reader accessors are header-only. Link the FlatCC runtime only for
builders, verifiers, or JSON helpers.

## Zephyr

The C archive includes `zephyr/module.yml`, Kconfig integration, generated C
headers, and the allocation-free embedded MCAP writer. West projects can add
the extracted archive as a module. Enable `CONFIG_SYNAPSE_FBS_MCAP` only when
logging is required so disabled logging has no linked-code cost.

## Catalog assets

Every language exposes the generated topic and command catalog. It includes
IDs, canonical keys, scopes, encodings, payload sizes, instance behavior, wire
types, and schema hashes. C and C++ archives also ship `topics.json`.

Release archives contain `fbs/`, `bfbs/`, `schema.sha256`, and `bfbs.sha256`
so downstream builds and log readers can verify the exact schema set.
