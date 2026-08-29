use crate::admin::{
    accept_admin as accept_pending_admin, check_admin, has_admin, propose_admin,
    read_admin, read_admin_active_after, read_clawback_cosigner, read_pending_admin,
    transfer_admin, write_admin, write_clawback_cosigner,
};
use crate::allowance::{get_allowances_for_spender, read_allowance, revoke_all_allowances, spend_allowance, validate_allowance, write_allowance};
use crate::balance::{decrease_supply, increase_supply, read_balance, read_max_supply, read_total_supply, receive_balance, spend_balance};
use crate::batch::{approve_batch, burn_from_batch, clawback_batch, freeze_batch, transfer_batch_with_memo, unfreeze_batch};
use crate::storage_types::DataKey;
use crate::dispute::{
    expire_dispute, get_dispute as dispute_get, get_dispute_history_for_escrow,
    get_open_disputes, open_dispute, resolve_dispute, DisputeRecord,
    get_disputes_by_claimant,
};
use crate::escrow::{
    admin_settle_escrow as escrow_admin_settle, create_escrow as escrow_create,
    escrow_between as escrow_between_fn, escrow_stats as escrow_get_stats,
    escrowed_value_for_depositor as escrowed_value_for_depositor_fn,
    get_escrow as escrow_get, refund_escrow as escrow_refund,
    release_escrow as escrow_release, EscrowRecord,
};
use crate::freeze::{freeze_account, get_frozen_accounts, is_frozen as read_frozen_status, unfreeze_account};
use crate::metadata::{read_decimal, read_name, read_symbol, update_metadata_fields, validate_metadata, write_metadata, TokenMetadata};
use crate::pause::{is_paused, pause, require_not_paused, unpause};
use crate::recurring::{
    amend_recurring, cancel_recurring, execute_recurring, get_next_execution_ledger,
    get_recurring, get_recurring_by_payer, is_executable, pause_recurring,
    recurring_count_for_payer, resume_recurring, setup_recurring,
    transfer_recurring_payer, RecurringRecord,
};
use crate::splitter::{
    create_split as split_create,
    get_split as split_get, get_splits_for_recipient as split_get_for_recipient,
    splitter_stats as split_stats, SplitRecord, SplitRecipient, SplitterStats,
};
use crate::validation::{require_not_frozen_account, require_positive_amount};
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Bytes, Env, String, Vec};

#[contract]
pub struct VeritixToken;

#[derive(Clone)]
#[contracttype]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimal: u32,
    pub total_supply: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct AdminInfo {
    pub admin: Address,
    pub paused: bool,
}

#[derive(Clone)]
#[contracttype]
pub struct ContractStats {
    pub escrow_count: u32,
    pub split_count: u32,
    pub recurring_count: u32,
    pub dispute_count: u32,
    pub total_supply: i128,
}

#[contractimpl]
impl VeritixToken {
    // --- Admin ---
    /// Initialises the token contract, setting the admin address and token metadata.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `admin` - Address that will hold admin privileges (mint, clawback, freeze, pause).
    /// * `name` - Human-readable token name (e.g. `"Veritix Pay"`).
    /// * `symbol` - Ticker symbol (e.g. `"VTX"`).
    /// * `decimal` - Decimal precision used by wallet UIs (typically `7` on Stellar).
    ///
    /// # Panics
    /// * `"AlreadyInitialized"` — if the contract has been previously initialised.
    /// * Panics from `validate_metadata` if name/symbol are empty or too long.
    ///
    /// # Example
    /// ```rust,ignore
    /// client.initialize(&admin, &name, &symbol, &7u32);
    /// ```
    pub fn initialize(e: Env, admin: Address, name: String, symbol: String, decimal: u32) {
        if has_admin(&e) {
            panic!("AlreadyInitialized");
        }
        let metadata = TokenMetadata { name: name.clone(), symbol: symbol.clone(), decimal };
        validate_metadata(&metadata);
        write_metadata(&e, metadata);
        write_admin(&e, &admin);
        e.storage().instance().set(&DataKey::MaxSupply, &0i128);
        e.events().publish(
            (symbol_short!("init_done"), admin),
            (name, symbol, decimal),
        );
    }
    pub fn initialize_with_max_supply(e: Env, admin: Address, name: String, symbol: String, decimal: u32, max_supply: i128) {
        if has_admin(&e) {
            panic!("AlreadyInitialized");
        }
        let metadata = TokenMetadata { name: name.clone(), symbol: symbol.clone(), decimal };
        validate_metadata(&metadata);
        write_metadata(&e, metadata);
        write_admin(&e, &admin);
        e.storage().instance().set(&DataKey::MaxSupply, &max_supply);
        e.events().publish(
            (symbol_short!("init_done"), admin),
            (name, symbol, decimal, max_supply),
        );
    }

