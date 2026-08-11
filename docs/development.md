# Development and releases

## Supported environment

The Nix flake is the supported toolchain on `x86_64-linux` and
`aarch64-linux`. On other hosts, use a Linux VM, container, or WSL environment.

Enter an interactive shell with:

```sh
nix develop
```

The shell and Nix apps share the same compiler, runtime, and package-tool pins.
GitHub Actions invokes those apps directly, so local and hosted commands cannot
drift.

## Public commands

Run all fast verification:

```sh
nix run .#test
```

This performs `cargo fmt --check`, Clippy with warnings denied, FlatCC schema
compilation, BFBS reflection checks, wire-compatibility validation, and the
catalog smoke tests.

Build and verify release packages:

```sh
nix run .#packages
```

`nix run .#build` is an alias. Arguments after `--` are passed to `xtask ci`,
which lets the release workflow use:

```sh
nix run .#packages -- --release-name v0.8.0
```

Run the complete branch CI sequence:

```sh
nix run .#ci
```

## Generation flow

The source inputs are deliberately separated:

- `fbs/` contains only the authoritative FlatBuffers schemas.
- `templates/{rust,python,js,c,cpp}` contains package skeletons.
- `templates/xtask` contains MiniJinja templates for generated catalogs,
  checksums, BFBS source assets, and smoke programs.

`xtask` asks FlatCC to compile BFBS reflection schemas, then reads reflection
data to validate and generate metadata. It does not parse `.fbs` syntax itself.
The pinned upstream `flatc` remains only for official Rust, Python, and C++
binding generation; FlatCC generates C bindings and BFBS.

Generated outputs live below `target/xtask/` and are safe to remove:

- `packages/rust`
- `packages/python`
- `packages/js`
- `artifacts`
- build, check, smoke, and downloaded-source work directories

## Wire hashes

Per-type and schema-set hashes are computed from FlatCC BFBS reflection on every
build and rendered into generated catalogs. No checksum baseline is committed.
Normal Zenoh consumers compare the per-type hash; constrained endpoints compare
the schema-set hash before exchanging compact frames.

Published wire-type names should remain immutable. An incompatible payload
change gets a new wire type and topic so old and new consumers fail clearly
rather than silently decoding different layouts.

## Package verification

The package command:

1. stages Rust, Python, and JavaScript skeletons;
2. renders every generated text file through MiniJinja;
3. generates official language bindings and BFBS assets;
4. builds and tests the Rust crate;
5. builds, checks, installs, and imports the Python wheel;
6. type-checks, packs, installs, and imports the npm package;
7. assembles C and C++ archives with pinned runtimes;
8. builds CMake `FetchContent` and `find_package` consumers;
9. writes and reads real MCAP logs across languages;
10. emits schema and BFBS checksum manifests.

## Version pins

`flake.nix` is the single version manifest for the package version, FlatBuffers,
FlatCC, MCAP implementations, and TypeScript. Git commits are pinned where a
source build is required. Generated bindings and their runtimes remain in
lockstep.

## Release workflow

Pushes and pull requests run `nix run .#ci`. A tag matching `v*.*.*` runs the
release workflow. The tag must match `package.version` in `flake.nix`.

The release publishes:

- the staged Rust crate through crates.io trusted publishing;
- the Python wheel and source distribution through PyPI trusted publishing;
- the npm tarball through npm trusted publishing;
- C and C++ tarballs plus checksum files on the GitHub Release.

The repository and workflow identities must be registered with each package
registry before tagging. A failed publication can be retried safely: the
workflow checks which package versions already exist before publishing.

## CI action maintenance

CI uses Node-24-compatible GitHub Actions. The Nix cache is explicitly backed
by GitHub Actions cache and does not attempt unauthenticated FlakeHub access.
Validate workflow edits locally with:

```sh
nix shell nixpkgs#actionlint --command actionlint
```
