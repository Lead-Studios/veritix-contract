use soroban_sdk::{contracttype, Address};

/// Every persistent storage key the contract owns.
///
/// Keeping the key set in one enum means a storage layout question only ever
/// has one answer, and the layout can be dumped as a whole when auditing.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Address holding admin authority.
    Admin,
    /// The administrator nominated by a two-step ownership transfer, if any.
    PendingAdmin,
    /// The ledger from which `PendingAdmin` becomes the active `Admin`.
    AdminActiveAfterLedger,
    /// The second signer required alongside the admin for a clawback.
    ClawbackCosigner,

    // --- Balances ------------------------------------------------------------
    /// Token balance of a single account.
    Balance(Address),
    /// Token balance key used by the balance module.
    BalanceOf(Address),
    /// Spending allowance granted by `owner` to `spender`.
    Allowance(Address, Address),
    /// The portion of an account's balance held by an active escrow.
    EscrowLocked(Address),

    // --- Compliance ----------------------------------------------------------
    /// Whether the whole contract is paused.
    Paused,
    /// Whether token holders must be whitelisted.
    WhitelistEnabled,
    /// Whether an account is frozen and cannot move tokens.
    Frozen(Address),

    // --- Supply --------------------------------------------------------------
    /// Total tokens in circulation.
    TotalSupply,
    /// Hard supply cap. Unset or 0 means supply is uncapped.
    MaxSupply,

    // --- Escrows and disputes ------------------------------------------------
    /// An escrow held under its id, for lifecycle and settlement tracking.
    Escrow(u64),
    /// A payment split held under its id.
    Split(u64),
    /// The dispute record open over an escrow, keyed by escrow id.
    Dispute(u64),
    /// An authorised recurring payment, keyed by recurring payment id.
    Recurring(u64),

    // --- Counters ------------------------------------------------------------
    /// A monotonically increasing per-address counter.
    Counter(Address),
}

/// Where an escrow is in its lifecycle.
///
/// `Active` is the only state an escrow can be created in; the other two are
/// terminal and mutually exclusive, and neither ever returns to `Active`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    /// Funds are still held and the escrow can still be settled.
    Active,
    /// Settled in the beneficiary's favour.
    Released,
    /// Settled in the depositor's favour.
    Refunded,
}

/// One escrow: who deposited, who is owed, and under what deadline.
///
/// `amount` is the amount still held, not the amount originally deposited. A
/// partial settlement reduces it, and an escrow is settled only once it reaches
/// zero, so a record never has to be read alongside the event log to tell how
/// much is still owed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EscrowRecord {
    pub depositor: Address,
    pub beneficiary: Address,
    pub token: Address,
    pub amount: i128,
    pub deadline_ledger: u32,
    pub status: EscrowStatus,
    /// Ledger at which the escrow was created, for age and expiry checks.
    pub created_ledger: u32,
}

/// A payment split: one sender, many recipients, shares in basis points.
///
/// `shares_bps` is parallel to `recipients`: `shares_bps[i]` is the share owed
/// to `recipients[i]`, and the shares must total 10_000.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitRecord {
    /// Who created the split and receives the surplus on cancellation.
    pub sender: Address,
    /// The token being distributed.
    pub token: Address,
    /// The full amount the split covers.
    pub total_amount: i128,
    /// Who is to be paid, in share order.
    pub recipients: Vec<Address>,
    /// Basis-point share per recipient, parallel to `recipients`. Totals 10_000.
    pub shares_bps: Vec<u32>,
    /// Whether distribution has already happened. Guards against paying twice.
    pub distributed: bool,
}

/// A recurring charge authorised between a payer and a payee.
///
/// `next_execution` is the ledger at which the next charge becomes due;
/// `interval_ledgers` is the gap between successive charges.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringPayment {
    /// Who is charged.
    pub payer: Address,
    /// Who receives the charge.
    pub payee: Address,
    /// The token charged.
    pub token: Address,
    /// The amount charged on each execution, in the token's base unit.
    pub amount: i128,
    /// Ledgers between successive charges. Must be greater than zero.
    pub interval_ledgers: u32,
    /// Ledger at which the next charge becomes due.
    pub next_execution: u32,
    /// Whether the schedule is still authorised. Cleared on cancellation.
    pub active: bool,
    /// Whether the schedule is temporarily suspended. Survives resume.
    pub paused: bool,
}

/// Lifecycle state of a dispute over an escrow.
///
/// The order is the progression: `Open` may become `Appealed`, and `Appealed`
/// may settle into `Resolved`. `Expired` is the alternative terminal state for
/// a dispute nobody acted on.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    /// Raised and awaiting a ruling. The escrow stays frozen.
    Open,
    /// A ruling was handed down and the escrow settled. Terminal.
    Resolved,
    /// The losing party escalated after a ruling. The escrow stays frozen.
    Appealed,
    /// Nobody acted before the deadline, so the dispute was closed out.
    /// Terminal.
    Expired,
}

/// A dispute raised over a single escrow.
///
/// While a dispute is `Open` or `Appealed` the underlying escrow must not be
/// released or refunded; that is the whole point of raising one. A ruling
/// recorded by `resolve_dispute` moves the dispute to `Resolved` with the
/// payout blocked until the appeal window lapses; if the losing party appeals,
/// the dispute becomes `Appealed` and only `resolve_appeal` (a final, unappealable
/// ruling) may settle the escrow.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRecord {
    /// Who raised the dispute.
    pub claimant: Address,
    /// The escrow this dispute is over.
    pub escrow_id: u64,
    /// Current lifecycle state.
    pub status: DisputeStatus,
    /// Ledger at which the dispute was raised, for expiry and age checks.
    pub opened_ledger: u32,
    /// The arbiter assigned to settle it, if one has been set.
    pub resolver: Option<Address>,
    /// Ledger at which the most recent ruling was handed down; `0` if no
    /// ruling has been recorded yet. Measured against the appeal window.
    pub resolved_ledger: u32,
    /// The party declared the winner by the most recent ruling.
    pub winner: Option<Address>,
    /// True once an appeal has been resolved. The ruling is then final and a
    /// second appeal on the same escrow must be rejected.
    pub is_final: bool,
}
