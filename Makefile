.PHONY: build test fmt clippy clean

# The factory's tests deploy the real escrow WASM, so it must be built first.
build:
	cargo build --target wasm32v1-none --release -p trustescrow-escrow -p trustescrow-factory

test: build
	cargo test

fmt:
	cargo fmt --all

clippy: build
	cargo clippy --all-targets -- -D warnings

clean:
	cargo clean
