# Storage layout

Every value the contract persists is reached through a single
`DataKey` enum in `src/storage_types.rs`. There is exactly one such enum in the
crate, by design: a previous implementation carried two side by side and they
disagreed about where an account's balance lived.

If you need a new key, **extend `DataKey`**. Do not declare another enum, and
do not hand-build a storage key from a tuple or a string.

## Durability tiers

| Tier           | Bounded by                                                                       | Holds                                                                                                 |
| -------------- | -------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| **Instance**   | The contract's storage footprint, restored wholesale if the contract is archived | Fixed configuration: who the admin is, total supply, the supply cap                                   |
| **Persistent** | Grows with usage; each entry archived independently once its TTL lapses          | Per-account and per-record data: balances, allowances, counters, escrows, splits, schedules, disputes |

Instance storage is cheap to read and is restored as a unit, so it is reserved
for a small fixed set of configuration values. Anything whose count grows with
the number of users belongs in persistent storage.

## Key reference

| `DataKey` variant             | Value type    | Durability | Owning module | Notes                                                                               |
| ----------------------------- | ------------- | ---------- | ------------- | ----------------------------------------------------------------------------------- |
| `Admin`                       | `Address`     | Instance   | `admin`       | Set once at initialization. Panics on re-initialization.                            |
| `PendingAdmin`                | `Address`     | Instance   | `admin`       | Nominee during a two-step ownership transfer. Absent when no transfer is in flight. |
| `AdminActiveAfterLedger`      | `u32`         | Instance   | `admin`       | Ledger from which `PendingAdmin` becomes `Admin`.                                   |
| `ClawbackCosigner`            | `Address`     | Instance   | `admin`       | Second signer a clawback requires alongside the admin.                              |
| `TotalSupply`                 | `i128`        | Instance   | `balance`     | Tokens in circulation, net of burns.                                                |
| `MaxSupply`                   | `i128`        | Instance   | `balance`     | Hard cap fixed at initialization. Absent when the token is uncapped.                |
| `Balance(Address)`            | `i128`        | Persistent | `balance`     | Token balance of one account.                                                       |
| `Allowance(Address, Address)` | `(i128, u32)` | Persistent | `allowance`   | Amount approved by the first address for the second, and the ledger it expires at.  |
| `Counter(Address)`            | `u32`         | Persistent | `counter`     | Monotonic per-address counter.                                                      |

Record types that are stored under keys added by later issues:

| Record                      | Key                     | Value type                | Durability | Owning module |
| --------------------------- | ----------------------- | ------------------------- | ---------- | ------------- |
| `EscrowRecord`              | `Escrow(u64)`           | `EscrowRecord`            | Persistent | `escrow`      |
| `SplitRecord`               | `Split(u64)`            | `SplitRecord`             | Persistent | `splitter`    |
| `RecurringPayment`          | `Recurring(u64)`        | `RecurringPayment`        | Persistent | `recurring`   |
| Recurring execution history | `RecurringHistory(u64)` | `Vec<RecurringExecution>` | Persistent | `recurring`   |
| `DisputeRecord`             | `Dispute(u64)`          | `DisputeRecord`           | Persistent | `dispute`     |

Recurring execution history retains at most 100 charges per schedule. When a
new successful charge is recorded at capacity, the oldest entry is discarded.

The `Escrow`, `Split`, `Recurring`, and `Dispute` key variants are added by the
issues that introduce those modules. They are listed here so the intended
durability is agreed in advance: all four are per-record, unbounded in count,
and therefore persistent.

## TTL policy

Soroban entries expire. An entry that is never touched eventually falls below
the network's minimum TTL and is archived, after which reading it returns
nothing — the contract cannot tell "never existed" from "expired", and a
balance that reads as zero is indistinguishable from a drained account.

Two helpers in `src/storage_types.rs` implement the policy. Both are the only
supported way to extend an entry's life.

| Helper                     | Applies to           | Threshold                                                      | Extends by                                              |
| -------------------------- | -------------------- | -------------------------------------------------------------- | ------------------------------------------------------- |
| `bump_instance(e)`         | Instance storage     | `INSTANCE_LIFETIME_THRESHOLD` (518,400 ledgers, ~30 days)      | `INSTANCE_BUMP_AMOUNT` (2,592,000 ledgers, ~15 days)    |
| `bump_persistent(e, &key)` | One persistent entry | `PERSISTENT_LIFETIME_THRESHOLD` (2,073,600 ledgers, ~120 days) | `PERSISTENT_BUMP_AMOUNT` (4,752,000 ledgers, ~180 days) |

Ledger figures assume Stellar's 5-second ledger close.

### When to call them

- **`bump_instance`** at the top of any entry point that reads or writes
  instance storage — in practice every administrative function, since the
  admin key and the supply counters live there. An archived instance is more
  serious than an archived balance: the contract does not exist at all until
  it is restored.
- **`bump_persistent`** on **every read and every write** of a persistent key,
  not only on writes.

That second point is the one most easily got wrong. Bumping only on writes
looks sufficient until you consider a long-dormant escrow: nothing writes to it
between creation and release, so at release time the entry is already archived
and the contract reads a missing record. A read that returns a live value is
exactly the case where the owner still cares, which is why reads bump too.

## Adding a key

1. Add the variant to `DataKey` in `src/storage_types.rs`, grouped by concern.
2. Decide instance or persistent. If the number of entries grows with usage,
   it is persistent.
3. Update the key reference table above.
4. Use `bump_persistent` on every read and write of the new key, or
   `bump_instance` if it is instance storage.
5. If the key moves or renames an existing value, that is an upgrade-guide
   concern, not a feature — see `docs/upgrade-guide.md`.
