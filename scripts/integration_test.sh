#!/usr/bin/env bash
# Integration test: deploys the contract to localnet and runs the full ticket purchase flow.
# Prerequisites: stellar CLI, a running localnet (see README.md for setup).
set -euo pipefail

NETWORK="${STELLAR_NETWORK:-standalone}"
RPC_URL="${STELLAR_RPC_URL:-http://localhost:8000/soroban/rpc}"
PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Standalone Network ; February 2017}"

WASM_PATH="../../target/wasm32v1-none/release/veritixpay_token.wasm"

echo "==> Building contract..."
make build

echo "==> Generating test identities..."
stellar keys generate admin    --network "$NETWORK" --rpc-url "$RPC_URL" --network-passphrase "$PASSPHRASE" 2>/dev/null || true
stellar keys generate buyer    --network "$NETWORK" --rpc-url "$RPC_URL" --network-passphrase "$PASSPHRASE" 2>/dev/null || true
stellar keys generate seller   --network "$NETWORK" --rpc-url "$RPC_URL" --network-passphrase "$PASSPHRASE" 2>/dev/null || true

ADMIN_ADDR=$(stellar keys address admin)
BUYER_ADDR=$(stellar keys address buyer)
SELLER_ADDR=$(stellar keys address seller)

echo "  admin:  $ADMIN_ADDR"
echo "  buyer:  $BUYER_ADDR"
echo "  seller: $SELLER_ADDR"

echo "==> Funding accounts via friendbot..."
curl -s "http://localhost:8000/friendbot?addr=$ADMIN_ADDR"  > /dev/null
curl -s "http://localhost:8000/friendbot?addr=$BUYER_ADDR"  > /dev/null
curl -s "http://localhost:8000/friendbot?addr=$SELLER_ADDR" > /dev/null

echo "==> Deploying contract..."
CONTRACT_ID=$(stellar contract deploy \
  --wasm "$WASM_PATH" \
  --source admin \
  --network "$NETWORK" \
  --rpc-url "$RPC_URL" \
  --network-passphrase "$PASSPHRASE")
echo "  contract: $CONTRACT_ID"

invoke() {
  stellar contract invoke \
    --id "$CONTRACT_ID" \
    --source "$1" \
    --network "$NETWORK" \
    --rpc-url "$RPC_URL" \
    --network-passphrase "$PASSPHRASE" \
    -- "${@:2}"
}

echo "==> Initializing contract..."
invoke admin initialize \
  --admin "$ADMIN_ADDR" \
  --name  "Veritix" \
  --symbol "VTX" \
  --decimal 7

echo "==> Minting 1000 VTX to buyer..."
invoke admin mint \
  --admin "$ADMIN_ADDR" \
  --to    "$BUYER_ADDR" \
  --amount 1000

echo "==> Creating escrow (buyer -> seller, 1000 VTX)..."
ESCROW_ID=$(invoke buyer create_escrow \
  --depositor    "$BUYER_ADDR" \
  --beneficiary  "$SELLER_ADDR" \
  --amount       1000 \
  --expiry_ledger 999999)
echo "  escrow_id: $ESCROW_ID"

echo "==> Releasing escrow..."
invoke seller release_escrow \
  --caller    "$SELLER_ADDR" \
  --escrow_id "$ESCROW_ID"

echo "==> Asserting seller balance == 1000..."
SELLER_BALANCE=$(invoke admin balance --id "$SELLER_ADDR")
if [ "$SELLER_BALANCE" != "1000" ]; then
  echo "FAIL: expected seller balance 1000, got $SELLER_BALANCE"
  exit 1
fi

echo "✅ Integration test passed. Seller balance: $SELLER_BALANCE"
