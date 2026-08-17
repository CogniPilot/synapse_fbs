# synapse-fbs

Generated Python FlatBuffers bindings for the Synapse message schemas.

The schema source of truth lives in `fbs/`. CI stages this package under
`target/xtask/packages/python`, generates bindings there, then publishes it to
PyPI. To build locally from the repository root:

```sh
make packages
```

Install the staged wheel locally with:

```sh
pip install target/xtask/packages/python/dist/*.whl
```

Install with the optional upstream MCAP implementation for first-class log
reading and writing:

```sh
pip install 'synapse-fbs[mcap]'
```

```py
from synapse.mcap import Reader, TimeBasis, Writer
```

`Writer` applies the frozen `synapse/1` metadata, embeds the exact packaged
BFBS, and defaults to direct uncompressed, unchunked output. `Reader` validates
the profile and required metadata before yielding upstream MCAP records. It
does not require the log's schema-set hash to equal the installed release:
historical logs are decoded from their own embedded BFBS.

The generated package includes topic catalog helpers for bridge and routing
code:

```py
from synapse import topic_catalog

key = topic_catalog.key_for_topic("VehicleHealth")
payload_type = topic_catalog.topic_by_id(1).payload_type
```