    /// Updates the max supply cap. Only allows reducing the cap, never increasing it.
    /// 
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `admin` - Caller; must equal the stored admin address.
    /// * `new_max` - New max supply value.
    /// 
    /// # Panics
    /// * `"Unauthorized"` - if admin is not the current admin.
    /// * `"Cannot set max supply below current total supply"` - if new_max is less than the current total supply.
    /// * `"CannotRaiseMaxSupply"` - if attempting to increase the max supply from its current value.
    pub fn set_max_supply(e: Env, admin: Address, new_max: i128) {
        check_admin(&e, &admin);
        crate::balance::set_max_supply(&e, new_max);
        e.events().publish(
            (soroban_sdk::Symbol::new(&e, "max_supply"), admin),
            (new_max,),
        );
    }
    pub fn set_admin(e: Env, new_admin: Address) {
        transfer_admin(&e, new_admin);
    }
    pub fn propose_admin(e: Env, new_admin: Address) {
        propose_admin(&e, &new_admin);
    }
    pub fn accept_admin(e: Env) {
        accept_pending_admin(&e);
    }
    pub fn admin(e: Env) -> Address {
        read_admin(&e)
    }
    pub fn has_admin(e: Env) -> bool {
        has_admin(&e)
    }
    pub fn admin_info(e: Env) -> AdminInfo {
        AdminInfo { admin: read_admin(&e), paused: is_paused(&e) }
    }
    pub fn get_pending_admin(e: Env) -> Option<Address> {
        read_pending_admin(&e)
    }
    pub fn admin_active_after(e: Env) -> Option<u32> {
        read_admin_active_after(&e)
    }

    // --- Pause ---
    pub fn pause(e: Env, admin: Address) {
        pause(&e, admin);
    }
    pub fn unpause(e: Env, admin: Address) {
        unpause(&e, admin);
    }
    pub fn is_paused(e: Env) -> bool {
        is_paused(&e)
    }

