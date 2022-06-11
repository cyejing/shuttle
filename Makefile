NAME := shuttle
PACKAGE_NAME := github.com/cyejing/shuttle
VERSION := `git describe --dirty`
COMMIT := `git rev-parse HEAD`

.PHONY: build

build: 
	cargo build --verbose

clean:
	cargo clean

check: 
	cargo fmt
	cargo check
	cargo fmt --check -v
	cargo clippy -- -D warnings
	cargo test --verbose



