#!/usr/bin/env bash
# Deploys every real contract in this workspace to Stellar testnet and records the
# resulting contract IDs in deployments/<network>.json. Requires the `stellar` CLI
# (https://developer.stellar.org/docs/tools/cli) to already be installed and on PATH.
#
# Two contracts need real init arguments that aren't just addresses we control:
#   - token-paymaster needs a real token contract. We use testnet's native XLM Stellar
#     Asset Contract (SAC), resolved via `stellar contract id asset`, not a fabricated one.
#   - account-abstraction-wallet needs a real secp256r1 passkey public key. DEMO_PASSKEY_PUBKEY
#     below must be set to a real, generated (not invented) 65-byte uncompressed public key —
#     see docs/DEPLOYMENT_GUIDE.md for how to generate one. This is a demo key for exercising
#     the deployed contract, not tied to any real WebAuthn authenticator.
set -euo pipefail

NETWORK="${STELLAR_NETWORK:-testnet}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$REPO_ROOT/deployments"
OUT_FILE="$OUT_DIR/$NETWORK.json"
mkdir -p "$OUT_DIR"

if [ -z "${DEMO_PASSKEY_PUBKEY:-}" ]; then
  echo "DEMO_PASSKEY_PUBKEY is not set — see docs/DEPLOYMENT_GUIDE.md for how to generate a real secp256r1 demo key." >&2
  exit 1
fi

echo "Deploying soroban-gasless-contracts to Stellar $NETWORK..."

if ! stellar keys ls | grep -q "^deployer$"; then
  echo "Generating deployer key..."
  stellar keys generate deployer --global
fi
stellar keys fund deployer --network "$NETWORK" || true
DEPLOYER_ADDR=$(stellar keys address deployer)

cd "$REPO_ROOT"
cargo build --release --target wasm32-unknown-unknown

WASM_DIR="target/wasm32-unknown-unknown/release"

deploy() {
  local wasm_name="$1"
  stellar contract deploy \
    --wasm "$WASM_DIR/$wasm_name.wasm" \
    --source deployer \
    --network "$NETWORK"
}

echo "Resolving testnet native XLM Stellar Asset Contract..."
NATIVE_TOKEN_ID=$(stellar contract id asset --asset native --network "$NETWORK")

echo "Deploying trusted_forwarder..."
FORWARDER_ID=$(deploy trusted_forwarder)
stellar contract invoke --id "$FORWARDER_ID" --source deployer --network "$NETWORK" \
  -- init --admin "$DEPLOYER_ADDR" --version v1

echo "Deploying token_paymaster..."
TOKEN_PAYMASTER_ID=$(deploy token_paymaster)
stellar contract invoke --id "$TOKEN_PAYMASTER_ID" --source deployer --network "$NETWORK" \
  -- init --admin "$DEPLOYER_ADDR" --token "$NATIVE_TOKEN_ID" --fee_per_tx 100

echo "Deploying voucher_paymaster (no init — stateless per-voucher checks)..."
VOUCHER_PAYMASTER_ID=$(deploy voucher_paymaster)

echo "Deploying gas_estimator (no init — pure function)..."
GAS_ESTIMATOR_ID=$(deploy gas_estimator)

echo "Deploying account-abstraction-wallet (demo instance)..."
WALLET_ID=$(deploy account_abstraction_wallet)
stellar contract invoke --id "$WALLET_ID" --source deployer --network "$NETWORK" \
  -- init --owner "$DEPLOYER_ADDR" --passkey_pubkey "$DEMO_PASSKEY_PUBKEY"

cat > "$OUT_FILE" <<EOF
{
  "network": "$NETWORK",
  "deployed_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "deployer": "$DEPLOYER_ADDR",
  "native_token": "$NATIVE_TOKEN_ID",
  "contracts": {
    "trusted_forwarder": "$FORWARDER_ID",
    "token_paymaster": "$TOKEN_PAYMASTER_ID",
    "voucher_paymaster": "$VOUCHER_PAYMASTER_ID",
    "gas_estimator": "$GAS_ESTIMATOR_ID",
    "account_abstraction_wallet": "$WALLET_ID"
  },
  "notes": {
    "account_abstraction_wallet": "Initialized with a demo secp256r1 passkey (DEMO_PASSKEY_PUBKEY), not a real WebAuthn authenticator."
  }
}
EOF

echo ""
echo "Deployed to $NETWORK — recorded in $OUT_FILE"
cat "$OUT_FILE"
