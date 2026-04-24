# synapse-fbs

Generated Python FlatBuffers bindings for the Synapse message schemas.

The schema source of truth lives in `fbs/`. CI stages this package under
`target/xtask/packages/python`, generates bindings there, then publishes it to
PyPI. To build locally from the repository root:

```sh
cargo run --locked --manifest-path xtask/Cargo.toml -- ci
```

Install the staged wheel locally with:

```sh
pip install target/xtask/packages/python/dist/*.whl
```
