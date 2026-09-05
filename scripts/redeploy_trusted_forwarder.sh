#!/usr/bin/env bash
# Targeted redeploy of ONLY trusted-forwarder, after adding execute_batch (atomic multi-call
# dispatch under one signature and one nonce). A new function can't appear on an
# already-deployed contract's WASM — Soroban has no post-deploy upgrade path here — so the
# currently-deployed instance genuinely doesn't have execute_batch and needs replacing for
# that feature to be live and testable on testnet. execute_forwarded's ABI is unchanged, so
# this is additive, not breaking, but it still requires a fresh deploy either way. Every
# other contract in this workspace is unchanged and keeps its existing testnet address —
# this script does not touch them.
set -euo pipefail

NETWORK="${STELLAR_NETWORK:-testnet}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEPLOYMENTS_FILE="$REPO_ROOT/deployments/$NETWORK.json"

if [ ! -f "$DEPLOYMENTS_FILE" ]; then
  echo "Expected an existing $DEPLOYMENTS_FILE to merge into — run scripts/deploy.sh first if this is a fresh environment." >&2
  exit 1
fi
if ! command -v jq &> /dev/null; then
  echo "This script needs 'jq' to safely merge the new address into $DEPLOYMENTS_FILE (apt install jq / brew install jq)." >&2
  exit 1
fi

echo "Redeploying trusted-forwarder to Stellar $NETWORK (all other contracts untouched)..."

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
cargo build --release --target wasm32v1-none --package trusted-forwarder

OLD_ID=$(jq -r '.contracts.trusted_forwarder' "$DEPLOYMENTS_FILE")
NEW_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/trusted_forwarder.wasm \
  --source deployer \
  --network "$NETWORK")
stellar contract invoke --id "$NEW_ID" --source deployer --network "$NETWORK" \
  -- init --admin "$DEPLOYER_ADDR" --version v1

TMP_FILE="$(mktemp)"
jq \
  --arg new_id "$NEW_ID" \
  --arg old_id "$OLD_ID" \
  --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '.contracts.trusted_forwarder = $new_id
   | .notes.trusted_forwarder = ("Redeployed " + $ts + " after adding execute_batch (atomic multi-call dispatch under one signature/nonce) — " + $old_id + " was the pre-feature instance and lacks execute_batch entirely, since Soroban has no way to add a function to already-deployed WASM. execute_forwarded'"'"'s own ABI is unchanged.")' \
  "$DEPLOYMENTS_FILE" > "$TMP_FILE"
mv "$TMP_FILE" "$DEPLOYMENTS_FILE"

if [ -f "$REPO_ROOT/README.md" ] && grep -q "$OLD_ID" "$REPO_ROOT/README.md"; then
  sed -i "s/$OLD_ID/$NEW_ID/g" "$REPO_ROOT/README.md"
  echo "Patched README.md's deployed-addresses table."
fi

echo ""
echo "Redeployed trusted_forwarder: $OLD_ID -> $NEW_ID"
echo "Updated: $DEPLOYMENTS_FILE"
if [ -f "$REPO_ROOT/README.md" ] && grep -q "$NEW_ID" "$REPO_ROOT/README.md"; then
  echo "Updated: README.md"
fi
echo ""
echo "Next steps (not done by this script):"
echo "  - grep this repo for the old address ($OLD_ID) to confirm nothing else references it"
echo "  - review: git -C \"$REPO_ROOT\" diff"
echo "  - git add deployments/$NETWORK.json README.md && git commit"
