default: build

all: test

# Check for required tools and Rust targets
preflight:
	@echo "Checking prerequisites..."
	@which stellar > /dev/null 2>&1 || (echo "❌ stellar CLI not found. Install with: cargo install stellar-cli" && exit 1)
	@echo "✓ stellar CLI found"
	@rustup target list --installed | grep -q wasm32-unknown-unknown || (echo "❌ wasm32-unknown-unknown target not installed. Install with: rustup target add wasm32-unknown-unknown" && exit 1)
	@echo "✓ wasm32-unknown-unknown target installed"
	@which cargo > /dev/null 2>&1 || (echo "❌ cargo not found. Install Rust from: https://rustup.rs" && exit 1)
	@echo "✓ cargo found"
	@echo "✅ All prerequisites met!"

test: build
	cargo test

build:
	stellar contract build
	# stellar contract build outputs to wasm32v1-none (Soroban's canonical target).
	# wasm32-unknown-unknown is only used for `cargo test` and clippy, not for WASM artifacts.
	@ls -l ../../target/wasm32v1-none/release/*.wasm

build-logs:
	cargo build --profile release-with-logs --target wasm32-unknown-unknown
	@ls -l ../../target/wasm32-unknown-unknown/release-with-logs/*.wasm

test-logs:
	cargo test --profile release-with-logs

fmt:
	cargo fmt --all

clean:
	cargo clean

# Deploy to localnet (set STELLAR_NETWORK=testnet for testnet)
deploy:
	bash $(CURDIR)/../../../scripts/deploy.sh

# Invoke a contract function. Usage: make invoke ARGS="balance alice"
invoke:
	bash $(CURDIR)/../../../scripts/invoke.sh $(ARGS)

# Run the full localnet integration test (requires a running localnet — see README.md)
integration-test:
	bash $(CURDIR)/../../../scripts/integration_test.sh
