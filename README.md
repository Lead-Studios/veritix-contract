# Veritix Pay

![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)
![Built With Rust](https://img.shields.io/badge/Built%20With-Rust-orange.svg)
![Network: Stellar / Soroban](https://img.shields.io/badge/Network-Stellar%20%2F%20Soroban-7b5ea7.svg)
![Status: In Development](https://img.shields.io/badge/Status-In%20Development-yellow.svg)

On-chain payment infrastructure for the Veritix ticketing platform, built with Rust and Soroban on the Stellar network.

---

## Overview

Veritix Pay is the payment layer of a blockchain-based ticketing system. It lives entirely on-chain as a Soroban smart contract and handles all financial operations that power the Veritix platform — from a fan buying a ticket to an organizer receiving settlement funds after an event.

The contract handles **token transfers**, **escrow for ticket purchases**, **recurring payments**, **payment splitting between organizers, artists, and venues**, and **dispute resolution** when something goes wrong. Each of these is designed as a focused, composable module that shares a common storage layout and authorization model.

Escrowed funds are held temporarily on the contract's own ledger balance. When an escrow is created, tokens move from the depositor into the contract address, stay there while the escrow is unresolved, and move back out only on release or refund. This means the contract address can hold a real token balance during escrow flows without increasing total supply.

This project is actively being built in the open. Core token primitives, escrow logic, recurring payments, splitting, and dispute resolution are implemented and tested. Some draft modules exist in `src/` but are not yet wired into the crate. Contributors can claim open GitHub Issues to extend and improve existing modules. See the [Contributing](#contributing) section below.

---

## Why Stellar & Soroban

- **Deterministic execution and predictable fees** — no gas spikes or unpredictable costs
- **Fast finality** — Stellar's 5-second finality is suitable for real-time ticket validation and payment confirmation
- **Rust-based safety guarantees** — Soroban contracts are written in Rust, giving strong compile-time correctness checks
- **Native Stellar ecosystem integration** — works directly with Stellar assets and accounts, no bridges required

---

## Contract Modules

The entry point is `veritixpay/contract/token/src/lib.rs`. Modules compiled into the crate are listed below.

| Module | File | Status | Description |
|--------|------|--------|-------------|
| Token Core | `contract.rs` | Compiled | Mint, burn, transfer, approve, clawback, freeze |
| Admin | `admin.rs` | Compiled | Admin address controls and rotation |
| Allowance | `allowance.rs` | Compiled | Third-party spending approvals |
| Balance | `balance.rs` | Compiled | Ledger balance reads/writes |
| Escrow | `escrow.rs` | Compiled | Create, release, and refund escrow holds |
| Freeze | `freeze.rs` | Compiled | Regulatory account blocking |
| Metadata | `metadata.rs` | Compiled | Token name, symbol, decimals |
| Storage Types | `storage_types.rs` | Compiled | Shared `DataKey` enum and struct definitions |
| Recurring Payments | `recurring.rs` | Draft | Set up and execute recurring charges (not yet exposed in `contract.rs`) |
| Payment Splitter | `splitter.rs` | Draft | Split a payment between multiple parties (not yet exposed in `contract.rs`) |
| Dispute Resolution | `dispute.rs` | Draft | Open and resolve payment disputes (not yet exposed in `contract.rs`) |

---

## Getting Started

### Prerequisites

| Tool | Notes | Install |
|------|-------|---------|
| Rust (stable) | Required to compile Soroban contracts | https://rustup.rs |
| wasm32 target | Required build target | `rustup target add wasm32-unknown-unknown` |
| Stellar CLI (latest) | For building and deploying | `cargo install stellar-cli` |

Verify your setup:

```bash
rustc --version
stellar --version
```

### Clone and Build

```bash
git clone https://github.com/Lead-Studios/veritix-contract.git
cd veritix-contract/veritixpay/contract/token

make preflight  # verify all required tools are installed
make build    # compile to WASM (requires stellar CLI)
make test     # run tests
make fmt      # format code
make clean    # remove build artifacts
```

> **Note:** Run `make preflight` first to verify your setup. It checks for stellar CLI, Rust, and the wasm32-unknown-unknown target.

---

### Localnet Integration Test

The integration test deploys the compiled WASM to a local Soroban node and runs the full ticket purchase flow end-to-end: initialize → mint → create_escrow → release_escrow → assert balance.

**Prerequisites**

| Tool | Notes | Install |
|------|-------|---------|
| Docker | Required to run the Stellar quickstart image | https://docs.docker.com/get-docker |
| stellar CLI | Already required for building | `cargo install stellar-cli` |

**Start a local Soroban node**

```bash
docker run --rm -it \
  -p 8000:8000 \
  --name stellar \
  stellar/quickstart:latest \
  --standalone \
  --enable-soroban-rpc
```

Wait until you see `Soroban RPC running` in the logs (usually ~10 seconds).

**Run the integration test**

```bash
cd veritixpay/contract/token
make integration-test
```

The script will build the contract, deploy it to localnet, run the full flow, and print `✅ Integration test passed` on success.

**Environment overrides**

| Variable | Default | Description |
|----------|---------|-------------|
| `STELLAR_NETWORK` | `standalone` | Network name passed to stellar CLI |
| `STELLAR_RPC_URL` | `http://localhost:8000/soroban/rpc` | Soroban RPC endpoint |
| `STELLAR_NETWORK_PASSPHRASE` | `Standalone Network ; February 2017` | Network passphrase |

```bash
# Example: run against testnet
STELLAR_NETWORK=testnet \
STELLAR_RPC_URL=https://soroban-testnet.stellar.org \
STELLAR_NETWORK_PASSPHRASE="Test SDF Network ; September 2015" \
make integration-test
```

---

## Project Structure

```
veritixpay/
├── contract/
│   └── token/
│       ├── src/
│       │   ├── lib.rs              # Crate entry point — module declarations
│       │   ├── contract.rs         # Public Soroban interface (VeritixToken)
│       │   ├── admin.rs            # Admin storage and rotation
│       │   ├── allowance.rs        # Spending approvals
│       │   ├── balance.rs          # Ledger balance helpers
│       │   ├── escrow.rs           # Escrow logic
│       │   ├── freeze.rs           # Account freeze/unfreeze
│       │   ├── metadata.rs         # Token metadata
│       │   ├── storage_types.rs    # Shared DataKey enum and structs
│       │   ├── test.rs             # Compiled unit tests
│       │   ├── escrow_test.rs      # Escrow-specific tests
│       │   └── admin_test.rs       # Admin rotation tests
│       ├── Cargo.toml
│       └── Makefile
├── Cargo.toml
├── Cargo.lock
├── .gitignore
└── README.md
```

---

## Contributing

Contributions are welcome and actively encouraged. This project is structured so that each contract module is an independent issue that any contributor can pick up.

To get started:
1. Browse [open issues](https://github.com/Lead-Studios/veritix-contract/issues)
2. Comment on one to get assigned
3. Branch from `main`, build your module, write tests, and open a PR

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full guide — including project structure, how to add a module, storage conventions, authorization rules, and the PR checklist.

---

## Open Source Wave

This project is part of an active open-source funding wave on [Drips Network](https://www.drips.network/). Contributors who build meaningful features may be eligible for rewards. Build something real, on a real chain, with real incentives.

---

## Related Repositories

- **Backend:** https://github.com/Lead-Studios/veritix-backend
- **Web client:** https://github.com/Lead-Studios/veritix-web

---

## License

MIT
