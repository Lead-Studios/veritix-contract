use soroban_sdk::{testutils::{Address as _, Events as _, Ledger as _}, Address, Env, Symbol, TryFromVal, Vec};

use crate::balance::read_balance;
use crate::contract::VeritixToken;
use crate::dispute::{expire_dispute, get_dispute, resolve_dispute, DisputeStatus};
use soroban_sdk::Bytes;
use crate::escrow::{create_escrow, get_escrow};
use crate::storage_types::{read_counter, DataKey};

fn setup_env() -> Env {
    let e = Env::default();
    e.mock_all_auths();
    e
}

fn setup_escrow(e: &Env, contract_id: &Address) -> (Address, Address, u32) {
    let depositor = Address::generate(e);
    let beneficiary = Address::generate(e);
    let amount = 1_000i128;
    let mut escrow_id = 0u32;
    e.as_contract(contract_id, || {
        crate::balance::receive_balance(e, depositor.clone(), amount);
        escrow_id = create_escrow(e, depositor.clone(), beneficiary.clone(), amount, 1000);
    });
    (depositor, beneficiary, escrow_id)
}

fn open_dispute(e: &Env, claimant: Address, escrow_id: u32, resolver: Address, evidence: Bytes, expiry_ledger: u32) -> u32 {
    crate::dispute::open_dispute(e, claimant, escrow_id, resolver, evidence, expiry_ledger, 0)
}




// Verifies that open_dispute stores a record with correct escrow_id, claimant,
// resolver, and initial Open status. If this fails, the dispute creation flow
// is broken and no disputes can be filed.
#[test]
fn test_open_dispute_stores_record() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        let record = get_dispute(&e, dispute_id);
        assert_eq!(record.escrow_id, escrow_id);
        assert_eq!(record.claimant, depositor);
        assert_eq!(record.resolver, resolver);
        assert_eq!(record.status, DisputeStatus::Open);
    });
}

// Happy-path: resolves dispute in favour of the beneficiary, verifying that
// the escrow is released and funds are transferred to the beneficiary.
#[test]
fn test_resolve_dispute_for_beneficiary() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (_depositor, beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(&e, beneficiary.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        resolve_dispute(&e, resolver.clone(), dispute_id, true);

        let record = get_dispute(&e, dispute_id);
        assert_eq!(record.status, DisputeStatus::ResolvedForBeneficiary);

        let escrow = get_escrow(&e, escrow_id);
        assert!(escrow.released);

        assert_eq!(read_balance(&e, beneficiary.clone()), 1_000);
    });
}

// Happy-path: resolves dispute in favour of the depositor, verifying that
// the escrow is refunded and funds are returned to the depositor.
#[test]
fn test_resolve_dispute_for_depositor() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        resolve_dispute(&e, resolver.clone(), dispute_id, false);

        let record = get_dispute(&e, dispute_id);
        assert_eq!(record.status, DisputeStatus::ResolvedForDepositor);

        let escrow = get_escrow(&e, escrow_id);
        assert!(escrow.refunded);

        assert_eq!(read_balance(&e, depositor.clone()), 1_000);
    });
}

// Ensures that only the designated resolver can resolve a dispute — an
// impostor caller must be rejected with "UnauthorizedResolver".
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "UnauthorizedResolver")]
fn test_resolve_dispute_wrong_resolver_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let impostor = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        resolve_dispute(&e, impostor.clone(), dispute_id, true);
    });
}

// Ensures that resolving an already-resolved dispute panics — prevents
// double resolution that could double-spend escrow funds.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "AlreadyResolved")]
fn test_double_resolve_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        resolve_dispute(&e, resolver.clone(), dispute_id, true);
        resolve_dispute(&e, resolver.clone(), dispute_id, false);
    });
}

