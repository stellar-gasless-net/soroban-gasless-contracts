#!/usr/bin/env bash
# Targeted redeploy of ONLY account-abstraction-wallet, after adding real per-function
# session-key scoping and an optional cumulative SEP-41 spend cap. add_session_key gained
# two parameters (allowed_functions, spend_cap), so this is a breaking ABI change — the
# currently-deployed instance's add_session_key genuinely doesn't match this new signature.
# Every other contract in this workspace is unchanged and keeps its existing testnet address
# — this script does not touch them.
#
# Needs the SAME DEMO_PASSKEY_PUBKEY setup as scripts/deploy.sh — see
# docs/DEPLOYMENT_GUIDE.md for how to generate a real secp256r1 demo key. This redeploy uses
# a fresh demo owner/passkey rather than trying to carry over any state from the old
# instance, matching this workspace's existing precedent (account_abstraction_wallet's own
# prior redeploy note already used a fresh demo key the same way).
set -euo pipefail

NETWORK="${STELLAR_NETWORK:-testnet}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEPLOYMENTS_FILE="$REPO_ROOT/deployments/$NETWORK.json"

if [ -z "${DEMO_PASSKEY_PUBKEY:-}" ]; then
  echo "DEMO_PASSKEY_PUBKEY is not set — see docs/DEPLOYMENT_GUIDE.md for how to generate a real secp256r1 demo key." >&2
  exit 1
fi
if [ ! -f "$DEPLOYMENTS_FILE" ]; then
  echo "Expected an existing $DEPLOYMENTS_FILE to merge into — run scripts/deploy.sh first if this is a fresh environment." >&2
  exit 1
fi
if ! command -v jq &> /dev/null; then
  echo "This script needs 'jq' to safely merge the new address into $DEPLOYMENTS_FILE (apt install jq / brew install jq)." >&2
  exit 1
fi

echo "Redeploying account-abstraction-wallet to Stellar $NETWORK (all other contracts untouched)..."

if ! stellar keys address deployer >/dev/null 2>&1; then
  echo "No 'deployer' identity found. This script expects the SAME deployer used for the original deployment (deployments/$NETWORK.json's \"deployer\" field), not a fresh one." >&2
  exit 1
fi
stellar keys fund deployer --network "$NETWORK" || true
DEPLOYER_ADDR=$(stellar keys address deployer)
EXPECTED_DEPLOYER=$(jq -r '.deployer' "$DEPLOYMENTS_FILE")
if [ "$DEPLOYER_ADDR" != "$EXPECTED_DEPLOYER" ]; then
  echo "WARNING: local 'deployer' identity ($DEPLOYER_ADDR) does not match the deployer recorded in $DEPLOYMENTS_FILE ($EXPECTED_DEPLOYER). Continuing anyway, but the new contract's admin/deployer will differ from the rest of the workspace's contracts." >&2
fi

cd "$REPO_ROOT"
cargo build --release --target wasm32v1-none --package account-abstraction-wallet

OLD_ID=$(jq -r '.contracts.account_abstraction_wallet' "$DEPLOYMENTS_FILE")
NEW_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/account_abstraction_wallet.wasm \
  --source deployer \
  --network "$NETWORK")
stellar contract invoke --id "$NEW_ID" --source deployer --network "$NETWORK" \
  -- init --owner "$DEPLOYER_ADDR" --passkey_pubkey "$DEMO_PASSKEY_PUBKEY"

TMP_FILE="$(mktemp)"
jq \
  --arg new_id "$NEW_ID" \
  --arg old_id "$OLD_ID" \
  --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '.contracts.account_abstraction_wallet = $new_id
   | .notes.account_abstraction_wallet = ("Redeployed " + $ts + " after adding real per-function session-key scoping and an optional cumulative SEP-41 spend cap — add_session_key gained allowed_functions and spend_cap parameters, a breaking ABI change, so " + $old_id + " is stale and cannot be reused as-is. Initialized with a fresh demo secp256r1 passkey (DEMO_PASSKEY_PUBKEY), not a real WebAuthn authenticator. No session key is registered yet on this new instance.")' \
  "$DEPLOYMENTS_FILE" > "$TMP_FILE"
mv "$TMP_FILE" "$DEPLOYMENTS_FILE"

if [ -f "$REPO_ROOT/README.md" ] && grep -q "$OLD_ID" "$REPO_ROOT/README.md"; then
  sed -i "s/$OLD_ID/$NEW_ID/g" "$REPO_ROOT/README.md"
  echo "Patched README.md's deployed-addresses table."
fi

echo ""
echo "Redeployed account_abstraction_wallet: $OLD_ID -> $NEW_ID"
echo "Updated: $DEPLOYMENTS_FILE"
if [ -f "$REPO_ROOT/README.md" ] && grep -q "$NEW_ID" "$REPO_ROOT/README.md"; then
  echo "Updated: README.md"
fi
echo ""
echo "Next steps (not done by this script):"
echo "  - grep this repo for the old address ($OLD_ID) to confirm nothing else references it"
echo "  - review: git -C \"$REPO_ROOT\" diff"
echo "  - git add deployments/$NETWORK.json README.md && git commit"
