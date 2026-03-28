use crate::escrow::{get_escrow, release_escrow, refund_escrow};
use crate::storage_types::{increment_counter, DataKey};
use soroban_sdk::{contracttype, Address, Env, Symbol};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    Open,
    ResolvedForBeneficiary,
    ResolvedForDepositor,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeRecord {
    pub id: u32,
    pub escrow_id: u32,
    pub claimant: Address,
    pub resolver: Address,
    pub status: DisputeStatus,
}

/// Opens a dispute against an existing escrow.
pub fn open_dispute(
    e: &Env,
    claimant: Address,
    escrow_id: u32,
    resolver: Address,
) -> u32 {
    // 1. Authorization: Only the claimant can initiate this call
    claimant.require_auth();

    // 2. Fetch escrow and validate current state
    let escrow = get_escrow(e, escrow_id);
    
    // Check if the escrow is already finalized
    if escrow.released || escrow.refunded {
        panic!("InvalidState: Cannot open dispute on a settled escrow");
    }

    // 3. Authorization check: Claimant must be a party involved in the escrow
    if claimant != escrow.depositor && claimant != escrow.beneficiary {
        panic!("Unauthorized: Only depositor or beneficiary can open a dispute");
    }

    // 4. Generate a new Dispute ID using the counter in storage
    let count = increment_counter(e, &DataKey::DisputeCount);

    // 5. Create and store the dispute record
    let record = DisputeRecord {
        id: count,
        escrow_id,
        claimant: claimant.clone(),
        resolver,
        status: DisputeStatus::Open,
    };
    
    // Store in persistent storage as disputes may last longer than instance TTL
    e.storage().persistent().set(&DataKey::Dispute(count), &record);

    // 6. Emit Observability Event
    e.events().publish(
        (Symbol::new(e, "dispute"), Symbol::new(e, "opened"), escrow_id),
        claimant
    );

    count
}

/// Resolves an open dispute.
pub fn resolve_dispute(
    e: &Env,
    resolver: Address,
    dispute_id: u32,
    release_to_beneficiary: bool,
) {
    // 1. Authorization: Only the designated resolver can resolve the dispute
    resolver.require_auth();

    // 2. Fetch the dispute record
    let mut dispute: DisputeRecord = e
        .storage()
        .persistent()
        .get(&DataKey::Dispute(dispute_id))
        .expect("Dispute not found");

    // 3. Validation: Check if already resolved (Double-resolution panic)
    if dispute.status != DisputeStatus::Open {
        panic!("AlreadyResolved: This dispute has already been resolved");
    }

    // 4. Validation: Verify the resolver matches the record
    if dispute.resolver != resolver {
        panic!("UnauthorizedResolver: Only the designated resolver can resolve this");
    }

    // 5. Execute resolution by calling the core escrow logic
    if release_to_beneficiary {
        // Triggers the standard release logic from escrow.rs
        release_escrow(e, dispute.escrow_id);
        dispute.status = DisputeStatus::ResolvedForBeneficiary;
    } else {
        // Triggers the standard refund logic from escrow.rs
        refund_escrow(e, dispute.escrow_id);
        dispute.status = DisputeStatus::ResolvedForDepositor;
    }

    // 6. Persist the updated dispute status
    e.storage().persistent().set(&DataKey::Dispute(dispute_id), &dispute);

    // 7. Emit Observability Event
    e.events().publish(
        (Symbol::new(e, "dispute"), Symbol::new(e, "resolved"), dispute_id),
        release_to_beneficiary
    );
}

/// Helper to read a dispute record
pub fn get_dispute(e: &Env, dispute_id: u32) -> DisputeRecord {
    e.storage()
        .persistent()
        .get(&DataKey::Dispute(dispute_id))
        .expect("Dispute not found")
}
