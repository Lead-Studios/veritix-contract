//! Emergency withdrawal of stranded tokens from the contract.

use crate::admin;
use crate::escrow;
use soroban_sdk::{token, Address, Env};

/// Emitted on an emergency withdrawal.
#[soroban_sdk::contractevent(data_format = "vec")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyWithdrawal {
    #[topic]
    pub admin: Address,
    #[topic]
    pub recipient: Address,
    #[topic]
    pub token: Address,
    pub amount: i128,
}

/// Allows admin to withdraw stranded tokens from the contract without touching escrowed funds.
///
/// # Panics
///
/// Panics when caller is not admin, or when withdrawal would dip into value locked in escrows,
/// or when `amount <= 0`.
pub fn emergency_withdraw(
    e: &Env,
    admin: &Address,
    recipient: &Address,
    token: &Address,
    amount: i128,
) {
    admin::check_admin(e, admin);
    crate::validation::require_positive_amount(amount);

    let client = token::TokenClient::new(e, token);
    let contract_address = e.current_contract_address();
    let contract_balance = client.balance(&contract_address);
    let locked = escrow::value_locked(e);

    let available = contract_balance.saturating_sub(locked);
    if amount > available {
        panic!(
            "InsufficientUnescrowedFunds: requested {} but only {} is unescrowed (balance: {}, locked: {})",
            amount, available, contract_balance, locked
        );
    }

    client.transfer(&contract_address, recipient, &amount);

    EmergencyWithdrawal {
        admin: admin.clone(),
        recipient: recipient.clone(),
        token: token.clone(),
        amount,
    }
    .publish(e);
}