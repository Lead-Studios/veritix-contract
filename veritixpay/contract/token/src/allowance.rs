use crate::storage_types::{AllowanceDataKey, AllowanceValue, DataKey};
use soroban_sdk::{Address, Env};

pub fn read_allowance(e: &Env, from: Address, spender: Address) -> AllowanceValue {
    let key = DataKey::Allowance(AllowanceDataKey {
        from: from.clone(),
        spender: spender.clone(),
    });

    if let Some(allowance) = e
        .storage()
        .persistent()
        .get::<DataKey, AllowanceValue>(&key)
    {
        // Equal-to-current remains valid for the current ledger and expires
        // once the sequence advances past the stored expiration ledger.
        if allowance.expiration_ledger < e.ledger().sequence() {
            AllowanceValue {
                amount: 0,
                expiration_ledger: allowance.expiration_ledger,
            }
        } else {
            allowance
        }
    } else {
        AllowanceValue {
            amount: 0,
            expiration_ledger: 0,
        }
    }
}

pub fn write_allowance(
    e: &Env,
    from: Address,
    spender: Address,
    amount: i128,
    expiration_ledger: u32,
) {
    if expiration_ledger < e.ledger().sequence() {
        panic!("expiration ledger is in the past");
    }

    let key = DataKey::Allowance(AllowanceDataKey {
        from: from.clone(),
        spender: spender.clone(),
    });

    if amount == 0 {
        e.storage().persistent().remove(&key);
    } else {
        let allowance = AllowanceValue {
            amount,
            expiration_ledger,
        };
        e.storage().persistent().set(&key, &allowance);
    }
}

pub fn spend_allowance(e: &Env, from: Address, spender: Address, amount: i128) {
    let allowance = read_allowance(e, from.clone(), spender.clone());

    // Equal-to-current remains spendable during the current ledger only.
    if allowance.expiration_ledger < e.ledger().sequence() {
        panic!("allowance is expired");
    }

    if allowance.amount < amount {
        panic!("insufficient allowance");
    }

    write_allowance(
        e,
        from,
        spender,
        allowance.amount - amount,
        allowance.expiration_ledger,
    );
}
