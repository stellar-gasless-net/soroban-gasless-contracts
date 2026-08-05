# `soroban-gasless-contracts`

Soroban smart contracts powering the **Stellar Gasless Meta-Transaction Network**.

## 📑 Included Contracts

1. **`trusted-forwarder`**: Validates off-chain user authorization entries and executes target contract invocations while managing nonces and deadline limits.
2. **`token-paymaster`**: Allows users to pay gas execution costs using custom SAC tokens (e.g. USDC, ARST, EURC).
3. **`voucher-paymaster`**: Sponsoring engine validating single-use ECDSA coupons or promotional gas vouchers signed by dApp developers.

## 🛠️ Build & Test

```bash
cargo build --target wasm32-unknown-unknown --release
cargo test
```
