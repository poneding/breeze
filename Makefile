.DEFAULT_GOAL := help
SHELL := sh

CARGO ?= cargo
RUSTC ?= rustc
PYTHON ?= python3
CARGO_TARGET_DIR ?= target
TARGET ?= $(shell $(RUSTC) -vV | awk '/^host:/ {print $$2}')
VERSION ?= $(shell $(PYTHON) -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')
BREEZE_DATA_DIR ?= $(CURDIR)/target/manual-state
ARGS ?=

# Keep local release builds on the same minimum macOS version as CI bundles.
MACOSX_DEPLOYMENT_TARGET ?= 15.0
export CARGO_TARGET_DIR MACOSX_DEPLOYMENT_TARGET

.PHONY: help fmt fmt-check check lint test test-tools verify build run release package clean

help:
	@printf '%s\n' \
	  'make check       Check formatting and compile all workspace targets' \
	  'make lint        Run Clippy with warnings denied' \
	  'make test        Run Rust and release-tool tests' \
	  'make verify      Run check, lint, and test' \
	  'make build       Build the workspace in the development profile' \
	  'make run         Run Breeze with isolated data (optional ARGS="...")' \
	  'make release     Build an optimized app for TARGET (defaults to host)' \
	  'make package     Build installers under target/dist/' \
	  'make fmt         Apply rustfmt' \
	  'make fmt-check   Check rustfmt without changing files' \
	  'make test-tools  Run release-tool tests (Python 3.11+)' \
	  'make clean       Remove Cargo build outputs' \
	  '' \
	  'Overrides: CARGO, RUSTC, PYTHON, CARGO_TARGET_DIR, TARGET, VERSION,' \
	  '           BREEZE_DATA_DIR, ARGS, MACOSX_DEPLOYMENT_TARGET' \
	  'release/package never create tags, push commits, or publish releases.'

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

check: fmt-check
	$(CARGO) check --workspace --all-targets --locked

lint:
	$(CARGO) clippy --workspace --all-targets --locked -- -D warnings

test: test-tools
	$(CARGO) test --workspace --locked

test-tools:
	$(PYTHON) -B -m unittest discover -s scripts -p 'test_*.py' -v

verify: check lint test

build:
	$(CARGO) build --workspace --locked

run:
	BREEZE_DATA_DIR="$(BREEZE_DATA_DIR)" $(CARGO) run --locked -p breeze-app -- $(ARGS)

release:
	$(CARGO) build --release --locked -p breeze-app --target "$(TARGET)"

package: release
	$(PYTHON) scripts/release.py package --version "$(VERSION)" --target "$(TARGET)" \
	  --binary "$(CARGO_TARGET_DIR)/$(TARGET)/release/breeze-app$(if $(findstring windows,$(TARGET)),.exe,)"

clean:
	$(CARGO) clean
