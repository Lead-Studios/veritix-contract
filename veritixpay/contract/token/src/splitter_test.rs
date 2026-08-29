use soroban_sdk::{testutils::{Address as _, Events as _}, Address, Env, Symbol, TryFromVal, Vec};

use crate::balance::read_balance;
use crate::balance::read_total_supply;
use crate::contract::VeritixToken;
use crate::splitter::{
    cancel_split, create_split, distribute, get_split, get_splits_for_recipient,
    replace_split_recipient, splitter_stats, SplitRecipient,
};
use crate::storage_types::{read_counter, DataKey};

fn setup_env() -> Env {
    let e = Env::default();
    e.mock_all_auths();
    e
}

fn make_recipients(e: &Env, shares: &[(Address, u32)]) -> Vec<SplitRecipient> {
    let mut v = Vec::new(e);
    for (addr, bps) in shares {
        v.push_back(SplitRecipient {
            address: addr.clone(),
            share_bps: *bps,
        });
    }
    v
}

// Verifies that create_split stores a record with correct sender, amount, and
// initial state (not distributed, not cancelled).
#[test]
fn test_create_split_stores_record() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r2.clone(), 5000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        let record = get_split(&e, split_id);
        assert_eq!(record.sender, sender);
        assert_eq!(record.total_amount, 1000);
        assert!(!record.distributed);
        assert!(!record.cancelled);
    });
}

// Happy-path distribution: creates a 50/50 split between two recipients and
// verifies each receives exactly half the total amount.
#[test]
fn test_distribute_two_recipients_equal_split() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r2.clone(), 5000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        distribute(&e, sender.clone(), split_id);
        assert_eq!(read_balance(&e, r1.clone()), 500);
        assert_eq!(read_balance(&e, r2.clone()), 500);
        assert!(get_split(&e, split_id).distributed);
    });
}

// Verifies that cancelling a split refunds the full amount to the sender and
// marks the record as cancelled (not distributed). If this fails, senders
// cannot recover funds from abandoned splits.
#[test]
fn test_cancel_split_refunds_sender_and_marks_record() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);

        assert_eq!(read_balance(&e, sender.clone()), 0);

        cancel_split(&e, sender.clone(), split_id);

        let record = get_split(&e, split_id);
        assert!(record.cancelled);
        assert!(!record.distributed);
        assert_eq!(read_balance(&e, sender.clone()), 1000);
        assert_eq!(read_balance(&e, r1.clone()), 0);
    });
}

// Ensures that rounding dust from BPS-based division always goes to the last
// recipient. With 3 recipients splitting 10 units by 3333/3333/3334 bps,
// distribution must be 3 + 3 + 4 (not 3 + 3 + 3 with 1 lost).
#[test]
fn test_distribute_rounding_dust_goes_to_last_recipient() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);
    let r3 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 10);
        // 3333 + 3333 + 3334 = 10000 bps; 10 units → 3 + 3 + 4
        let recipients = make_recipients(
            &e,
            &[(r1.clone(), 3333), (r2.clone(), 3333), (r3.clone(), 3334)],
        );
        let split_id = create_split(&e, sender.clone(), recipients, 10);
        distribute(&e, sender.clone(), split_id);
        assert_eq!(read_balance(&e, r1.clone()), 3);
        assert_eq!(read_balance(&e, r2.clone()), 3);
        assert_eq!(read_balance(&e, r3.clone()), 4);
    });
}

// Ensures that a non-sender caller cannot trigger distribution — the auth
// check must reject unauthorized distribute calls.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "unauthorized")]
fn test_distribute_unauthorized_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let hacker = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        distribute(&e, hacker.clone(), split_id);
    });
}

// Ensures that distributing an already-distributed split panics — prevents
// double distribution that would drain the contract balance.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "already distributed")]
fn test_double_distribute_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        distribute(&e, sender.clone(), split_id);
        distribute(&e, sender.clone(), split_id);
    });
}

// Ensures that cancelling an already-distributed split panics — once funds
// are sent, the sender cannot claw them back via cancel.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "already distributed")]
fn test_cancel_after_distribute_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        distribute(&e, sender.clone(), split_id);
        cancel_split(&e, sender.clone(), split_id);
    });
}