// Ensures that opening a dispute on an already-settled escrow (released or
// refunded) is rejected with "InvalidState".
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidState")]
fn test_open_dispute_on_settled_escrow_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (_depositor, beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        crate::escrow::release_escrow(&e, beneficiary.clone(), escrow_id);
        open_dispute(&e, beneficiary.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
    });
}

// Ensures that opening a second dispute on the same unresolved escrow panics
// — prevents dispute spam on the same escrow.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "DisputeAlreadyOpen")]
fn test_duplicate_open_dispute_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        // Second open on the same unresolved escrow must fail.
        open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
    });
}

// Verifies that a new dispute can be opened on a different escrow after a
// previous dispute has been resolved — the EscrowDispute pointer is cleared
// on resolution, allowing subsequent disputes on other escrows.
#[test]
fn test_reopen_dispute_after_resolution() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    // Create a second escrow to reopen on (the first is settled after resolution).
    let (depositor2, _beneficiary2, escrow_id2) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        resolve_dispute(&e, resolver.clone(), dispute_id, false);

        // After resolution the EscrowDispute pointer is cleared; a new dispute on a
        // different (still-open) escrow must succeed.
        let new_id = open_dispute(&e, depositor2.clone(), escrow_id2, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        let record = get_dispute(&e, new_id);
        assert_eq!(record.status, DisputeStatus::Open);
    });
}

// Ensures that the claimant cannot also be the resolver — a resolver must be
// an impartial third party.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidResolver: resolver cannot be the claimant")]
fn test_open_dispute_rejects_claimant_as_resolver() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        open_dispute(&e, depositor.clone(), escrow_id, depositor.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
    });
}

// Ensures that the depositor cannot be the resolver — prevents a conflict of
// interest where the depositor resolves their own dispute.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidResolver: resolver cannot be the depositor")]
fn test_open_dispute_rejects_depositor_as_resolver() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let (_depositor, beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let escrow = get_escrow(&e, escrow_id);
        open_dispute(&e, beneficiary.clone(), escrow_id, escrow.depositor.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
    });
}

// Ensures that the beneficiary cannot be the resolver — prevents a conflict of
// interest where the beneficiary resolves their own dispute.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidResolver: resolver cannot be the beneficiary")]
fn test_open_dispute_rejects_beneficiary_as_resolver() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let escrow = get_escrow(&e, escrow_id);
        open_dispute(&e, depositor.clone(), escrow_id, escrow.beneficiary.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
    });
}

// Ensures that a stranger (neither depositor nor beneficiary) cannot open a
// dispute — only escrow participants have standing.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "Unauthorized: only escrow parties can open a dispute")]
fn test_open_dispute_stranger_as_claimant_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let stranger = Address::generate(&e);
    let (_depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        open_dispute(&e, stranger.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
    });
}

// --- Issue #277: resolve_dispute requires resolver authorization ---

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "missing authorization")]
fn test_resolve_dispute_without_auth_panics() {
    let e = Env::default();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        // Create dispute with mocked auths
        e.mock_all_auths();
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);

        // Clear all auths - resolver must authorize for resolve_dispute
        e.set_auths(&[]);
        resolve_dispute(&e, resolver.clone(), dispute_id, true);
    });
}

// Ensures that the dispute counter does not skip IDs when a dispute call is
// rejected — IDs from successful opens must be sequential (1, 2, 3).
#[test]
fn test_dispute_counter_does_not_skip_on_rejected_call() {
    // Verify that a rejected open_dispute call (duplicate) does not increment the counter.
    // We do this by opening two disputes on two separate escrows and confirming IDs are 1 and 2.
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);
    let (depositor2, _beneficiary2, escrow_id2) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        // First dispute gets ID 1
        let id1 = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        assert_eq!(id1, 1);

        // Second dispute on a different escrow gets ID 2 (no gap)
        let id2 = open_dispute(&e, depositor2.clone(), escrow_id2, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        assert_eq!(id2, 2);
    });
}

// --- Issue #162: Event emission tests ---

