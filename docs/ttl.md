# TTL policy

Soroban entries expire. An entry that is never touched eventually falls below the network's minimum TTL and is archived, after which reading it returns nothing - the contract cannot tell "never existed" from "expired".

Two helpers in `src/storage_types.rs` implement the policy:

| Helper | Applies to | Threshold | Extends to |
| --- | --- | --- | --- |
| `bump_instance(e)` | Instance storage | `INSTANCE_LIFETIME_THRESHOLD` (518,400 ledgers, ~30 days) | `INSTANCE_BUMP_AMOUNT` (2,592,000 ledgers, ~150 days) |
| `bump_persistent(e, &key)` | One persistent entry | `PERSISTENT_LIFETIME_THRESHOLD` (2,073,600 ledgers, ~120 days) | `PERSISTENT_BUMP_AMOUNT` (4,752,000 ledgers, ~275 days) |

Ledger figures assume a 5-second ledger close.

## Where state lives

The contract-wide configuration keys — `Admin`, `InitializedAtLedger`, `Paused`,
`WhitelistEnabled`, `TotalSupply` and `MaxSupply` — live in **instance**
storage. Everything that grows with usage — balances, allowances, escrows,
schedules, disputes — is **persistent**.

## Current coverage

Every entry point begins with `bump_instance(e)`, so the instance TTL is
extended on every call. `bump_persistent` is applied by the dispute module.
Other modules do not yet call the helper, so their entries are not extended on
access. New code should call `bump_persistent` on every read and write of a
persistent key.

## Adding a key

1. Add the variant to `DataKey` in `src/storage_types.rs`.
2. Use persistent storage if the number of entries grows with usage.
3. Call `bump_persistent` on every read and write of the key.
4. Document it in `docs/storage-layout.md`.