    // --- Token Operations ---
    /// Mints `amount` new tokens into `to`'s balance.
    ///
    /// Only the contract admin may call this function.  Increases total supply.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `admin` - Caller; must equal the stored admin address.
    /// * `to` - Recipient of the newly minted tokens.
    /// * `amount` - Positive quantity to mint (in base units).
    ///
    /// # Panics
    /// * `"InvalidRecipient: cannot transfer directly to the contract address"` — if `to` is the contract.
    /// * `"Unauthorized"` — if `admin` is not the current admin.
    /// * `"Paused"` — if the contract is paused.
    /// * `"InvalidAmount: must be positive"` — if `amount ≤ 0`.
    /// * `"supply overflow"` — if minting would overflow `i128`.
    ///
    /// # Example
    /// ```rust,ignore
    /// client.mint(&admin, &recipient, &500_000_0000i128);
    /// ```
    pub fn mint(e: Env, admin: Address, to: Address, amount: i128) {
        if to == e.current_contract_address() {
            panic!("InvalidRecipient: cannot transfer directly to the contract address — use create_escrow instead");
        }
        check_admin(&e, &admin);
        require_not_paused(&e);
        require_positive_amount(amount);
        receive_balance(&e, to.clone(), amount);
        increase_supply(&e, amount);
        e.events().publish((symbol_short!("mint"), admin), (to, amount));
    }
    /// Burns `amount` tokens from `from`'s balance, reducing total supply.
    ///
    /// Requires `from` to authorise the call.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `from` - Address whose balance is reduced; must `require_auth`.
    /// * `amount` - Positive quantity to burn (in base units).
    ///
    /// # Panics
    /// * `"Paused"` — if the contract is paused.
    /// * `"InvalidAmount: must be positive"` — if `amount ≤ 0`.
    /// * `"InsufficientBalance"` — if `from` lacks sufficient funds.
    /// * `"supply underflow"` — if burning would underflow.
    ///
    /// # Example
    /// ```rust,ignore
    /// client.burn(&holder, &100_0000i128);
    /// ```
    pub fn burn(e: Env, from: Address, amount: i128) {
        from.require_auth();
        require_not_paused(&e);
        require_positive_amount(amount);
        spend_balance(&e, from.clone(), amount);
        decrease_supply(&e, amount);
        e.events().publish((symbol_short!("burn"), from), amount);
    }
    pub fn burn_from(e: Env, spender: Address, from: Address, amount: i128) {
        spender.require_auth();
        require_not_frozen_account(&e, &from);
        require_not_paused(&e);
        require_positive_amount(amount);
        spend_allowance(&e, from.clone(), spender.clone(), amount);
        spend_balance(&e, from.clone(), amount);
        decrease_supply(&e, amount);
        e.events().publish((symbol_short!("burn_from"), spender), (from, amount));
    }
    pub fn set_clawback_cosigner(e: Env, admin: Address, cosigner: Address) {
        check_admin(&e, &admin);
        write_clawback_cosigner(&e, &cosigner);
        e.events().publish((symbol_short!("cosigner"), admin), cosigner);
    }
    pub fn clawback(e: Env, admin: Address, from: Address, amount: i128) {
        check_admin(&e, &admin);
        if from == e.current_contract_address() {
            panic!("InvalidClawback: cannot clawback from the contract address");
        }
        if let Some(cosigner) = read_clawback_cosigner(&e) {
            cosigner.require_auth();
        }
        require_positive_amount(amount);
        spend_balance(&e, from.clone(), amount);
        decrease_supply(&e, amount);
        e.events().publish((symbol_short!("clawback"), admin), (from, amount));
    }
    pub fn clawback_batch(e: Env, admin: Address, targets: Vec<(Address, i128)>) {
        check_admin(&e, &admin);
        if let Some(cosigner) = read_clawback_cosigner(&e) {
            cosigner.require_auth();
        }
        clawback_batch(&e, admin, targets);
    }
    pub fn burn_from_batch(e: Env, spender: Address, targets: Vec<(Address, i128)>) {
        burn_from_batch(&e, spender, targets);
    }
    /// Transfers `amount` tokens from `from` to `to`.
    ///
    /// Requires `from` to authorise the call.  The contract must not be paused
    /// and neither account may be frozen.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `from` - Sender address; must call `require_auth` implicitly.
    /// * `to` - Recipient address.
    /// * `amount` - Positive token quantity (in base units).
    ///
    /// # Panics
    /// * `"InvalidRecipient: cannot transfer directly to the contract address"` — if `to` equals the contract.
    /// * `"Paused"` — if the contract is paused.
    /// * `"AccountFrozen"` — if either party is frozen.
    /// * `"InsufficientBalance"` — if `from` lacks sufficient funds.
    /// * `"InvalidAmount: must be positive"` — if `amount ≤ 0`.
    ///
    /// # Example
    /// ```rust,ignore
    /// client.transfer(&from, &to, &1_000_0000i128);
    /// ```
    pub fn transfer(e: Env, from: Address, to: Address, amount: i128) {
        if to == e.current_contract_address() {
            panic!("InvalidRecipient: cannot transfer directly to the contract address — use create_escrow instead");
        }
        from.require_auth();
        require_not_paused(&e);
        require_not_frozen_account(&e, &from);
        require_positive_amount(amount);
        spend_balance(&e, from.clone(), amount);
        receive_balance(&e, to.clone(), amount);
        e.events().publish((symbol_short!("transfer"), from), (to, amount));
    }
    pub fn transfer_from(e: Env, spender: Address, from: Address, to: Address, amount: i128) {
        if to == e.current_contract_address() {
            panic!("InvalidRecipient: cannot transfer directly to the contract address — use create_escrow instead");
        }
        spender.require_auth();
        require_not_paused(&e);
        require_not_frozen_account(&e, &from);
        require_positive_amount(amount);
        validate_allowance(&e, from.clone(), spender.clone(), amount);
        spend_allowance(&e, from.clone(), spender.clone(), amount);
        spend_balance(&e, from.clone(), amount);
        receive_balance(&e, to.clone(), amount);
        e.events().publish((symbol_short!("xfer_from"), from), (to, amount));
    }
    pub fn transfer_with_memo(e: Env, from: Address, to: Address, amount: i128, memo: Bytes) {
        from.require_auth();
        require_not_paused(&e);
        require_not_frozen_account(&e, &from);
        require_positive_amount(amount);
        spend_balance(&e, from.clone(), amount);
        receive_balance(&e, to.clone(), amount);
        e.events().publish((symbol_short!("xfer_memo"), from), (to, amount, memo));
    }
    pub fn approve(e: Env, from: Address, spender: Address, amount: i128, expiration_ledger: u32) {
        from.require_auth();
        write_allowance(&e, from.clone(), spender.clone(), amount, expiration_ledger);
        e.events().publish((symbol_short!("approve"), from), (spender, amount));
    }
    pub fn freeze(e: Env, target: Address) {
        let admin = read_admin(&e);
        check_admin(&e, &admin);
        freeze_account(&e, admin, target);
    }
    pub fn unfreeze(e: Env, target: Address) {
        let admin = read_admin(&e);
        check_admin(&e, &admin);
        unfreeze_account(&e, admin, target);
    }
    pub fn freeze_batch(e: Env, admin: Address, targets: Vec<Address>) {
        freeze_batch(&e, admin, targets);
    }
    pub fn unfreeze_batch(e: Env, admin: Address, targets: Vec<Address>) {
        unfreeze_batch(&e, admin, targets);
    }
    pub fn approve_batch(e: Env, from: Address, approvals: Vec<(Address, i128, u32)>) {
        approve_batch(&e, from, approvals);
    }
    pub fn transfer_batch_with_memo(e: Env, from: Address, recipients: Vec<(Address, i128, Bytes)>) {
        transfer_batch_with_memo(&e, from, recipients);
    }