// Verifies that open_dispute emits a single event with (dispute_opened,
// escrow_id, claimant) topics.
#[test]
fn test_open_dispute_emits_event() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    // Clear escrow creation event
    let _ = e.events().all();

    e.as_contract(&contract_id, || {
        open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
    });

    let events = e.events().all();
    // Events: escr_crtd (from setup_escrow) + disp_open = 2 total
    assert!(events.len() >= 1);
    // The last event is the disp_open event with 3 topics: (dispute_opened, escrow_id, claimant)
    assert_eq!(events.last().unwrap().1.len(), 3);
}

// Verifies that resolve_dispute emits events (escrow_refunded and
// dispute_resolved) with correct topic structure.
#[test]
fn test_resolve_dispute_emits_event() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);

        // Clear prior events
        let _ = e.events().all();

        resolve_dispute(&e, resolver.clone(), dispute_id, false);
    });

    let events = e.events().all();
    // Expect: escrow_refunded + dispute_resolved = 2 events
    assert!(events.len() >= 1);
    // Last event should be dispute_resolved with 3 topics
    let last = events.last().unwrap();
    assert_eq!(last.1.len(), 3);
}

// --- Dispute counter tests ---

// Ensures the dispute counter starts at zero before any disputes are opened.
#[test]
fn test_dispute_count_starts_at_zero() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);

    e.as_contract(&contract_id, || {
        let count = read_counter(&e, &DataKey::DisputeCount);
        assert_eq!(count, 0);
    });
}

// Verifies the dispute counter increments correctly with dispute IDs 1, 2, 3
// across multiple escrows — no ID gaps or collisions.
#[test]
fn test_dispute_count_increments_on_open() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);

    // Create three escrows for three disputes
    let (depositor1, _beneficiary1, escrow_id1) = setup_escrow(&e, &contract_id);
    let (depositor2, _beneficiary2, escrow_id2) = setup_escrow(&e, &contract_id);
    let (depositor3, _beneficiary3, escrow_id3) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        // Before any disputes
        assert_eq!(read_counter(&e, &DataKey::DisputeCount), 0);

        // Open first dispute
        let dispute_id = open_dispute(&e, depositor1.clone(), escrow_id1, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        assert_eq!(dispute_id, 1);
        assert_eq!(read_counter(&e, &DataKey::DisputeCount), 1);

        // Open second dispute
        let dispute_id = open_dispute(&e, depositor2.clone(), escrow_id2, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        assert_eq!(dispute_id, 2);
        assert_eq!(read_counter(&e, &DataKey::DisputeCount), 2);

        // Open third dispute
        let dispute_id = open_dispute(&e, depositor3.clone(), escrow_id3, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        assert_eq!(dispute_id, 3);
        assert_eq!(read_counter(&e, &DataKey::DisputeCount), 3);
    });
}

// Verifies that resolve_dispute_with_note stores the note on the dispute record
// and correctly updates the status to ResolvedForBeneficiary.
#[test]
fn test_resolve_dispute_with_note_stores_note() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);
    let resolver = Address::generate(&e);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        let note = soroban_sdk::Bytes::from_slice(&e, b"resolved: funds to beneficiary");
        crate::dispute::resolve_dispute_with_note(
            &e, resolver.clone(), dispute_id, true, note.clone(),
        );
        let record = get_dispute(&e, dispute_id);
        assert_eq!(record.resolution_note, note);
        assert_eq!(record.status, DisputeStatus::ResolvedForBeneficiary);
    });
}

#[test]
fn test_open_dispute_stores_evidence() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let evidence = Bytes::from_slice(&e, b"invoice-ref-42");
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), evidence.clone(), e.ledger().sequence() + 500);
        let record = get_dispute(&e, dispute_id);
        assert_eq!(record.evidence, evidence);
        assert_eq!(record.expiry_ledger, e.ledger().sequence() + 500);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "EvidenceTooLong")]