// Ensures that distributing a cancelled split panics — funds have already
// been returned to the sender.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "split cancelled")]
fn test_distribute_after_cancel_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        cancel_split(&e, sender.clone(), split_id);
        distribute(&e, sender.clone(), split_id);
    });
}

// Ensures that creating a split with an empty recipient list is rejected —
// distribution to zero recipients is meaningless and wastes storage.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "recipients list cannot be empty")]
fn test_create_split_rejects_empty_recipients() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients: Vec<SplitRecipient> = Vec::new(&e);
        create_split(&e, sender.clone(), recipients, 1000);
    });
}

// Ensures that creating a split with a recipient that has 0 bps is rejected
// — zero-share recipients serve no purpose and can cause unexpected dust.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "recipient share_bps cannot be zero")]
fn test_create_split_rejects_zero_share_recipient() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000), (r2.clone(), 0)]);
        create_split(&e, sender.clone(), recipients, 1000);
    });
}

// Ensures that duplicate recipient addresses are rejected — prevents ambiguity
// about which duplicate receives the funds.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "duplicate recipient address")]
fn test_create_split_rejects_duplicate_recipients() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r1.clone(), 5000)]);
        create_split(&e, sender.clone(), recipients, 1000);
    });
}

// Ensures that a split with zero or negative total amount is rejected —
// escrows and splits must lock a positive amount.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "amount must be positive")]
fn test_create_split_rejects_non_positive_amount() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        create_split(&e, sender.clone(), recipients, 0);
    });
}

// Verifies the 20-recipient cap — creating a split with 21 recipients must be
// rejected to stay within Soroban's computational and ledger entry limits.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "TooManyRecipients: maximum 20 recipients allowed")]
fn test_create_split_too_many_recipients_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        // Build 21 recipients each with equal share — total bps won't matter since
        // the cap check fires first.
        let mut pairs: soroban_sdk::Vec<SplitRecipient> = soroban_sdk::Vec::new(&e);
        for _ in 0..21 {
            pairs.push_back(SplitRecipient {
                address: Address::generate(&e),
                share_bps: 476, // approximate; cap check fires before bps validation
            });
        }
        create_split(&e, sender.clone(), pairs, 1000);
    });
}

// Critical invariant: after split creation and distribution, the sum of all
// tracked balances must equal total supply — confirms tokens are not created
// or destroyed during the split lifecycle.
#[test]
fn test_split_create_and_distribute_preserves_supply() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);
    let amount = 1_000i128;

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), amount);
        crate::balance::increase_supply(&e, amount);

        let contract_addr = e.current_contract_address();
        let all = [sender.clone(), r1.clone(), r2.clone(), contract_addr.clone()];

        let assert_invariant = |addrs: &[Address]| {
            let sum = addrs.iter().fold(0i128, |s, a| s + read_balance(&e, a.clone()));
            assert_eq!(crate::balance::read_total_supply(&e), sum);
        };

        assert_invariant(&all);

        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r2.clone(), 5000)]);
        let split_id = create_split(&e, sender.clone(), recipients, amount);
        assert_invariant(&all);

        distribute(&e, sender.clone(), split_id);
        assert_invariant(&all);
    });
}

// --- Issue #165: cancel_split tests ---

// Ensures that only the sender can cancel a split — a third party hacker must
// be rejected with "unauthorized".
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "unauthorized")]
fn test_cancel_split_unauthorized_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let hacker = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        cancel_split(&e, hacker.clone(), split_id);
    });
}

// Verifies that cancelling a split preserves the supply invariant — funds
// return to sender and total supply is unchanged.
#[test]
fn test_cancel_preserves_supply_invariant() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        crate::balance::increase_supply(&e, 1000);

        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);

        // After create: sender=0, contract=1000, total=1000
        let contract_addr = e.current_contract_address();
        assert_eq!(
            read_balance(&e, sender.clone()) + read_balance(&e, contract_addr.clone()),
            read_total_supply(&e)
        );

        cancel_split(&e, sender.clone(), split_id);

        // After cancel: sender=1000, contract=0, total=1000
        assert_eq!(
            read_balance(&e, sender.clone()) + read_balance(&e, contract_addr.clone()),
            read_total_supply(&e)
        );
    });
}

// --- Issue #162: Event emission tests ---

