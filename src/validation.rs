/// Panics unless `amount` is strictly positive.
///
/// Amount validation is repeated across the token, escrow, splitter and
/// recurring paths. Routing every one of them through a single helper keeps the
/// rejection message byte-for-byte identical, which is what indexers and
/// clients match on when they classify a failed transaction.
///
/// # Panics
///
/// Panics with an `InvalidAmount` message naming `amount` when `amount <= 0`.
/// Zero is rejected as well: every caller in this contract either credits or
/// debits a ledger, and a zero movement would burn gas to change nothing.
pub fn require_positive_amount(amount: i128) {
    assert!(
        amount > 0,
        "InvalidAmount: amount must be strictly positive, got {}",
        amount
    );
}