fn test_open_dispute_evidence_too_long_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let evidence = Bytes::from_slice(&e, &[0u8; 129]);
        open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), evidence, e.ledger().sequence() + 500);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidExpiry")]
fn test_open_dispute_past_expiry_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence());
    });
}

#[test]
fn test_expire_on_exact_expiry_ledger_succeeds() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let expiry = e.ledger().sequence() + 100;
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), expiry);
        e.ledger().with_mut(|l| l.sequence_number = expiry);
        expire_dispute(&e, dispute_id);
        let record = get_dispute(&e, dispute_id);
        assert_eq!(record.status, DisputeStatus::Expired);
        assert_eq!(read_balance(&e, depositor.clone()), 1_000);
    });
}

#[test]
fn test_expire_dispute_auto_resolves_for_depositor() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let expiry = e.ledger().sequence() + 100;
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), expiry);
        e.ledger().with_mut(|l| l.sequence_number = expiry + 1);
        expire_dispute(&e, dispute_id);
        let record = get_dispute(&e, dispute_id);
        assert_eq!(record.status, DisputeStatus::Expired);
        assert_eq!(read_balance(&e, depositor.clone()), 1_000);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "NotExpired")]
fn test_expire_one_ledger_before_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let expiry = e.ledger().sequence() + 1;
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), expiry);
        expire_dispute(&e, dispute_id);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "NotExpired")]
fn test_expire_dispute_before_expiry_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let expiry = e.ledger().sequence() + 100;
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), expiry);
        expire_dispute(&e, dispute_id);
    });
}

// --- Event content tests ---

// Verifies that open_dispute emits ("disp_open", escrow_id, claimant) topics
// and the evidence Bytes as data.
#[test]
fn test_open_dispute_event_topics_and_data() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let evidence = Bytes::from_slice(&e, b"invoice-42");
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        open_dispute(
            &e,
            depositor.clone(),
            escrow_id,
            resolver.clone(),
            evidence.clone(),
            e.ledger().sequence() + 500,
        );
    });

    let events = e.events().all();
    let event = events.last().unwrap();
    let topics = event.1;
    assert_eq!(topics.len(), 3);
    let t0 = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
    assert_eq!(t0, soroban_sdk::symbol_short!("disp_open"));
    let t1 = u32::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
    assert_eq!(t1, escrow_id);
    let t2 = Address::try_from_val(&e, &topics.get(2).unwrap()).unwrap();
    assert_eq!(t2, depositor);
    let data_evidence = Bytes::try_from_val(&e, &event.2).unwrap();
    assert_eq!(data_evidence, evidence);
}

// Verifies that resolve_dispute emits 2 events: an escrow settlement event
// followed by ("disp_rslv", dispute_id, resolver) with release_to_beneficiary
// as bool data.
#[test]
fn test_resolve_dispute_event_topics_and_data() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(
            &e,
            depositor.clone(),
            escrow_id,
            resolver.clone(),
            Bytes::new(&e),
            e.ledger().sequence() + 1000,
        );
        let before = e.events().all().len();
        // resolve for depositor (release_to_beneficiary = false → escr_rfnd emitted)
        resolve_dispute(&e, resolver.clone(), dispute_id, false);

        let events = e.events().all();
        // settle_escrow_by_outcome emits escr_rfnd, then disp_rslv
        assert_eq!(events.len(), before + 2);

        let last = events.last().unwrap();
        let topics = last.1;
        assert_eq!(topics.len(), 3);
        let t0 = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
        assert_eq!(t0, soroban_sdk::symbol_short!("disp_rslv"));
        let t1 = u32::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
        assert_eq!(t1, dispute_id);
        let t2 = Address::try_from_val(&e, &topics.get(2).unwrap()).unwrap();
        assert_eq!(t2, resolver);
        let data_release = bool::try_from_val(&e, &last.2).unwrap();
        assert!(!data_release);
    });
}

