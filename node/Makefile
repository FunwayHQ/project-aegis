# AEGIS Node Makefile
# Convenience commands for development

.PHONY: help build test run clean check fmt lint

help:
	@echo "AEGIS Node Development Commands"
	@echo ""
	@echo "  make build     - Build the project in release mode"
	@echo "  make test      - Run all tests"
	@echo "  make run       - Run the server locally"
	@echo "  make check     - Check code without building"
	@echo "  make fmt       - Format code with rustfmt"
	@echo "  make lint      - Run clippy linter"
	@echo "  make clean     - Clean build artifacts"

build:
	cargo build --release

test:
	cargo test --all

run:
	cargo run

check:
	cargo check

fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings

clean:
	cargo clean
