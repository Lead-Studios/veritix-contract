# Veritix Pay Contract
The core Soroban smart contract for the Veritix Pay ticketing platform, handling token primitives, escrow, splitting, and recurring payments.

## Architecture Overview



The contract is designed with a layered architecture to separate concerns and ensure secure token operations. At the foundational level, `storage_types.rs` acts as the shared foundation, defining the core data keys and structures used across all files. Building upon this, `admin.rs`, `metadata.rs`, `balance.rs`, and `allowance.rs` serve as the token primitives, handling raw ledger updates and basic authorization. The `contract.rs` module acts as the public interface that ties these primitives together, exposing the standard Soroban token functions to the outside world. Finally, complex business logic is encapsulated in payment feature modules like `escrow.rs`, `recurring.rs`, `splitter.rs`, and `dispute.rs`, which sit on top of the primitives to execute real-world ticketing workflows.

## Storage Model



| DataKey Variant | Storage Type | Description |
| :--- | :--- | :--- |
| `Admin` | Instance | Stores the `Address` of the contract administrator. |
| `Metadata` | Instance | Stores token details (name, symbol, decimals). |
| `Balance(Address)` | Persistent | Stores the `i128` token balance of an address. |
| `Allowance(AllowanceDataKey)` | Persistent | Stores the `i128` approved spend limit between two addresses. |
| `EscrowCount` | Instance | Tracks the total number of standard escrows created. |
| `Escrow(u32)` | Persistent | Stores an `EscrowRecord` containing lockup details and status. |
| `MultiEscrowCount` | Instance | Tracks the total number of multi-recipient escrows. |
| `MultiEscrow(u32)` | Persistent | Stores a `MultiEscrowRecord` for proportional payouts. |
| `RecurringCount` | Instance | Tracks the total number of recurring payment setups. |
| `Recurring(u32)` | Persistent | Stores a `RecurringRecord` for subscription states. |
| `SplitCount` | Instance | Tracks the total number of payment splits. |
| `Split(u32)` | Persistent | Stores a `SplitRecord` with basis points for each recipient. |
| `DisputeCount` | Instance | Tracks the total number of opened disputes. |
| `Dispute(u32)` | Persistent | Stores a `DisputeRecord` containing adjudication status. |
| `Freeze(Address)` | Persistent | Stores a `bool` indicating if an account is blocked. |

## Module Reference

| File | Purpose | Key Public Functions |
| :--- | :--- | :--- |
| `admin.rs` | Administrator management | `check_admin`, `transfer_admin` |
| `allowance.rs` | Third-party spending approvals | `read_allowance`, `write_allowance` |
| `balance.rs` | Ledger updates and math | `read_balance`, `receive_balance`, `spend_balance` |
| `contract.rs` | Main entry point / Soroban interface | `transfer`, `mint`, `clawback`, `freeze` |
| `dispute.rs` | Escrow adjudication | `open_dispute`, `resolve_dispute` |
| `escrow.rs` | Time-locked & conditional payments | `create_escrow`, `release_escrow`, `create_multi_escrow` |
| `freeze.rs` | Regulatory compliance blocking | `freeze_account`, `unfreeze_account`, `is_frozen` |
| `metadata.rs` | Token identity storage | `read_name`, `read_symbol`, `read_decimal` |
| `recurring.rs` | Subscription logic via ledger intervals | `setup_recurring`, `execute_recurring` |
| `splitter.rs` | Proportional revenue sharing | `create_split`, `distribute` |
| `storage_types.rs` | Enums and structs for state | *None (Data Definitions)* |

## How to Build and Test
Use the standard makefile commands to interact with the contract:
* `make build`: Compiles the Rust code into a WebAssembly (`.wasm`) binary optimized for the Soroban environment.
* `make test`: Runs the entire comprehensive unit testing suite across all modules to ensure logic and panic states execute correctly.
* `make clean`: Removes the `target/` directory and compiled binaries.

## Authorization Model
Security is enforced natively using the Soroban SDK. Every state-changing function requires the caller to authorize the transaction, invoked via `address.require_auth()`. Administrative functions rely on `check_admin(&e)`, which verifies the caller against the stored `DataKey::Admin` address. To prevent state archiving, storage TTL (Time To Live) is bumped automatically during `read_balance` and `read_allowance` calls, ensuring active accounts remain on the ledger.

Allowance expiration is ledger-based: an allowance with `expiration_ledger == current_ledger` remains valid for the current ledger and expires once the ledger sequence advances past that value.

## Adding a New Module
1. Define any new data structures or `DataKey` variants in `storage_types.rs`.
2. Create your logic file (e.g., `new_feature.rs`) and implement the core functions.
3. Expose the necessary interface methods in `contract.rs`.
4. Register the module by adding `pub mod new_feature;` to `lib.rs`.
5. Create a corresponding `new_feature_test.rs` file to ensure 100% test coverage.

## Links
* [Back to Root README](../README.md)
* [Contributing Guidelines](../CONTRIBUTING.md)