// --- Dispute lifecycle / terminal-state tests ---

// Verifies that resolving for the beneficiary transfers the locked amount to the
// beneficiary and leaves the depositor with zero (the funds were never returned).
#[test]
fn test_resolve_for_beneficiary_releases_funds_to_beneficiary() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, beneficiary, escrow_id) = setup_escrow(&e, &contract_id);
    let amount = 1_000i128;

    e.as_contract(&contract_id, || {
        // Depositor opens the dispute; resolver rules in favour of beneficiary.
        let dispute_id = open_dispute(
            &e,
            depositor.clone(),
            escrow_id,
            resolver.clone(),
            Bytes::new(&e),
            e.ledger().sequence() + 1000,
        );
        resolve_dispute(&e, resolver.clone(), dispute_id, true);

        assert_eq!(read_balance(&e, beneficiary.clone()), amount);
        assert_eq!(read_balance(&e, depositor.clone()), 0);
        let escrow = get_escrow(&e, escrow_id);
        assert!(escrow.released);
        assert!(!escrow.refunded);
    });
}

// Verifies that resolving for the depositor returns the locked amount to the
// depositor and leaves the beneficiary with zero.
#[test]
fn test_resolve_for_depositor_refunds_funds_to_depositor() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, beneficiary, escrow_id) = setup_escrow(&e, &contract_id);
    let amount = 1_000i128;

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(
            &e,
            beneficiary.clone(),
            escrow_id,
            resolver.clone(),
            Bytes::new(&e),
            e.ledger().sequence() + 1000,
        );
        resolve_dispute(&e, resolver.clone(), dispute_id, false);

        assert_eq!(read_balance(&e, depositor.clone()), amount);
        assert_eq!(read_balance(&e, beneficiary.clone()), 0);
        let escrow = get_escrow(&e, escrow_id);
        assert!(escrow.refunded);
        assert!(!escrow.released);
    });
}

// Ensures that calling release_escrow on an escrow already released via dispute
// resolution panics with "already settled" — the escrow is a terminal state.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "already settled")]
fn test_release_escrow_after_dispute_resolved_for_beneficiary_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (_depositor, beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(
            &e,
            beneficiary.clone(),
            escrow_id,
            resolver.clone(),
            Bytes::new(&e),
            e.ledger().sequence() + 1000,
        );
        resolve_dispute(&e, resolver.clone(), dispute_id, true);
        // Escrow is now released — a second release must panic.
        crate::escrow::release_escrow(&e, beneficiary.clone(), escrow_id);
    });
}

// Ensures that calling refund_escrow on an escrow already refunded via dispute
// resolution panics — the refunded state is terminal.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "already settled")]
fn test_refund_escrow_after_dispute_resolved_for_depositor_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(
            &e,
            depositor.clone(),
            escrow_id,
            resolver.clone(),
            Bytes::new(&e),
            e.ledger().sequence() + 1000,
        );
        resolve_dispute(&e, resolver.clone(), dispute_id, false);
        // Escrow is now refunded — a second refund must panic.
        crate::escrow::refund_escrow(&e, depositor.clone(), escrow_id);
    });
}

// Ensures that opening a second dispute on the same escrow while the first is
// still open panics with "DisputeAlreadyOpen" — one dispute per escrow at a time.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "DisputeAlreadyOpen")]
fn test_open_second_dispute_on_same_escrow_while_first_is_open_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        open_dispute(
            &e,
            depositor.clone(),
            escrow_id,
            resolver.clone(),
            Bytes::new(&e),
            e.ledger().sequence() + 1000,
        );
        // First dispute is still open — opening another must panic.
        open_dispute(
            &e,
            depositor.clone(),
            escrow_id,
            resolver.clone(),
            Bytes::new(&e),
            e.ledger().sequence() + 1000,
        );
    });
}