// Verifies that distribute emits a single event with (split_distributed,
// split_id, sender) topics and total_amount as data.
#[test]
fn test_distribute_emits_event() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);

        // Clear create event (create_split emits no event currently — documented below)
        let _ = e.events().all();

        distribute(&e, sender.clone(), split_id);
    });

    let events = e.events().all();
    assert_eq!(events.len(), 1);
    // Topics: (split_distributed, split_id, sender), data: total_amount
    assert_eq!(events.first().unwrap().1.len(), 3);
}

// NOTE: create_split currently emits no event — this is a known gap (see issue #162).
// The distribute function emits split_distributed after funds are sent to recipients.

// Verifies that cancel_split emits a single event with (split_cancelled,
// split_id, caller) topics and total_amount as data.
#[test]
fn test_cancel_split_emits_event() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);

        let _ = e.events().all();

        cancel_split(&e, sender.clone(), split_id);
    });

    let events = e.events().all();
    assert_eq!(events.len(), 1);
    // Topics: (split_cancelled, split_id, caller), data: total_amount
    assert_eq!(events.first().unwrap().1.len(), 3);
}

// --- Split counter tests ---

// Ensures the split counter starts at zero before any splits are created.
#[test]
fn test_split_count_starts_at_zero() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);

    e.as_contract(&contract_id, || {
        let count = read_counter(&e, &DataKey::SplitCount);
        assert_eq!(count, 0);
    });
}

// Verifies the split counter increments correctly and split IDs are sequential
// (1, 2, 3) without gaps.
#[test]
fn test_split_count_increments_on_create() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 3000);

        // Before any splits
        assert_eq!(read_counter(&e, &DataKey::SplitCount), 0);

        // Create first split
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        assert_eq!(split_id, 1);
        assert_eq!(read_counter(&e, &DataKey::SplitCount), 1);

        // Create second split
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        assert_eq!(split_id, 2);
        assert_eq!(read_counter(&e, &DataKey::SplitCount), 2);

        // Create third split
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        assert_eq!(split_id, 3);
        assert_eq!(read_counter(&e, &DataKey::SplitCount), 3);
    });
}

// Verifies bulk_distribute distributes multiple splits in a single atomic call
// and marks them all as distributed with correct balances.
#[test]
fn test_bulk_distribute_distributes_all_splits() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 2000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let id1 = create_split(&e, sender.clone(), recipients.clone(), 1000);
        let id2 = create_split(&e, sender.clone(), recipients, 1000);
        let mut ids = soroban_sdk::Vec::new(&e);
        ids.push_back(id1);
        ids.push_back(id2);
        crate::splitter::bulk_distribute(&e, sender.clone(), ids);
        assert!(get_split(&e, id1).distributed);
        assert!(get_split(&e, id2).distributed);
        assert_eq!(read_balance(&e, r1.clone()), 2000);
    });
}

// Ensures bulk_distribute rejects more than 10 split IDs per batch (BulkLimit).
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "BulkLimit")]
fn test_bulk_distribute_rejects_more_than_ten_ids() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 11_000);
        let mut ids = soroban_sdk::Vec::new(&e);
        for _ in 0..11 {
            let recs = make_recipients(&e, &[(r1.clone(), 10000)]);
            ids.push_back(create_split(&e, sender.clone(), recs, 1000));
        }
        crate::splitter::bulk_distribute(&e, sender.clone(), ids);
    });
}

// Verifies that create_split_with_escrow returns one distinct escrow ID per
// recipient, and all IDs are unique.
#[test]
fn test_create_split_with_escrow_returns_one_id_per_recipient() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r2.clone(), 5000)]);
        let ids = crate::splitter::create_split_with_escrow(
            &e, sender.clone(), recipients, 1000, 1000,
        );
        assert_eq!(ids.len(), 2);
        // Both IDs must be distinct
        assert_ne!(ids.get(0).unwrap(), ids.get(1).unwrap());
    });
}

// Verifies that create_split_with_memo stores the memo on the split record
// for off-chain correlation (e.g., order reference).
#[test]
fn test_create_split_with_memo_stores_memo() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 500);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let memo = soroban_sdk::Bytes::from_slice(&e, b"order-ref-001");
        let split_id = crate::splitter::create_split_with_memo(
            &e,
            sender.clone(),
            recipients,
            500,
            memo.clone(),
        );
        let record = get_split(&e, split_id);
        assert_eq!(record.memo, memo);
    });
}

