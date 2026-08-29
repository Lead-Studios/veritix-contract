//! Shared storage model.
//! Defines instance-vs-persistent key ownership, value shapes, and TTL bump policy constants.

use soroban_sdk::{contracttype, Address, Env, IntoVal, TryFromVal, Val};

/// Target real-time lifetime: ~1 year (at least 365 days).
/// Calculation: 31,536,000 seconds / 5 seconds per ledger = 6,307,200 ledgers.
/// 6,310,000 provides just over 1 year of storage TTL to ensure users do not
/// lose their balances due to archival within a standard 1-year inactivity period.
pub const BALANCE_LIFETIME_THRESHOLD: u32 = 6_310_000;

/// The bump amount is set slightly higher than the threshold to avoid
/// continuous bumping on every transaction and save on fees.
pub const BALANCE_BUMP_AMOUNT: u32 = 6_400_000;
pub const ALLOWANCE_LIFETIME_THRESHOLD: u32 = 518400;
pub const ALLOWANCE_BUMP_AMOUNT: u32 = 535000;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 518400;
pub const INSTANCE_BUMP_AMOUNT: u32 = 535000;
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 518400;
pub const PERSISTENT_BUMP_AMOUNT: u32 = 535000;
pub const SPLIT_LIFETIME_THRESHOLD: u32 = PERSISTENT_LIFETIME_THRESHOLD;
pub const SPLIT_BUMP_AMOUNT: u32 = PERSISTENT_BUMP_AMOUNT;
pub const RECURRING_LIFETIME_THRESHOLD: u32 = PERSISTENT_LIFETIME_THRESHOLD;
pub const RECURRING_BUMP_AMOUNT: u32 = PERSISTENT_BUMP_AMOUNT;
pub const DISPUTE_LIFETIME_THRESHOLD: u32 = PERSISTENT_LIFETIME_THRESHOLD;
pub const DISPUTE_BUMP_AMOUNT: u32 = PERSISTENT_BUMP_AMOUNT;
pub const ESCROW_LIFETIME_THRESHOLD: u32 = 7_884_000;
pub const ESCROW_BUMP_AMOUNT: u32 = 7_900_000;
pub const ADMIN_ACTIVATION_DELAY: u32 = 17_280;
pub const WARNING_WINDOW: u32 = 1000;

#[derive(Clone)]
#[contracttype]
pub struct AllowanceDataKey { pub from: Address, pub spender: Address }

#[derive(Clone)]
#[contracttype]
pub struct AllowanceValue { pub amount: i128, pub expiration_ledger: u32 }

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Allowance(AllowanceDataKey),
    SpenderAllowances(Address),
    Balance(Address),
    Metadata,
    TotalSupply,
    MaxSupply,
    HolderSet,
    SnapshotCount,
    Snapshot(u32),
    EscrowCount,
    Escrow(u32),
    DepositorEscrows(Address),
    RecurringCount,
    Recurring(u32),
    PayerRecurrings(Address),
    RecipientSplits(Address),
    SplitCount,
    Split(u32),
    DisputeCount,
    Dispute(u32),
    ClaimantDisputes(Address),
    EscrowDispute(u32),
    EscrowDisputeHistory(u32),
    ResolverDisputes(Address),
    OpenDisputes,
    Freeze(Address),
    FrozenAccounts,
    OwnerAllowances(Address),
    Paused,
    ClawbackCoSigner,
    PendingAdmin,
    AdminActiveAfter,
    ExpiryWarned(u32),
    Nonce(Address),
    DistributedCount,
    CancelledCount,
    TotalDistributedValue,
}

pub fn read_persistent_record<T>(e: &Env, key: &DataKey, missing_message: &'static str) -> T
where T: TryFromVal<Env, Val> {
    let storage = e.storage().persistent();
    let value = storage.get::<DataKey, T>(key).unwrap_or_else(|| panic!("{}", missing_message));
    storage.extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    value
}

pub fn write_persistent_record<T>(e: &Env, key: &DataKey, value: &T)
where T: IntoVal<Env, Val> {
    let storage = e.storage().persistent();
    storage.set(key, value);
    storage.extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
}

pub fn bump_instance(e: &Env) {
    e.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

pub fn read_counter(e: &Env, key: &DataKey) -> u32 { e.storage().instance().get(key).unwrap_or(0) }

#[derive(Clone)]
#[contracttype]
pub struct EscrowStats {
    pub total_count: u32,
    pub active_count: u32,
    pub settled_count: u32,
    pub total_value_locked: i128,
}

pub fn increment_counter(e: &Env, key: &DataKey) -> u32 {
    bump_instance(e);
    let next = read_counter(e, key) + 1;
    e.storage().instance().set(key, &next);
    next
}
