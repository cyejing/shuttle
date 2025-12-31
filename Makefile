NAME := shuttle
PACKAGE_NAME := github.com/cyejing/shuttle
VERSION := `git describe --dirty`
COMMIT := `git rev-parse HEAD`

.PHONY: build

build: 
	cargo build --verbose --release

test:
	cargo test --verbose

clean:
	cargo clean

check: 
	cargo check
	cargo fmt --check -v
	cargo clippy -- -D warnings

fix:
	cargo clippy --all --fix --allow-dirty --allow-staged
	cargo fmt