// Ensures that creating a split with recipient shares summing to exactly 10000 bps succeeds.
#[test]
fn test_create_split_total_bps_exactly_10000_ok() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 3000), (r2.clone(), 7000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        assert_eq!(split_id, 1);
    });
}

// Ensures that creating a split with recipient shares summing to less than 10000 bps panics.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidShares: recipient shares must sum to exactly 10000 bps")]
fn test_create_split_total_bps_less_than_10000_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r2.clone(), 4000)]);
        create_split(&e, sender.clone(), recipients, 1000);
    });
}

// --- Event content tests ---

// NOTE: create_split emits no event — intentional absence, distribute is the
// observable action that off-chain indexers track.

// Verifies that distribute emits ("splt_dist", split_id, sender) topics and
// total_amount as scalar data.
#[test]
fn test_distribute_event_topics_and_data() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let amount = 1_000i128;

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), amount);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, amount);
        // create_split emits no event, so before == 0
        distribute(&e, sender.clone(), split_id);

        let events = e.events().all();
        assert_eq!(events.len(), 1);
        let event = events.last().unwrap();
        let topics = event.1;
        assert_eq!(topics.len(), 3);
        let t0 = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
        assert_eq!(t0, soroban_sdk::symbol_short!("splt_dist"));
        let t1 = u32::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
        assert_eq!(t1, split_id);
        let t2 = Address::try_from_val(&e, &topics.get(2).unwrap()).unwrap();
        assert_eq!(t2, sender);
        let data_amount = i128::try_from_val(&e, &event.2).unwrap();
        assert_eq!(data_amount, amount);
    });
}

// Ensures that creating a split with recipient shares summing to more than 10000 bps panics.
#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidShares: recipient shares must sum to exactly 10000 bps")]
fn test_create_split_total_bps_more_than_10000_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r2.clone(), 5001)]);
        create_split(&e, sender.clone(), recipients, 1000);
    });
}

// Required test names per issue #425 specification.
#[test]
fn test_create_split_shares_sum_to_10000_ok() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r2.clone(), 5000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        assert_eq!(split_id, 1);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidShares: recipient shares must sum to exactly 10000 bps")]
fn test_create_split_shares_sum_to_9999_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r2.clone(), 4999)]);
        create_split(&e, sender.clone(), recipients, 1000);
    });
}

#[test]
#[cfg_attr(windows, ignore)]
    #[should_panic(expected = "InvalidShares: recipient shares must sum to exactly 10000 bps")]
fn test_create_split_shares_sum_to_10001_panics() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r2.clone(), 5001)]);
        create_split(&e, sender.clone(), recipients, 1000);
    });
}

#[test]
fn test_get_splits_for_recipient_returns_empty_for_unknown_recipient() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let recipient = Address::generate(&e);

    e.as_contract(&contract_id, || {
        let ids = get_splits_for_recipient(&e, recipient.clone());
        assert_eq!(ids.len(), 0);
    });
}

#[test]
fn test_create_split_indexes_ids_for_each_recipient() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);
    let r2 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 5000), (r2.clone(), 5000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);

        let r1_ids = get_splits_for_recipient(&e, r1.clone());
        let r2_ids = get_splits_for_recipient(&e, r2.clone());
        assert_eq!(r1_ids.len(), 1);
        assert_eq!(r2_ids.len(), 1);
        assert_eq!(r1_ids.get(0).unwrap(), split_id);
        assert_eq!(r2_ids.get(0).unwrap(), split_id);
    });
}

#[test]
fn test_get_splits_for_recipient_returns_all_matching_split_ids() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let recipient = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 2000);
        let split_one = create_split(
            &e,
            sender.clone(),
            make_recipients(&e, &[(recipient.clone(), 10000)]),
            1000,
        );
        let split_two = create_split(
            &e,
            sender.clone(),
            make_recipients(&e, &[(recipient.clone(), 10000)]),
            1000,
        );

        let ids = get_splits_for_recipient(&e, recipient.clone());
        assert_eq!(ids.len(), 2);
        assert_eq!(ids.get(0).unwrap(), split_one);
        assert_eq!(ids.get(1).unwrap(), split_two);
    });
}

