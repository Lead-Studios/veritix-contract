//! Dispute statistics and claimant indexing.

use crate::storage_types::{DataKey, DisputeRecord, DisputeStatus};
use soroban_sdk::{Address, Env, Vec};

/// Indexes a newly raised dispute under its claimant and in the global dispute list.
pub fn index_dispute(e: &Env, claimant: &Address, escrow_id: u64) {
    let mut claimant_disputes: Vec<u64> = e
        .storage()
        .persistent()
        .get(&DataKey::ClaimantDisputes(claimant.clone()))
        .unwrap_or_else(|| Vec::new(e));
    claimant_disputes.push_back(escrow_id);
    e.storage()
        .persistent()
        .set(&DataKey::ClaimantDisputes(claimant.clone()), &claimant_disputes);

    let mut all_disputes: Vec<u64> = e
        .storage()
        .persistent()
        .get(&DataKey::DisputeIds)
        .unwrap_or_else(|| Vec::new(e));
    all_disputes.push_back(escrow_id);
    e.storage()
        .persistent()
        .set(&DataKey::DisputeIds, &all_disputes);
}

/// Returns `(open_count, resolved_count, expired_count)` across all disputes.
pub fn dispute_stats(e: &Env) -> (u32, u32, u32) {
    let all_disputes: Vec<u64> = e
        .storage()
        .persistent()
        .get(&DataKey::DisputeIds)
        .unwrap_or_else(|| Vec::new(e));

    let mut open = 0u32;
    let mut resolved = 0u32;
    let mut expired = 0u32;

    for id in all_disputes.iter() {
        if let Some(record) =
            e.storage().persistent().get::<_, DisputeRecord>(&DataKey::Dispute(id))
        {
            match record.status {
                DisputeStatus::Open | DisputeStatus::Appealed => open += 1,
                DisputeStatus::Resolved => resolved += 1,
                DisputeStatus::Expired => expired += 1,
            }
        }
    }

    (open, resolved, expired)
}

/// Returns all dispute records raised by `claimant`.
pub fn get_disputes_by_claimant(e: &Env, claimant: &Address) -> Vec<DisputeRecord> {
    let ids: Vec<u64> = e
        .storage()
        .persistent()
        .get(&DataKey::ClaimantDisputes(claimant.clone()))
        .unwrap_or_else(|| Vec::new(e));

    let mut out = Vec::new(e);
    for id in ids.iter() {
        if let Some(record) =
            e.storage().persistent().get::<_, DisputeRecord>(&DataKey::Dispute(id))
        {
            out.push_back(record);
        }
    }
    out
}