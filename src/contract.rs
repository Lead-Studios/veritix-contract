use crate::metadata::{self, TokenMetadata};
use crate::storage_types::{DataKey, EscrowRecord, RecurringExecution};
use crate::{admin, balance, escrow, recurring, whitelist};
use soroban_sdk::{contract, contractimpl, Address, Env, String, Vec};

#[contract]
pub struct VeriTixPay;

/// Stores the admin and the token metadata, refusing a second call.
fn initialize_state(e: &Env, admin_addr: &Address, meta: &TokenMetadata) {
    if admin::is_initialized(e) {
        panic!("AlreadyInitialized: contract state is locked");
    }
    admin_addr.require_auth();
    e.storage().persistent().set(&DataKey::Admin, admin_addr);
    e.storage()
        .persistent()
        .set(&DataKey::InitializedAtLedger, &e.ledger().sequence());
    metadata::store(e, meta);
}

#[contractimpl]
impl VeriTixPay {
    /// Sets the admin and the default token metadata.
    ///
    /// One-shot: a second call is rejected rather than silently replacing the
    /// admin, because the admin is the root of trust for every privileged entry
    /// point below.
    pub fn initialize(e: Env, admin_addr: Address) {
        initialize_state(&e, &admin_addr, &metadata::defaults(&e));
    }

    /// Sets the admin and caller-supplied token metadata.
    pub fn initialize_with_metadata(
        e: Env,
        admin_addr: Address,
        name: String,
        symbol: String,
        decimals: u32,
    ) {
        initialize_state(
            &e,
            &admin_addr,
            &TokenMetadata {
                name,
                symbol,
                decimals,
            },
        );
    }

    /// True once an admin has been stored.
    pub fn is_initialized(e: Env) -> bool {
        admin::is_initialized(&e)
    }

    /// The address currently holding admin authority.
    pub fn admin(e: Env) -> Address {
        admin::admin(&e)
    }

    /// Ledger on which the contract was initialized, 0 when it never was.
    pub fn initialized_at_ledger(e: Env) -> u32 {
        admin::initialized_at_ledger(&e)
    }

    /// Enables whitelist enforcement. Admin-only.
    pub fn enable_whitelist(e: Env, admin_addr: Address) {
        whitelist::enable_whitelist(&e, &admin_addr);
    }

    /// Disables whitelist enforcement. Admin-only.
    pub fn disable_whitelist(e: Env, admin_addr: Address) {
        whitelist::disable_whitelist(&e, &admin_addr);
    }

    /// Whether whitelist enforcement is enabled.
    pub fn is_whitelist_enabled(e: Env) -> bool {
        whitelist::is_whitelist_enabled(&e)
    }

    /// The token name.
    pub fn name(e: Env) -> String {
        metadata::load(&e).name
    }

    /// The token symbol.
    pub fn symbol(e: Env) -> String {
        metadata::load(&e).symbol
    }

    /// The number of decimals the token is scaled by.
    pub fn decimals(e: Env) -> u32 {
        metadata::load(&e).decimals
    }

    /// Tokens held by `account`; 0 for an address that has never been credited.
    pub fn balance(e: Env, account: Address) -> i128 {
        balance::balance_of(&e, &account)
    }

    /// Tokens in circulation.
    pub fn total_supply(e: Env) -> i128 {
        balance::total_supply(&e)
    }

    /// The hard cap on total supply, or 0 when supply is unlimited.
    pub fn max_supply(e: Env) -> i128 {
        balance::max_supply(&e)
    }

    /// Mints `amount` new tokens to `to`. Admin-only and supply-capped.
    pub fn mint(e: Env, admin_addr: Address, to: Address, amount: i128) {
        admin::check_admin(&e, &admin_addr);
        balance::mint(&e, &to, amount);
    }