    // --- Views ---
    pub fn total_supply(e: Env) -> i128 {
        read_total_supply(&e)
    }
    pub fn max_supply(e: Env) -> i128 {
        read_max_supply(&e)
    }
    pub fn balance(e: Env, id: Address) -> i128 {
        read_balance(&e, id)
    }
    pub fn balance_of_batch(e: Env, addresses: Vec<Address>) -> Vec<i128> {
        let mut balances: Vec<i128> = Vec::new(&e);
        for i in 0..addresses.len() {
            let addr = addresses.get(i).unwrap();
            balances.push_back(read_balance(&e, addr));
        }
        balances
    }
    pub fn allowance(e: Env, from: Address, spender: Address) -> i128 {
        read_allowance(&e, from, spender).amount
    }
    pub fn is_frozen(e: Env, id: Address) -> bool {
        read_frozen_status(&e, &id)
    }
    pub fn is_frozen_batch(e: Env, addresses: Vec<Address>) -> Vec<bool> {
        let mut result: Vec<bool> = Vec::new(&e);
        for i in 0..addresses.len() {
            let addr = addresses.get(i).unwrap();
            result.push_back(read_frozen_status(&e, &addr));
        }
        result
    }
    pub fn total_holders(e: Env) -> u32 {
        let key = DataKey::HolderSet;
        let holders: Vec<Address> = e.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(&e));
        holders.len()
    }
    pub fn get_holders(e: Env, offset: u32, limit: u32) -> Vec<Address> {
        let key = DataKey::HolderSet;
        let all: Vec<Address> = e.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(&e));
        let mut result: Vec<Address> = Vec::new(&e);
        let end = (offset + limit).min(all.len());
        let mut i = offset;
        while i < end {
            result.push_back(all.get(i).unwrap());
            i += 1;
        }
        result
    }
    pub fn frozen_accounts(e: Env) -> Vec<Address> {
        get_frozen_accounts(&e)
    }
    pub fn decimals(e: Env) -> u32 {
        read_decimal(&e)
    }
    pub fn name(e: Env) -> String {
        read_name(&e)
    }
    pub fn symbol(e: Env) -> String {
        read_symbol(&e)
    }
    pub fn get_allowances_for_spender(e: Env, spender: Address) -> Vec<Address> {
        get_allowances_for_spender(&e, spender)
    }
    pub fn revoke_all_allowances(e: Env, from: Address) {
        revoke_all_allowances(&e, from);
    }
    pub fn token_info(e: Env) -> TokenInfo {
        TokenInfo {
            name: read_name(&e),
            symbol: read_symbol(&e),
            decimal: read_decimal(&e),
            total_supply: read_total_supply(&e),
        }
    }
    pub fn update_metadata(e: Env, admin: Address, name: Option<String>, symbol: Option<String>) {
        check_admin(&e, &admin);
        update_metadata_fields(&e, name, symbol);
        e.events().publish((symbol_short!("meta_upd"), admin), ());
    }
    pub fn contract_stats(e: Env) -> ContractStats {
        crate::storage_types::bump_instance(&e);
        ContractStats {
            escrow_count: crate::storage_types::read_counter(&e, &crate::storage_types::DataKey::EscrowCount),
            split_count: crate::storage_types::read_counter(&e, &crate::storage_types::DataKey::SplitCount),
            recurring_count: crate::storage_types::read_counter(&e, &crate::storage_types::DataKey::RecurringCount),
            dispute_count: crate::storage_types::read_counter(&e, &crate::storage_types::DataKey::DisputeCount),
            total_supply: read_total_supply(&e),
        }
    }

    // --- Escrow ---
    /// Creates a new escrow and locks `amount` tokens from `depositor` until `expiry_ledger`.
    ///
    /// Returns the numeric escrow ID that must be passed to `release_escrow`,
    /// `refund_escrow`, or `admin_settle_escrow`.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `depositor` - Address funding the escrow; must `require_auth`.
    /// * `beneficiary` - Address that receives funds on release.
    /// * `amount` - Positive token quantity to lock.
    /// * `expiry_ledger` - Ledger number after which the depositor may reclaim funds.
    ///
    /// # Panics
    /// * `"InvalidAmount: must be positive"` — if `amount ≤ 0`.
    /// * `"InsufficientBalance"` — if `depositor` has insufficient balance.
    ///
    /// # Example
    /// ```rust,ignore
    /// let id = client.create_escrow(&buyer, &seller, &10_000_0000i128, &current_ledger + 1000);
    /// ```
    pub fn create_escrow(e: Env, depositor: Address, beneficiary: Address, amount: i128, expiry_ledger: u32) -> u32 {
        escrow_create(&e, depositor, beneficiary, amount, expiry_ledger)
    }
    /// Releases an escrow, transferring the locked amount to the beneficiary.
    ///
    /// Only the depositor or the contract admin may release before expiry.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `caller` - Depositor or admin; must `require_auth`.
    /// * `escrow_id` - ID returned by `create_escrow`.
    ///
    /// # Panics
    /// * `"EscrowNotFound"` — if no escrow exists with the given ID.
    /// * `"Unauthorized"` — if `caller` is neither the depositor nor the admin.
    /// * `"EscrowAlreadyReleased"` — if the escrow has been previously settled.
    ///
    /// # Example
    /// ```rust,ignore
    /// client.release_escrow(&buyer, &escrow_id);
    /// ```
    pub fn release_escrow(e: Env, caller: Address, escrow_id: u32) {
        escrow_release(&e, caller, escrow_id)
    }
    pub fn refund_escrow(e: Env, caller: Address, escrow_id: u32) {
        escrow_refund(&e, caller, escrow_id)
    }
    pub fn get_escrow(e: Env, escrow_id: u32) -> EscrowRecord {
        escrow_get(&e, escrow_id)
    }
    pub fn admin_settle_escrow(e: Env, admin: Address, escrow_id: u32, recipient: Address) {
        escrow_admin_settle(&e, admin, escrow_id, recipient)
    }
    pub fn escrow_count(e: Env) -> u32 {
        crate::storage_types::bump_instance(&e);
        crate::storage_types::read_counter(&e, &crate::storage_types::DataKey::EscrowCount)
    }
    pub fn escrow_stats(e: Env) -> crate::storage_types::EscrowStats {
        crate::storage_types::bump_instance(&e);
        escrow_get_stats(&e)
    }
    pub fn escrowed_value_for_depositor(e: Env, depositor: Address) -> i128 {
        escrowed_value_for_depositor_fn(&e, depositor)
    }

    // --- Disputes ---
    /// Opens a dispute against an escrow, freezing it until resolved or expired.
    ///
    /// Returns a dispute ID.  The designated `resolver` must call `resolve_dispute`
    /// or the dispute will expire automatically at `expiry_ledger`.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `claimant` - Party opening the dispute; must `require_auth`.
    /// * `escrow_id` - Target escrow ID.
    /// * `resolver` - Trusted third party authorised to resolve the dispute.
    /// * `evidence` - Arbitrary bytes (e.g. IPFS CID, hash of off-chain evidence).
    /// * `expiry_ledger` - Ledger at which the dispute auto-expires if unresolved.
    ///
    /// # Panics
    /// * `"EscrowNotFound"` — if the escrow does not exist.
    /// * `"DisputeAlreadyOpen"` — if a dispute is already pending for this escrow.
    ///
    /// # Example
    /// ```rust,ignore
    /// let dispute_id = client.open_dispute(&buyer, &escrow_id, &resolver, &evidence_bytes, &expiry);
    /// ```
    pub fn open_dispute(e: Env, claimant: Address, escrow_id: u32, resolver: Address, evidence: Bytes, expiry_ledger: u32, mediation_fee_bps: u32) -> u32 {
        open_dispute(&e, claimant, escrow_id, resolver, evidence, expiry_ledger, mediation_fee_bps)
    }
    /// Resolves an open dispute, releasing the escrowed funds to one party.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `resolver` - Address designated at `open_dispute`; must `require_auth`.
    /// * `dispute_id` - ID returned by `open_dispute`.
    /// * `release_to_beneficiary` - If `true`, funds go to the beneficiary; if `false`, funds return to the depositor.
    ///
    /// # Panics
    /// * `"DisputeNotFound"` — if no dispute exists with the given ID.
    /// * `"Unauthorized"` — if `resolver` does not match the stored resolver.
    /// * `"DisputeExpired"` — if `expiry_ledger` has passed.
    ///
    /// # Example
    /// ```rust,ignore
    /// client.resolve_dispute(&resolver, &dispute_id, &true);
    /// ```
    pub fn resolve_dispute(e: Env, resolver: Address, dispute_id: u32, release_to_beneficiary: bool) {
        resolve_dispute(&e, resolver, dispute_id, release_to_beneficiary)
    }
    pub fn expire_dispute(e: Env, dispute_id: u32) {
        expire_dispute(&e, dispute_id)
    }
    pub fn get_dispute(e: Env, dispute_id: u32) -> DisputeRecord {
        dispute_get(&e, dispute_id)
    }
    pub fn get_dispute_history_for_escrow(e: Env, escrow_id: u32) -> Vec<u32> {
        get_dispute_history_for_escrow(&e, escrow_id)
    }
    pub fn get_open_disputes(e: Env) -> Vec<u32> {
        get_open_disputes(&e)
    }
    pub fn get_disputes_by_claimant(e: Env, claimant: Address) -> Vec<u32> {
        get_disputes_by_claimant(&e, claimant)
    }
    pub fn dispute_count(e: Env) -> u32 {
        crate::storage_types::bump_instance(&e);
        crate::storage_types::read_counter(&e, &crate::storage_types::DataKey::DisputeCount)
    }

    // --- Splitter ---
    /// Creates a split-payment order, locking `total_amount` for distribution to `recipients`.
    ///
    /// Returns a split ID.  Call `distribute` to execute the payment once the split is ready.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `sender` - Funding address; must `require_auth`.
    /// * `recipients` - List of `(address, basis_points)` entries. Basis points must sum to 10 000.
    /// * `total_amount` - Total positive token quantity to split.
    ///
    /// # Panics
    /// * `"InvalidAmount: must be positive"` — if `total_amount ≤ 0`.
    /// * `"InvalidSplit: basis points must sum to 10000"` — if the sum is incorrect.
    ///
    /// # Example
    /// ```rust,ignore
    /// let split_id = client.create_split(&sender, &recipients, &1_000_0000i128);
    /// ```
    pub fn create_split(e: Env, sender: Address, recipients: Vec<SplitRecipient>, total_amount: i128) -> u32 {
        sender.require_auth();
        split_create(&e, sender, recipients, total_amount)
    }
    pub fn split_to_escrow(
        e: Env,
        sender: Address,
        recipients: Vec<SplitRecipient>,
        total_amount: i128,
        expiry_ledger: u32,
    ) -> Vec<u32> {
        sender.require_auth();
        require_positive_amount(total_amount);

        if recipients.is_empty() {
            panic!("recipients list cannot be empty");
        }
        if recipients.len() > 20 {
            panic!("TooManyRecipients: maximum 20 recipients allowed");
        }

        let mut total_bps: u32 = 0;
        for i in 0..recipients.len() {
            let r = recipients.get(i).unwrap();
            if r.share_bps == 0 {
                panic!("recipient share_bps cannot be zero");
            }
            for j in (i + 1)..recipients.len() {
                if r.address == recipients.get(j).unwrap().address {
                    panic!("duplicate recipient address");
                }
            }
            total_bps += r.share_bps;
        }
        if total_bps != 10000 {
            panic!("InvalidShares: recipient shares must sum to exactly 10000 bps");
        }

        let mut escrow_ids = Vec::new(&e);
        let mut allocated: i128 = 0;
        let len = recipients.len();
        for i in 0..len {
            let recipient = recipients.get(i).unwrap();
            let share = if i == len - 1 {
                total_amount - allocated
            } else {
                total_amount * recipient.share_bps as i128 / 10000
            };
            allocated += share;

            let escrow_id = escrow_create(&e, sender.clone(), recipient.address.clone(), share, expiry_ledger);
            escrow_ids.push_back(escrow_id);
        }

        e.events().publish(
            (soroban_sdk::Symbol::new(&e, "split_escrow"), sender),
            total_amount,
        );

        escrow_ids
    }
    /// Distributes a previously created split, transferring each recipient's share.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `caller` - The original sender or admin; must `require_auth`.
    /// * `split_id` - ID returned by `create_split`.
    ///
    /// # Panics
    /// * `"SplitNotFound"` — if no split exists with the given ID.
    /// * `"Unauthorized"` — if `caller` did not create the split.
    /// * `"AlreadyDistributed"` — if the split has already been executed.
    ///
    /// # Example
    /// ```rust,ignore
    /// client.distribute(&sender, &split_id);
    /// ```
    pub fn distribute(e: Env, caller: Address, split_id: u32) {
        caller.require_auth();
        crate::splitter::distribute(&e, caller, split_id);
    }
    pub fn cancel_split(e: Env, caller: Address, split_id: u32) {
        caller.require_auth();
        crate::splitter::cancel_split(&e, caller, split_id);
    }
    pub fn get_split(e: Env, split_id: u32) -> SplitRecord {
        split_get(&e, split_id)
    }
    pub fn replace_split_recipient(e: Env, sender: Address, split_id: u32, old_recipient: Address, new_recipient: Address) {
        sender.require_auth();
        crate::splitter::replace_split_recipient(&e, sender, split_id, old_recipient, new_recipient);
    }
    pub fn get_splits_for_recipient(e: Env, recipient: Address) -> Vec<u32> {
        split_get_for_recipient(&e, recipient)
    }
    pub fn split_count(e: Env) -> u32 {
        crate::storage_types::bump_instance(&e);
        crate::storage_types::read_counter(&e, &crate::storage_types::DataKey::SplitCount)
    }
    pub fn splitter_stats(e: Env) -> SplitterStats {
        crate::storage_types::bump_instance(&e);
        split_stats(&e)
    }

    // --- Recurring Payments ---
    /// Sets up a recurring payment from `payer` to `payee`.
    ///
    /// Returns a recurring-payment ID.  Call `execute_recurring` each time a
    /// payment is due (when the current ledger ≥ last execution + `interval`).
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `payer` - Address charged on each execution; must `require_auth`.
    /// * `payee` - Address receiving the periodic payment.
    /// * `amount` - Positive token quantity per execution cycle.
    /// * `interval` - Number of ledgers between executions (e.g. `17_280` ≈ 1 day).
    ///
    /// # Panics
    /// * `"InvalidAmount: must be positive"` — if `amount ≤ 0`.
    /// * `"InvalidInterval"` — if `interval` is zero.
    ///
    /// # Example
    /// ```rust,ignore
    /// let id = client.setup_recurring(&payer, &payee, &50_0000i128, &17_280u32);
    /// ```
    pub fn setup_recurring(e: Env, payer: Address, payee: Address, amount: i128, interval: u32) -> u32 {
        setup_recurring(&e, payer, payee, amount, interval)
    }
    /// Executes one cycle of a recurring payment if the interval has elapsed.
    ///
    /// Can be called by anyone (permissionless keeper pattern); it checks internally
    /// that the current ledger is past the scheduled next-execution ledger.
    ///
    /// # Arguments
    /// * `e` - Soroban execution environment.
    /// * `recurring_id` - ID returned by `setup_recurring`.
    ///
    /// # Panics
    /// * `"RecurringNotFound"` — if no recurring payment exists with the given ID.
    /// * `"NotDueYet"` — if the interval has not elapsed since the last execution.
    /// * `"Paused"` — if the contract is paused.
    /// * `"RecurringPaused"` — if this specific recurring payment has been paused.
    /// * `"InsufficientBalance"` — if the payer lacks funds.
    ///
    /// # Example
    /// ```rust,ignore
    /// client.execute_recurring(&recurring_id);
    /// ```
    pub fn execute_recurring(e: Env, recurring_id: u32) {
        execute_recurring(&e, recurring_id)
    }
    pub fn cancel_recurring(e: Env, caller: Address, recurring_id: u32) {
        cancel_recurring(&e, caller, recurring_id)
    }
    pub fn pause_recurring(e: Env, caller: Address, recurring_id: u32) {
        pause_recurring(&e, caller, recurring_id)
    }
    pub fn resume_recurring(e: Env, caller: Address, recurring_id: u32) {
        resume_recurring(&e, caller, recurring_id)
    }
    pub fn amend_recurring(e: Env, caller: Address, recurring_id: u32, new_amount: Option<i128>, new_interval: Option<u32>) {
        amend_recurring(&e, caller, recurring_id, new_amount, new_interval)
    }
    pub fn transfer_recurring_payer(
        e: Env,
        old_payer: Address,
        new_payer: Address,
        recurring_id: u32,
    ) {
        transfer_recurring_payer(&e, old_payer, new_payer, recurring_id)
    }
    pub fn get_recurring(e: Env, recurring_id: u32) -> RecurringRecord {
        get_recurring(&e, recurring_id)
    }
    pub fn get_recurring_by_payer(e: Env, payer: Address) -> Vec<u32> {
        get_recurring_by_payer(&e, payer)
    }
    pub fn recurring_count_for_payer(e: Env, payer: Address) -> u32 {
        recurring_count_for_payer(&e, payer)
    }
    pub fn recurring_count(e: Env) -> u32 {
        crate::storage_types::bump_instance(&e);
        crate::storage_types::read_counter(&e, &crate::storage_types::DataKey::RecurringCount)
    }
    pub fn get_next_execution_ledger(e: Env, recurring_id: u32) -> u32 {
        get_next_execution_ledger(&e, recurring_id)
    }
    pub fn is_executable(e: Env, recurring_id: u32) -> bool {
        is_executable(&e, recurring_id)
    }

    /// Returns the first active escrow ID between depositor and beneficiary, or None.
    pub fn escrow_between(e: Env, depositor: Address, beneficiary: Address) -> Option<u32> {
        escrow_between_fn(&e, depositor, beneficiary)
    }

    /// Batch-cancels up to 20 recurring payments owned by payer atomically.
    pub fn cancel_recurring_batch(e: Env, payer: Address, recurring_ids: Vec<u32>) {
        crate::recurring::cancel_recurring_batch(&e, payer, recurring_ids)
    }
}