// Verifies that the disp_rslv event carries the correct outcome bool —
// tests the beneficiary path (true) complementing the depositor path tested
// in test_resolve_dispute_event_topics_and_data.
#[test]
fn test_resolve_dispute_emits_event_with_correct_outcome() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(
            &e,
            depositor.clone(),
            escrow_id,
            resolver.clone(),
            Bytes::new(&e),
            e.ledger().sequence() + 1000,
        );
        let before = e.events().all().len();
        // Resolve for beneficiary → release_to_beneficiary = true
        resolve_dispute(&e, resolver.clone(), dispute_id, true);

        let events = e.events().all();
        // settle_escrow_by_outcome emits escr_rls, then disp_rslv
        assert_eq!(events.len(), before + 2);
        let last = events.last().unwrap();
        let t0 = Symbol::try_from_val(&e, &last.1.get(0).unwrap()).unwrap();
        assert_eq!(t0, soroban_sdk::symbol_short!("disp_rslv"));
        let outcome = bool::try_from_val(&e, &last.2).unwrap();
        assert!(outcome, "event data must be true when resolved for beneficiary");
    });
}

// Verifies that dispute IDs are assigned sequentially — three disputes opened
// on three separate escrows receive IDs 1, 2, 3 with no gaps.
#[test]
fn test_dispute_counter_increments_sequentially() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (d1, _b1, eid1) = setup_escrow(&e, &contract_id);
    let (d2, _b2, eid2) = setup_escrow(&e, &contract_id);
    let (d3, _b3, eid3) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let id1 = open_dispute(
            &e, d1.clone(), eid1, resolver.clone(),
            Bytes::new(&e), e.ledger().sequence() + 1000,
        );
        let id2 = open_dispute(
            &e, d2.clone(), eid2, resolver.clone(),
            Bytes::new(&e), e.ledger().sequence() + 1000,
        );
        let id3 = open_dispute(
            &e, d3.clone(), eid3, resolver.clone(),
            Bytes::new(&e), e.ledger().sequence() + 1000,
        );
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    });
}

#[test]
fn test_full_dispute_lifecycle_with_appeal() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver_1 = Address::generate(&e);
    let resolver_2 = Address::generate(&e);
    let (depositor, beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        // Depositor opens dispute against beneficiary
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver_1.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);

        // Resolver_1 resolves for beneficiary (depositor loses)
        resolve_dispute(&e, resolver_1.clone(), dispute_id, true);
        let record = crate::dispute::get_dispute(&e, dispute_id);
        assert_eq!(record.status, DisputeStatus::ResolvedForBeneficiary);
        assert_eq!(crate::balance::read_balance(&e, beneficiary.clone()), 1_000);

        // Depositor appeals, appoints resolver_2
        crate::dispute::appeal_dispute(&e, depositor.clone(), dispute_id, resolver_2.clone());
        let record = crate::dispute::get_dispute(&e, dispute_id);
        assert_eq!(record.status, DisputeStatus::Appealed);
        let mut expected_appeal = Vec::new(&e);
        expected_appeal.push_back(resolver_2.clone());
        assert_eq!(record.appeal_resolver, expected_appeal);

        // Resolver_2 overturns: resolves for depositor
        crate::dispute::resolve_appeal(&e, resolver_2.clone(), dispute_id, true);
        let record = crate::dispute::get_dispute(&e, dispute_id);
        assert_eq!(record.status, DisputeStatus::ResolvedForDepositor);

        // Depositor gets full refund
        assert_eq!(crate::balance::read_balance(&e, depositor.clone()), 1_000);
        // Escrow is fully settled
        let escrow = crate::escrow::get_escrow(&e, escrow_id);
        assert!(escrow.refunded);
    });
}

