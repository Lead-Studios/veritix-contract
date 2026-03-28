use soroban_sdk::{contracttype, Address, Env, IntoVal, TryFromVal, Val};

pub const BALANCE_LIFETIME_THRESHOLD: u32 = 518400; // ~30 days
pub const BALANCE_BUMP_AMOUNT: u32 = 535000;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 518400;
pub const INSTANCE_BUMP_AMOUNT: u32 = 535000;

#[derive(Clone)]
#[contracttype]
pub struct AllowanceDataKey {
    pub from: Address,
    pub spender: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct AllowanceValue {
    pub amount: i128,
    pub expiration_ledger: u32,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Allowance(AllowanceDataKey),
    Balance(Address),
    Metadata,
    TotalSupply,
    EscrowCount,
    Escrow(u32),
    RecurringCount,
    Recurring(u32),
    SplitCount,
    Split(u32),
    DisputeCount,
    Dispute(u32),

    // --- Added for Multi-Escrow (Issue #36) ---
    MultiEscrowCount,
    MultiEscrow(u32),

    // --- Added for Freeze Functionality (Issue #35) ---
    Freeze(Address),
}

pub fn read_persistent_record<T>(e: &Env, key: &DataKey, missing_message: &'static str) -> T
where
    T: TryFromVal<Env, Val>,
{
    e.storage()
        .persistent()
        .get::<DataKey, T>(key)
        .unwrap_or_else(|| panic!("{}", missing_message))
}

pub fn write_persistent_record<T>(e: &Env, key: &DataKey, value: &T)
where
    T: IntoVal<Env, Val>,
{
    e.storage().persistent().set(key, value);
}
