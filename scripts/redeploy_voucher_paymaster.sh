#!/usr/bin/env bash
# Targeted redeploy of ONLY voucher-paymaster, after the sponsor-scoping (Used key) and
# front-running (user.require_auth) fixes. Every other contract in this workspace is
# unchanged and keeps its existing testnet address — this script does not touch them.
#
# voucher-paymaster has no initialize() (stateless per-voucher checks — see its own
# doc comments), so this is just build + deploy + record. No demo data existed on the
# pre-fix instance either (deployments/testnet.json's own notes: "No voucher batch is
# registered yet"), so there is nothing to re-register.
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

echo "Redeploying voucher-paymaster to Stellar $NETWORK (all other contracts untouched)..."

if ! stellar keys address deployer >/dev/null 2>&1; then
  echo "No 'deployer' identity found. This script expects the SAME deployer used for the original deployment (deployments/$NETWORK.json's \"deployer\" field), not a fresh one, since a new deployer would still work but would be a needless identity change." >&2
  exit 1
fi
DEPLOYER_ADDR=$(stellar keys address deployer)
EXPECTED_DEPLOYER=$(jq -r '.deployer' "$DEPLOYMENTS_FILE")
if [ "$DEPLOYER_ADDR" != "$EXPECTED_DEPLOYER" ]; then
  echo "WARNING: local 'deployer' identity ($DEPLOYER_ADDR) does not match the deployer recorded in $DEPLOYMENTS_FILE ($EXPECTED_DEPLOYER). Continuing anyway, but the new contract's admin/deployer will differ from the rest of the workspace's contracts." >&2
fi

cd "$REPO_ROOT"
cargo build --release --target wasm32v1-none --package voucher-paymaster

OLD_ID=$(jq -r '.contracts.voucher_paymaster' "$DEPLOYMENTS_FILE")
NEW_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/voucher_paymaster.wasm \
  --source deployer \
  --network "$NETWORK")

TMP_FILE="$(mktemp)"
jq \
  --arg new_id "$NEW_ID" \
  --arg old_id "$OLD_ID" \
  --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '.contracts.voucher_paymaster = $new_id
   | .notes.voucher_paymaster = ("Redeployed " + $ts + " after fixing sponsor-scoping (Used key now keyed by (sponsor, voucher_id), not voucher_id alone) and adding user.require_auth() to close a front-running/griefing hole in validate_voucher() — " + $old_id + " was the pre-fix instance and is stale. No voucher batch is registered yet on this new instance; register_voucher_batch must be called by a real sponsor before any voucher can be redeemed.")' \
  "$DEPLOYMENTS_FILE" > "$TMP_FILE"
mv "$TMP_FILE" "$DEPLOYMENTS_FILE"

if [ -f "$REPO_ROOT/README.md" ] && grep -q "$OLD_ID" "$REPO_ROOT/README.md"; then
  sed -i "s/$OLD_ID/$NEW_ID/g" "$REPO_ROOT/README.md"
  echo "Patched README.md's deployed-addresses table."
fi

echo ""
echo "Redeployed voucher_paymaster: $OLD_ID -> $NEW_ID"
echo "Updated: $DEPLOYMENTS_FILE"
if [ -f "$REPO_ROOT/README.md" ] && grep -q "$NEW_ID" "$REPO_ROOT/README.md"; then
  echo "Updated: README.md"
fi
echo ""
echo "Next steps (not done by this script):"
echo "  - grep this repo for the old address ($OLD_ID) to confirm nothing else references it"
echo "  - review: git -C \"$REPO_ROOT\" diff"
echo "  - git add deployments/$NETWORK.json README.md && git commit"