#[test]
fn test_replace_split_recipient_updates_recipient_indexes() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let old_recipient = Address::generate(&e);
    let new_recipient = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let split_id = create_split(
            &e,
            sender.clone(),
            make_recipients(&e, &[(old_recipient.clone(), 10000)]),
            1000,
        );

        replace_split_recipient(
            &e,
            sender.clone(),
            split_id,
            old_recipient.clone(),
            new_recipient.clone(),
        );

        let old_ids = get_splits_for_recipient(&e, old_recipient.clone());
        let new_ids = get_splits_for_recipient(&e, new_recipient.clone());
        assert_eq!(old_ids.len(), 0);
        assert_eq!(new_ids.len(), 1);
        assert_eq!(new_ids.get(0).unwrap(), split_id);
    });
}

// --- Issue #448: SplitterStats tests ---

// Verifies that splitter_stats returns all zeros before any splits are created.
#[test]
fn test_splitter_stats_initial_zeros() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);

    e.as_contract(&contract_id, || {
        let stats = splitter_stats(&e);
        assert_eq!(stats.total_splits, 0);
        assert_eq!(stats.distributed_count, 0);
        assert_eq!(stats.cancelled_count, 0);
        assert_eq!(stats.total_distributed_value, 0);
    });
}

// Verifies that total_splits increments on create but distributed_count and
// cancelled_count only increment on the respective terminal actions.
#[test]
fn test_splitter_stats_total_splits_increments_on_create() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 2000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        create_split(&e, sender.clone(), recipients.clone(), 1000);
        let stats = splitter_stats(&e);
        assert_eq!(stats.total_splits, 1);
        assert_eq!(stats.distributed_count, 0);
        assert_eq!(stats.cancelled_count, 0);

        create_split(&e, sender.clone(), recipients, 1000);
        let stats = splitter_stats(&e);
        assert_eq!(stats.total_splits, 2);
    });
}

// Verifies that distributed_count and total_distributed_value increment after distribute().
#[test]
fn test_splitter_stats_distributed_count_increments() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        distribute(&e, sender.clone(), split_id);

        let stats = splitter_stats(&e);
        assert_eq!(stats.distributed_count, 1);
        assert_eq!(stats.total_distributed_value, 1000);
        assert_eq!(stats.cancelled_count, 0);
    });
}

// Verifies that cancelled_count increments after cancel_split() and
// total_distributed_value is unaffected.
#[test]
fn test_splitter_stats_cancelled_count_increments() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 1000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);
        let split_id = create_split(&e, sender.clone(), recipients, 1000);
        cancel_split(&e, sender.clone(), split_id);

        let stats = splitter_stats(&e);
        assert_eq!(stats.cancelled_count, 1);
        assert_eq!(stats.distributed_count, 0);
        assert_eq!(stats.total_distributed_value, 0);
    });
}

// Verifies total_distributed_value accumulates across multiple distributions.
#[test]
fn test_splitter_stats_total_distributed_value_accumulates() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 3000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);

        let id1 = create_split(&e, sender.clone(), recipients.clone(), 1000);
        let id2 = create_split(&e, sender.clone(), recipients.clone(), 2000);
        distribute(&e, sender.clone(), id1);
        distribute(&e, sender.clone(), id2);

        let stats = splitter_stats(&e);
        assert_eq!(stats.distributed_count, 2);
        assert_eq!(stats.total_distributed_value, 3000);
    });
}

// Verifies mixed lifecycle: one distributed + one cancelled yields correct stats.
#[test]
fn test_splitter_stats_mixed_lifecycle() {
    let e = setup_env();
    let contract_id = e.register_contract(None, VeritixToken);
    let sender = Address::generate(&e);
    let r1 = Address::generate(&e);

    e.as_contract(&contract_id, || {
        crate::balance::receive_balance(&e, sender.clone(), 2000);
        let recipients = make_recipients(&e, &[(r1.clone(), 10000)]);

        let id1 = create_split(&e, sender.clone(), recipients.clone(), 1000);
        let id2 = create_split(&e, sender.clone(), recipients, 1000);
        distribute(&e, sender.clone(), id1);
        cancel_split(&e, sender.clone(), id2);

        let stats = splitter_stats(&e);
        assert_eq!(stats.total_splits, 2);
        assert_eq!(stats.distributed_count, 1);
        assert_eq!(stats.cancelled_count, 1);
        assert_eq!(stats.total_distributed_value, 1000);
    });
}
