CARGO ?= cargo
RELEASE_NAME ?= local
XTASK = $(CARGO) run --locked --manifest-path xtask/Cargo.toml --

.PHONY: bootstrap bootstrap-offline test local local-offline packages ci
.NOTPARALLEL:

bootstrap:
	$(XTASK) bootstrap

bootstrap-offline:
	CARGO_NET_OFFLINE=true $(XTASK) bootstrap --offline

test: bootstrap
	$(CARGO) fmt --check --manifest-path xtask/Cargo.toml
	$(CARGO) clippy --locked --manifest-path xtask/Cargo.toml --all-targets -- -D warnings
	$(CARGO) test --locked --manifest-path xtask/Cargo.toml
	$(XTASK) check
	$(XTASK) wire-check --update
	git diff --exit-code compatibility/wire-schema.toml

local: bootstrap
	$(XTASK) build --release-name local

local-offline: bootstrap-offline
	CARGO_NET_OFFLINE=true $(XTASK) build --release-name local

packages: bootstrap
	$(XTASK) ci --release-name "$(RELEASE_NAME)"

ci: test local packages
