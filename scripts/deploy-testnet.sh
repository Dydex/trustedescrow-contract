#!/usr/bin/env bash
# Deploy the TrustEscrow factory to Stellar testnet.
#
# Uploads the escrow WASM, deploys the factory pointing at that hash,
# allowlists the settlement token, and writes the results to
# deployments/testnet.env.
#
# Requires the Stellar CLI (https://developers.stellar.org/docs/tools/cli) and
# a funded testnet identity.
#
# Usage:
#   SOURCE=admin ARBITRATOR=G... FEE_RECIPIENT=G... TOKEN=C... \
#     scripts/deploy-testnet.sh
#
# FEE_BPS defaults to 150 (1.5%). The contract caps it at 1000.

set -euo pipefail

: "${SOURCE:?set SOURCE to a stellar CLI identity or secret key}"
: "${ARBITRATOR:?set ARBITRATOR to the arbitrator address}"
: "${FEE_RECIPIENT:?set FEE_RECIPIENT to the fee recipient address}"
: "${TOKEN:?set TOKEN to the settlement token contract id (e.g. testnet USDC SAC)}"
FEE_BPS="${FEE_BPS:-150}"
NETWORK="${NETWORK:-testnet}"

cd "$(dirname "$0")/.."
command -v stellar >/dev/null || { echo "stellar CLI not found" >&2; exit 1; }

make build

WASM_DIR=target/wasm32v1-none/release
ADMIN=$(stellar keys address "$SOURCE" 2>/dev/null || echo "$SOURCE")

echo "Uploading escrow WASM..."
ESCROW_WASM_HASH=$(stellar contract upload \
  --wasm "$WASM_DIR/trustescrow_escrow.wasm" \
  --source "$SOURCE" --network "$NETWORK")

echo "Deploying factory..."
FACTORY_ID=$(stellar contract deploy \
  --wasm "$WASM_DIR/trustescrow_factory.wasm" \
  --source "$SOURCE" --network "$NETWORK" \
  -- \
  --config "{\"admin\":\"$ADMIN\",\"escrow_wasm_hash\":\"$ESCROW_WASM_HASH\",\"arbitrator\":\"$ARBITRATOR\",\"fee_recipient\":\"$FEE_RECIPIENT\",\"fee_bps\":$FEE_BPS}")

echo "Allowlisting settlement token..."
stellar contract invoke --id "$FACTORY_ID" \
  --source "$SOURCE" --network "$NETWORK" \
  -- allow_token --token "$TOKEN" --allowed true

mkdir -p deployments
cat > "deployments/$NETWORK.env" <<EOF
NETWORK=$NETWORK
FACTORY_ID=$FACTORY_ID
ESCROW_WASM_HASH=$ESCROW_WASM_HASH
ADMIN=$ADMIN
ARBITRATOR=$ARBITRATOR
FEE_RECIPIENT=$FEE_RECIPIENT
FEE_BPS=$FEE_BPS
TOKEN=$TOKEN
EOF

echo
echo "Factory:          $FACTORY_ID"
echo "Escrow WASM hash: $ESCROW_WASM_HASH"
echo "Written to deployments/$NETWORK.env. Pin the WASM hash in the SDK."