    /// Moves `amount` of tokens from `from` to `to`.
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();
        balance::transfer(&e, &from, &to, amount);
    }

    // ---- Escrow ---------------------------------------------------------

    /// Holds `amount` of `token` for `beneficiary` until the event settles, and
    /// returns the new escrow's id.
    ///
    /// The funds leave the depositor immediately and sit in the contract, so a
    /// buyer cannot walk away after a ticket is sold.
    pub fn create_escrow(
        e: Env,
        depositor: Address,
        beneficiary: Address,
        token: Address,
        amount: i128,
        deadline_ledger: u32,
    ) -> u32 {
        escrow::create(&e, &depositor, &beneficiary, &token, amount, deadline_ledger)
    }

    /// Pays the beneficiary everything still held and closes the escrow.
    ///
    /// Settlable by the depositor or the admin, and only while the escrow is
    /// `Active`.
    pub fn release_escrow(e: Env, caller: Address, escrow_id: u32) {
        escrow::release(&e, &caller, escrow_id);
    }

    /// Returns everything still held to the depositor and closes the escrow.
    ///
    /// Settlable by the depositor or the admin, and only while the escrow is
    /// `Active`.
    pub fn refund_escrow(e: Env, caller: Address, escrow_id: u32) {
        escrow::refund(&e, &caller, escrow_id);
    }

    /// Pays `amount` of the held funds to the beneficiary, leaving the escrow
    /// `Active` if anything is still owed.
    pub fn release_partial_escrow(e: Env, caller: Address, escrow_id: u32, amount: i128) {
        escrow::release_partial(&e, &caller, escrow_id, amount);
    }

    /// The full record for `escrow_id`.
    ///
    /// # Panics
    ///
    /// Panics when no escrow exists at `escrow_id`, so a client cannot mistake
    /// "never created" for a record of zeros.
    pub fn get_escrow(e: Env, escrow_id: u32) -> EscrowRecord {
        escrow::record(&e, escrow_id)
    }

    /// Whether `escrow_id` has been settled — released or refunded.
    ///
    /// # Panics
    ///
    /// Panics when no escrow exists at `escrow_id`, for the same reason
    /// [`get_escrow`] does.
    pub fn is_escrow_settled(e: Env, escrow_id: u32) -> bool {
        escrow::is_settled(&e, escrow_id)
    }

    /// Raises a dispute over `escrow_id`, freezing it until a ruling is final.
    pub fn raise_dispute(e: Env, claimant: Address, escrow_id: u64, resolver: Option<Address>) {
        crate::dispute::raise_dispute(&e, &claimant, escrow_id, resolver);
    }

    /// Whether a dispute on `escrow_id` is still live and blocking settlement.
    pub fn is_dispute_open(e: Env, escrow_id: u64) -> bool {
        crate::dispute::is_dispute_open(&e, escrow_id)
    }

    /// The dispute record for `escrow_id`.
    pub fn get_dispute(e: Env, escrow_id: u64) -> crate::storage_types::DisputeRecord {
        crate::dispute::get_dispute(&e, escrow_id)
    }

    /// The retained successful charge history for a recurring schedule.
    pub fn get_recurring_history(e: Env, recurring_id: u64) -> Vec<RecurringExecution> {
        recurring::get_recurring_history(&e, recurring_id)
    }

    /// The arbiter picks a winner: records the ruling and opens the appeal
    /// window; a second call after the window lapses settles and pays.
    pub fn resolve_dispute(e: Env, resolver: Address, escrow_id: u64, winner: Address) {
        crate::dispute::resolve_dispute(&e, resolver, escrow_id, winner);
    }

    /// The losing party escalates a ruling within the appeal window.
    pub fn appeal_dispute(e: Env, caller: Address, escrow_id: u64) {
        crate::dispute::appeal_dispute(&e, &caller, escrow_id);
    }

    /// The final, unappealable ruling: settles the escrow and pays the winner.
    pub fn resolve_appeal(e: Env, resolver: Address, escrow_id: u64, winner: Address) {
        crate::dispute::resolve_appeal(&e, resolver, escrow_id, winner);
    }

    /// Sets the global dispute arbiter (resolver) used to settle disputes.
    ///
    /// Admin-only. Disputes need a designated resolver before any of them can
    /// be settled.
    pub fn set_arbiter(e: Env, admin: Address, arbiter: Address) {
        crate::dispute::set_arbiter(&e, &admin, &arbiter);
    }

    /// The configured dispute arbiter.
    ///
    /// # Panics
    ///
    /// Panics with `ArbiterNotSet` when no arbiter has been configured.
    pub fn get_arbiter(e: Env) -> Address {
        crate::dispute::get_arbiter(&e)
    }

    // ---- Multi-escrow ---------------------------------------------------

    /// Holds `total` of `token` split across `beneficiaries` and returns the
    /// new record's id.
    ///
    /// `beneficiaries[i]` is owed `amounts[i]`; the two vectors must be the
    /// same length and `amounts` must sum exactly to `total`.
    pub fn create_multi_escrow(
        e: Env,
        depositor: Address,
        token: Address,
        beneficiaries: soroban_sdk::Vec<Address>,
        amounts: soroban_sdk::Vec<i128>,
        total: i128,
    ) -> u64 {
        crate::multi_escrow::create(&e, &depositor, &token, beneficiaries, amounts, total)
    }

    /// The full record for `multi_escrow_id`.
    ///
    /// # Panics
    ///
    /// Panics when no multi-escrow exists at `multi_escrow_id`.
    pub fn get_multi_escrow(e: Env, multi_escrow_id: u64) -> crate::storage_types::MultiEscrowRecord {
        crate::multi_escrow::record(&e, multi_escrow_id)
    }

    /// Whether `multi_escrow_id` has been settled — released or refunded.
    ///
    /// # Panics
    ///
    /// Panics when no multi-escrow exists at `multi_escrow_id`.
    pub fn is_multi_escrow_settled(e: Env, multi_escrow_id: u64) -> bool {
        crate::multi_escrow::is_settled(&e, multi_escrow_id)
    }

    /// Pays every beneficiary their stored amount and marks the record
    /// `Released`. Settlement is atomic: a failure on any leg reverts the
    /// whole call.
    ///
    /// Settlable by the depositor or the admin, and only while the record is
    /// `Active`.
    pub fn release_multi_escrow(e: Env, caller: Address, multi_escrow_id: u64) {
        crate::multi_escrow::release(&e, &caller, multi_escrow_id);
    }

    /// Returns the whole deposit to the depositor and marks the record
    /// `Refunded`.
    ///
    /// Settlable by the depositor or the admin, and only while the record is
    /// `Active`.
    pub fn refund_multi_escrow(e: Env, caller: Address, multi_escrow_id: u64) {
        crate::multi_escrow::refund(&e, &caller, multi_escrow_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_types::EscrowStatus;
    use soroban_sdk::token::StellarAssetClient;
    use soroban_sdk::testutils::{Address as _, Events as _};
    use soroban_sdk::{vec, xdr, FromVal, IntoVal};

    struct Fixture {
        e: Env,
        client: VeriTixPayClient<'static>,
        contract_id: Address,
        contract: Address,
        admin: Address,
    }

    impl Fixture {
        fn new() -> Self {
            let e = Env::default();
            e.mock_all_auths();
            let contract_id = e.register_contract(None, VeriTixPay);
            let client = VeriTixPayClient::new(&e, &contract_id);
            let admin = Address::generate(&e);
            client.initialize(&admin);
            let contract = contract_id.clone();
            Fixture {
                e,
                client,
                contract_id,
                contract,
                admin,
            }
        }

        /// A SEP-41 asset, and an escrow over `amount` of it.
        fn escrow(
            &self,
            amount: i128,
        ) -> (Address, Address, Address, StellarAssetClient<'static>, u32) {
            let depositor = Address::generate(&self.e);
            let beneficiary = Address::generate(&self.e);
            let token_address = self
                .e
                .register_stellar_asset_contract_v2(self.admin.clone())
                .address();
            let token = StellarAssetClient::new(&self.e, &token_address);
            token.mint(&depositor, &amount);
            let id = self.client.create_escrow(
                &depositor,
                &beneficiary,
                &token_address,
                &amount,
                &2_000,
            );
            (depositor, beneficiary, token_address, token, id)
        }

        /// A SEP-41 asset with `holder` holding `amount` of it.
        fn asset(&self, holder: &Address, amount: i128) -> (Address, StellarAssetClient<'static>) {
            let address = self
                .e
                .register_stellar_asset_contract_v2(self.admin.clone())
                .address();
            let token = StellarAssetClient::new(&self.e, &address);
            token.mint(holder, &amount);
            (address, token)
        }

        /// The total the contract reports as still held in escrow.
        fn locked(&self) -> i128 {
            self.e
                .as_contract(&self.contract_id, || escrow::value_locked(&self.e))
        }

        fn escrows_created(&self) -> u32 {
            self.e
                .as_contract(&self.contract_id, || escrow::count(&self.e))
        }
    }

    fn last_topics(e: &Env) -> std::vec::Vec<xdr::ScVal> {
        let events = e.events().all();
        let last = events.events().last().expect("no event was emitted");
        let xdr::ContractEventBody::V0(body) = &last.body else {
            panic!("expected a v0 contract event");
        };
        body.topics.clone()
    }

    fn last_data(e: &Env) -> xdr::ScVal {
        let events = e.events().all();
        let last = events.events().last().expect("no event was emitted");
        let xdr::ContractEventBody::V0(body) = &last.body else {
            panic!("expected a v0 contract event");
        };
        body.data.clone().expect("an event with no data")
    }

    fn panics<R>(f: impl FnOnce() -> R) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err()
    }

    // ---- create_escrow (prerequisite) -----------------------------------

    #[test]
    fn create_escrow_holds_the_funds_for_the_beneficiary() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, _id) = f.escrow(1_000);

        assert_eq!(token.balance(&depositor), 0);
        assert_eq!(token.balance(&f.contract), 1_000);
        assert_eq!(token.balance(&beneficiary), 0);
        assert_eq!(f.locked(), 1_000);
    }

    #[test]
    fn a_new_escrow_is_active_and_spendable_by_neither_side() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        assert!(!f.client.is_escrow_settled(&id));
        assert_eq!(f.client.get_escrow(&id).status, EscrowStatus::Active);
    }

    // ---- #883: release_escrow -------------------------------------------

    #[test]
    fn release_pays_the_beneficiary_everything_held() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_escrow(&Address::generate(&f.e), &id);

        assert_eq!(token.balance(&beneficiary), 1_000);
        assert_eq!(token.balance(&f.contract), 0);
    }

    #[test]
    fn release_marks_the_escrow_released_and_settled() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.release_escrow(&Address::generate(&f.e), &id);

        assert_eq!(f.client.get_escrow(&id).status, EscrowStatus::Released);
        assert!(f.client.is_escrow_settled(&id));
    }

    #[test]
    fn release_decrements_the_locked_value() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.release_escrow(&Address::generate(&f.e), &id);

        assert_eq!(f.locked(), 0);
    }

    #[test]
    fn the_depositor_can_release() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_escrow(&depositor, &id);

        assert_eq!(token.balance(&beneficiary), 1_000);
    }

    #[test]
    fn the_admin_can_release() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_escrow(&f.admin, &id);

        assert_eq!(token.balance(&beneficiary), 1_000);
    }

    #[test]
    fn a_stranger_cannot_release() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);
        let stranger = Address::generate(&f.e);

        assert!(panics(|| {
            f.client.release_escrow(&stranger, &id);
        }));
        assert_eq!(token.balance(&f.contract), 1_000);
        assert!(!f.client.is_escrow_settled(&id));
    }

    #[test]
    fn a_released_escrow_cannot_be_released_again() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);
        f.client.release_escrow(&f.admin, &id);

        assert!(panics(|| {
            f.client.release_escrow(&f.admin, &id);
        }));
        // The beneficiary is paid once, not twice.
        assert_eq!(token.balance(&beneficiary), 1_000);
        assert_eq!(f.locked(), 0);
    }

    #[test]
    fn releasing_an_unknown_escrow_panics() {
        let f = Fixture::new();

        assert!(panics(|| {
            f.client.release_escrow(&f.admin, &99);
        }));
    }

    #[test]
    fn release_emits_the_beneficiary_the_id_and_the_amount() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.release_escrow(&f.admin, &id);

        assert_eq!(
            last_topics(&f.e),
            std::vec![
                xdr::ScVal::Symbol("escrow_released".try_into().unwrap()),
                xdr::ScVal::from_val(&f.e, &beneficiary.to_val()),
            ]
        );
        assert_eq!(
            last_data(&f.e),
            xdr::ScVal::from_val(
                &f.e,
                &vec![&f.e, id.to_val(), 1_000i128.to_val(), 0i128.to_val()]
            )
        );
    }

    #[test]
    fn releasing_one_escrow_leaves_the_others_locked() {
        let f = Fixture::new();
        let (_d1, _b1, _t1, _a1, first) = f.escrow(400);
        let (_d2, _b2, _t2, _a2, second) = f.escrow(600);

        f.client.release_escrow(&f.admin, &first);

        assert!(f.client.is_escrow_settled(&first));
        assert!(!f.client.is_escrow_settled(&second));
        assert_eq!(f.locked(), 600);
    }

    // ---- #884: refund_escrow --------------------------------------------

    #[test]
    fn refund_pays_the_depositor_everything_held() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.refund_escrow(&depositor, &id);

        assert_eq!(token.balance(&depositor), 1_000);
        assert_eq!(token.balance(&f.contract), 0);
        assert_eq!(token.balance(&beneficiary), 0);
    }

    #[test]
    fn refund_marks_the_escrow_refunded_and_settled() {
        let f = Fixture::new();
        let (depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.refund_escrow(&depositor, &id);

        assert_eq!(f.client.get_escrow(&id).status, EscrowStatus::Refunded);
        assert!(f.client.is_escrow_settled(&id));
        assert_eq!(f.locked(), 0);
    }

    #[test]
    fn the_admin_can_refund() {
        let f = Fixture::new();
        let (depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.refund_escrow(&f.admin, &id);

        assert_eq!(token.balance(&depositor), 1_000);
    }

    #[test]
    fn a_stranger_cannot_refund() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);
        let stranger = Address::generate(&f.e);

        assert!(panics(|| {
            f.client.refund_escrow(&stranger, &id);
        }));
        assert_eq!(token.balance(&f.contract), 1_000);
        assert!(!f.client.is_escrow_settled(&id));
    }

    #[test]
    fn a_refunded_escrow_cannot_be_released() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);
        f.client.refund_escrow(&depositor, &id);

        assert!(panics(|| {
            f.client.release_escrow(&f.admin, &id);
        }));
        assert_eq!(token.balance(&beneficiary), 0);
        assert_eq!(token.balance(&depositor), 1_000);
    }

    #[test]
    fn a_released_escrow_cannot_be_refunded() {
        let f = Fixture::new();
        let (depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);
        f.client.release_escrow(&f.admin, &id);

        assert!(panics(|| {
            f.client.refund_escrow(&depositor, &id);
        }));
        assert_eq!(token.balance(&depositor), 0);
    }

    #[test]
    fn refund_emits_the_depositor_the_id_and_the_amount() {
        let f = Fixture::new();
        let (depositor, _beneficiary, _token_address, _token, id) = f.escrow(750);

        f.client.refund_escrow(&depositor, &id);

        assert_eq!(
            last_topics(&f.e),
            std::vec![
                xdr::ScVal::Symbol("escrow_refunded".try_into().unwrap()),
                xdr::ScVal::from_val(&f.e, &depositor.to_val()),
            ]
        );
        assert_eq!(
            last_data(&f.e),
            xdr::ScVal::from_val(&f.e, &vec![&f.e, id.to_val(), 750i128.to_val()])
        );
    }

    // ---- #885: release_partial_escrow ----------------------------------

    #[test]
    fn a_partial_release_pays_only_that_amount() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);

        assert_eq!(token.balance(&beneficiary), 300);
        assert_eq!(token.balance(&f.contract), 700);
    }

    #[test]
    fn a_partial_release_reduces_the_record_and_leaves_it_active() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);

        let record = f.client.get_escrow(&id);
        assert_eq!(record.amount, 700);
        assert_eq!(record.status, EscrowStatus::Active);
        assert!(!f.client.is_escrow_settled(&id));
    }

    #[test]
    fn a_partial_release_decrements_the_locked_value() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);

        assert_eq!(f.locked(), 700);
    }

    #[test]
    fn successive_partial_releases_add_up() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);
        f.client.release_partial_escrow(&f.admin, &id, &200);

        assert_eq!(token.balance(&beneficiary), 500);
        assert_eq!(f.client.get_escrow(&id).amount, 500);
        assert_eq!(f.locked(), 500);
    }

    #[test]
    fn a_partial_release_of_the_remainder_settles_the_escrow() {
        // "Active until the remainder is zero" — the call that takes it to zero
        // is the one that closes the escrow.
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);
        f.client.release_partial_escrow(&f.admin, &id, &700);

        assert_eq!(token.balance(&beneficiary), 1_000);
        let record = f.client.get_escrow(&id);
        assert_eq!(record.amount, 0);
        assert_eq!(record.status, EscrowStatus::Released);
        assert!(f.client.is_escrow_settled(&id));
        assert_eq!(f.locked(), 0);
    }

    #[test]
    fn a_partial_release_above_the_remainder_is_refused() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);
        f.client.release_partial_escrow(&f.admin, &id, &300);

        assert!(panics(|| {
            f.client.release_partial_escrow(&f.admin, &id, &701);
        }));
        assert_eq!(token.balance(&beneficiary), 300);
        assert_eq!(f.client.get_escrow(&id).amount, 700);
        assert_eq!(f.locked(), 700);
    }

    #[test]
    fn a_partial_release_of_zero_is_refused() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        assert!(panics(|| {
            f.client.release_partial_escrow(&f.admin, &id, &0);
        }));
        assert_eq!(token.balance(&beneficiary), 0);
        assert_eq!(f.client.get_escrow(&id).amount, 1_000);
    }

    #[test]
    fn a_negative_partial_release_is_refused() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);

        assert!(panics(|| {
            f.client.release_partial_escrow(&f.admin, &id, &-100);
        }));
        assert_eq!(token.balance(&beneficiary), 0);
        assert_eq!(f.client.get_escrow(&id).amount, 1_000);
        assert_eq!(f.locked(), 1_000);
    }

    #[test]
    fn a_settled_escrow_cannot_be_partially_released() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);
        f.client.release_escrow(&f.admin, &id);

        assert!(panics(|| {
            f.client.release_partial_escrow(&f.admin, &id, &100);
        }));
        assert_eq!(token.balance(&beneficiary), 1_000);
    }

    #[test]
    fn a_stranger_cannot_partially_release() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);
        let stranger = Address::generate(&f.e);

        assert!(panics(|| {
            f.client.release_partial_escrow(&stranger, &id, &100);
        }));
        assert_eq!(token.balance(&f.contract), 1_000);
    }

    #[test]
    fn the_depositor_can_partially_release() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&depositor, &id, &250);

        assert_eq!(token.balance(&beneficiary), 250);
    }

    #[test]
    fn a_full_release_after_a_partial_pays_the_remainder() {
        let f = Fixture::new();
        let (_depositor, beneficiary, _token_address, token, id) = f.escrow(1_000);
        f.client.release_partial_escrow(&f.admin, &id, &250);

        f.client.release_escrow(&f.admin, &id);

        assert_eq!(token.balance(&beneficiary), 1_000);
        assert_eq!(f.locked(), 0);
        assert!(f.client.is_escrow_settled(&id));
    }

    #[test]
    fn a_partial_release_reports_the_remaining_amount() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.release_partial_escrow(&f.admin, &id, &300);

        assert_eq!(
            last_data(&f.e),
            xdr::ScVal::from_val(
                &f.e,
                &vec![&f.e, id.to_val(), 300i128.to_val(), 700i128.to_val()]
            )
        );
    }

    // ---- #886: the views -----------------------------------------------

    #[test]
    fn get_escrow_returns_the_stored_record() {
        let f = Fixture::new();
        let depositor = Address::generate(&f.e);
        let beneficiary = Address::generate(&f.e);
        let (token_address, _) = f.asset(&depositor, 2_000);
        let id = f
            .client
            .create_escrow(&depositor, &beneficiary, &token_address, &1_000, &2_000);

        let record = f.client.get_escrow(&id);

        assert_eq!(record.depositor, depositor);
        assert_eq!(record.beneficiary, beneficiary);
        assert_eq!(record.token, token_address);
        assert_eq!(record.amount, 1_000);
        assert_eq!(record.deadline_ledger, 2_000);
        assert_eq!(record.status, EscrowStatus::Active);
    }

    #[test]
    fn get_escrow_panics_on_an_unknown_id() {
        let f = Fixture::new();

        assert!(panics(|| {
            f.client.get_escrow(&7);
        }));
    }

    #[test]
    fn is_escrow_settled_is_false_before_and_true_after() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        assert!(!f.client.is_escrow_settled(&id));
        f.client.release_escrow(&f.admin, &id);
        assert!(f.client.is_escrow_settled(&id));
    }

    #[test]
    fn is_escrow_settled_is_true_for_a_refund_too() {
        let f = Fixture::new();
        let (depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);

        f.client.refund_escrow(&depositor, &id);

        assert!(f.client.is_escrow_settled(&id));
    }

    #[test]
    fn is_escrow_settled_panics_on_an_unknown_id() {
        // "Not settled" would be a false answer to a question about an escrow
        // that does not exist, and the caller would carry it forward.
        let f = Fixture::new();

        assert!(panics(|| {
            f.client.is_escrow_settled(&7);
        }));
    }

    #[test]
    fn the_view_follows_a_partial_release() {
        let f = Fixture::new();
        let (_depositor, _beneficiary, _token_address, _token, id) = f.escrow(1_000);
        f.client.release_partial_escrow(&f.admin, &id, &400);

        assert_eq!(f.client.get_escrow(&id).amount, 600);
        assert!(!f.client.is_escrow_settled(&id));
    }
}