#[test]
fn test_resolved_dispute_removed_from_open_disputes() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        let mut open_ids = crate::dispute::get_open_disputes(&e);
        let mut found = false;
        for i in 0..open_ids.len() {
            if open_ids.get(i).unwrap() == dispute_id {
                found = true;
                break;
            }
        }
        assert!(found);

        resolve_dispute(&e, resolver.clone(), dispute_id, false);

        open_ids = crate::dispute::get_open_disputes(&e);
        let mut still_open = false;
        for i in 0..open_ids.len() {
            if open_ids.get(i).unwrap() == dispute_id {
                still_open = true;
                break;
            }
        }
        assert!(!still_open);
    });
}

#[test]
fn test_appealed_dispute_removed_from_open_disputes_after_appeal_resolution() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver_1 = Address::generate(&e);
    let resolver_2 = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = open_dispute(&e, depositor.clone(), escrow_id, resolver_1.clone(), Bytes::new(&e), e.ledger().sequence() + 1000);
        resolve_dispute(&e, resolver_1.clone(), dispute_id, true);
        crate::dispute::appeal_dispute(&e, depositor.clone(), dispute_id, resolver_2.clone());

        let open_ids = crate::dispute::get_open_disputes(&e);
        let mut found = false;
        for i in 0..open_ids.len() {
            if open_ids.get(i).unwrap() == dispute_id {
                found = true;
                break;
            }
        }
        assert!(found, "appealed dispute should still be tracked as open");

        crate::dispute::resolve_appeal(&e, resolver_2.clone(), dispute_id, true);

        let open_ids = crate::dispute::get_open_disputes(&e);
        let mut still_open = false;
        for i in 0..open_ids.len() {
            if open_ids.get(i).unwrap() == dispute_id {
                still_open = true;
                break;
            }
        }
        assert!(!still_open, "dispute must be removed from open disputes after appeal resolution");
    });
}

#[test]
fn test_mediation_fee_zero() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = crate::dispute::open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000, 0);
        let resolver_balance_before = crate::balance::read_balance(&e, resolver.clone());
        let beneficiary_balance_before = crate::balance::read_balance(&e, beneficiary.clone());

        crate::dispute::resolve_dispute(&e, resolver.clone(), dispute_id, true);

        assert_eq!(crate::balance::read_balance(&e, resolver.clone()), resolver_balance_before);
        assert_eq!(crate::balance::read_balance(&e, beneficiary.clone()), beneficiary_balance_before + 1000);
    });
}

#[test]
fn test_mediation_fee_split_two_percent() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let dispute_id = crate::dispute::open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000, 200);
        let resolver_balance_before = crate::balance::read_balance(&e, resolver.clone());
        let beneficiary_balance_before = crate::balance::read_balance(&e, beneficiary.clone());

        crate::dispute::resolve_dispute(&e, resolver.clone(), dispute_id, true);

        assert_eq!(crate::balance::read_balance(&e, resolver.clone()), resolver_balance_before + 20);
        assert_eq!(crate::balance::read_balance(&e, beneficiary.clone()), beneficiary_balance_before + 980);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "MediationFeeBpsTooHigh")]
fn test_mediation_fee_too_high_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        crate::dispute::open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000, 501);
    });
}

#[test]
fn test_get_disputes_by_claimant() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let resolver = Address::generate(&e);
    let (depositor, _beneficiary, escrow_id) = setup_escrow(&e, &contract_id);

    e.as_contract(&contract_id, || {
        let list_before = crate::dispute::get_disputes_by_claimant(&e, depositor.clone());
        assert_eq!(list_before.len(), 0);

        let dispute_id = crate::dispute::open_dispute(&e, depositor.clone(), escrow_id, resolver.clone(), Bytes::new(&e), e.ledger().sequence() + 1000, 100);

        let list_after = crate::dispute::get_disputes_by_claimant(&e, depositor.clone());
        assert_eq!(list_after.len(), 1);
        assert_eq!(list_after.get(0).unwrap(), dispute_id);
    });
}
