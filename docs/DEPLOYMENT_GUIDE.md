# soroban-gasless-contracts Deployment Guide

Deploys `trusted_forwarder`, `token_paymaster`, `voucher_paymaster`, `gas_estimator`, and
`account_abstraction_wallet` to Stellar testnet. All five pass their real test suite
(`cargo test --all`, 14 tests — see the repo README) before this guide is relevant —
deployment doesn't substitute for that.

## Prerequisites
- **Stellar CLI**: `cargo install --locked stellar-cli`
- **Rust Wasm target**: `rustup target add wasm32-unknown-unknown` (this repo is on
  soroban-sdk 21.x, which predates the `wasm32v1-none` target used elsewhere in this org)
- **OpenSSL** (to generate the demo passkey below)

## Network
- **Network**: `testnet`
- **RPC URL**: `https://soroban-testnet.stellar.org:443`
- **Passphrase**: `"Test SDF Network ; September 2015"`

## 1. Generate a demo passkey

`account-abstraction-wallet.init()` requires a real 65-byte uncompressed secp256r1 public
key — the contract has no code path that accepts a fabricated or empty one. This generates
a real keypair for exercising the deployed contract; it isn't tied to any actual WebAuthn
authenticator, and the resulting wallet should be treated as a demo instance, not a
real user's account.

```bash
openssl ecparam -name prime256v1 -genkey -noout -out demo_wallet_passkey.pem
export DEMO_PASSKEY_PUBKEY=$(openssl ec -in demo_wallet_passkey.pem -pubout -outform DER 2>/dev/null | tail -c 65 | xxd -p -c 65)
```

Keep `demo_wallet_passkey.pem` if you want to later demonstrate a real signed
`__check_auth` call against the deployed wallet (see `contracts/account-abstraction-wallet/src/test.rs`
for the exact WebAuthn-shaped signing flow this contract expects). Do not commit it —
it's excluded via `.gitignore`.

## 2. Deploy

```bash
bash scripts/deploy.sh
```

This generates and friendbot-funds a `deployer` testnet identity if one doesn't already
exist, resolves testnet's real native XLM Stellar Asset Contract ID (used as
`token_paymaster`'s fee token — not a fabricated address), builds and deploys all five
contracts, and initializes the three that need it. Resulting contract IDs are written to
`deployments/testnet.json`.

## What this does NOT set up

- `token_paymaster` is initialized with a `fee_per_tx` of 100 stroops as a placeholder —
  adjust for real usage.
- No relayer or forwarder allowlisting is configured; `trusted_forwarder` starts with just
  an admin.
- The deployed wallet's owner is the `deployer` identity itself, purely so the contract has
  a valid admin/recovery address — a real deployment would set this to the actual end
  user's account.
