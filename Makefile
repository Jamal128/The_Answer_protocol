CARGO := cargo
WORLD_FILE ?= data/world.yaml
HOST ?= 127.0.0.1
PORT ?= 4242

.PHONY: all install build run-server run-client run-client-gui lint test clean

all: build

## Install / fetch Rust dependencies and verify toolchain components
install:
	rustup component add clippy rustfmt
	$(CARGO) fetch
	$(CARGO) build --workspace

## Compile all crates in the workspace
build:
	$(CARGO) build --workspace

## Run the TCP game server
run-server:
	$(CARGO) run -p server -- --host $(HOST) --port $(PORT) --world $(WORLD_FILE)

## Run the terminal (CLI) client
run-client:
	$(CARGO) run -p cli -- --host $(HOST) --port $(PORT)

## Run the graphical (GUI) client
run-client-gui:
	$(CARGO) run -p gui -- --host $(HOST) --port $(PORT)

## Run formatter check and strict Clippy linter
lint:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings

## Run unit and integration tests
test:
	$(CARGO) test --workspace

## Clean build artifacts
clean:
	$(CARGO) clean