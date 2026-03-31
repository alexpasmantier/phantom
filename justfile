default:
    just --list

# Build the project
build:
    cargo build --workspace

# Run integration tests with live monitor (tmux popup)
test: build
    ./tests/run.sh --monitor

# Run integration tests without monitor
test-headless: build
    ./tests/run.sh

# Run cargo unit/integration tests
test-cargo: build
    cargo test -p phantom-daemon -- --test-threads=1

# Run the Rust integration test suite (headless)
test-rust: build
    cargo run -p phantom-test --features monitor --example runner

# Run the Rust integration test suite with live monitor
test-rust-monitor: build
    cargo run -p phantom-test --features monitor --example runner -- --monitor

# Run all tests (cargo + integration)
test-all: test-cargo test-headless

# Check the project
check:
    cargo check --workspace

# Format
format:
    cargo fmt --all

# Lint
lint:
    cargo clippy --all-targets -- -D warnings

# Fix + format + lint
fix:
    cargo fix --allow-dirty --allow-staged
    just format
    just lint
