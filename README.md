# 📜 Soroban Gasless Smart Contracts (`soroban-gasless-contracts`)

[![Rust](https://img.shields.io/badge/Rust-1.78+-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Soroban SDK](https://img.shields.io/badge/Soroban_SDK-v21.2.0-7C3AED?style=for-the-badge&logo=rust&logoColor=white)](https://soroban.stellar.org/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg?style=for-the-badge)](./LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-Welcome-brightgreen.svg?style=for-the-badge)](./CONTRIBUTING.md)

**High-performance, secure Soroban WASM smart contracts powering Stellar Gasless Meta-Transactions, Account Abstraction (WebAuthn Passkeys), and Paymaster Fee Sponsorship on the Stellar Network.**

This repository contains the **On-Chain Smart Contract Trust Anchor** for the [`stellar-gasless-net`](https://github.com/stellar-gasless-net) ecosystem.

---

## 🏛️ Smart Contract System Architecture

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                      soroban-gasless-contracts WASM Suite                        │
│                                                                                  │
│  ┌───────────────────────────────┐              ┌─────────────────────────────┐  │
│  │       trusted-forwarder       │              │       token-paymaster       │  │
│  │ - SorobanDomainSeparator      │              │ - SAC USDC Dynamic Swap     │  │
│  │ - Nonce Bitmap Replay Guard   │              │ - Fee Refund & Deductions   │  │
│  │ - Atomic Batch Execution      │              │ - Discount Tier Policies    │  │
│  └───────────────┬───────────────┘              └──────────────┬──────────────┘  │
│                  │                                             │                 │
│                  └──────────────────────┬──────────────────────┘                 │
│                                         │                                        │
│                                         v                                        │
│  ┌───────────────────────────────┐              ┌─────────────────────────────┐  │
│  │  account-abstraction-wallet   │              │      voucher-paymaster      │  │
│  │ - WebAuthn secp256r1 Passkey  │              │ - Single-Use ECDSA Coupons  │  │
│  │ - Delegated Session Keys      │              │ - Merkle Inclusion Proofs   │  │
│  └───────────────────────────────┘              └─────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

---

## 🛠️ Detailed Contract Capabilities & Mechanics

### 1. `trusted-forwarder` Contract
* **EIP-712 Style `SorobanDomainSeparator`**: Binds signatures to the specific Stellar Network (`Testnet` / `Mainnet`), channel, and contract ID to prevent cross-chain signature replay attacks.
* **Sequence Nonce Bitmaps**: Tracks executed nonces per user address, preventing transaction reordering or double-spending.
* **Atomic Batch Multi-Call (`execute_batch`)**: Bundles multiple Soroban contract calls into a single atomic transaction.

### 2. `token-paymaster` Contract
* **SAC USDC Auto Fee Swap**: Automatically accepts Soroban Asset Contract (SAC) tokens like USDC from the user and swaps them to reimburse the Relayer's XLM gas pool.
* **Fee Refund Safeguard**: Refunds excess gas if execution consumes less Stroops than estimated.

### 3. `account-abstraction-wallet` Contract
* **Native `secp256r1` Passkey Verification**: Validates browser WebAuthn biometric signatures (TouchID / FaceID) natively inside Soroban WASM.
* **Delegated Session Keys**: Grants dApps temporary scoped execution permissions with spending limits and expiration timestamps.

### 4. `voucher-paymaster` Contract
* **Merkle Proof Coupons**: Validates cryptographic promotional vouchers off-chain, granting targeted users 100% free transactions.

---

## 🧪 Local Testing & Verification

All contracts are fully unit-tested using Soroban SDK `Env` mocking and pinned Cargo dependencies (`resolver = "2"`):

```bash
# 1. Run complete unit test suite
cargo test --all

# 2. Build optimized release WASM binaries
cargo build --all --target wasm32-unknown-unknown --release
```

---

## 🤝 Contributing & `CONTRIBUTING.md` Guidelines

We welcome contributions from developer community members, security researchers, and Stellar builders!

Before submitting pull requests, please read our dedicated **[`CONTRIBUTING.md`](./CONTRIBUTING.md)** guide:
* 📖 **[Smart Contracts Contributor Guide](./CONTRIBUTING.md)**
* 🛡️ **[Security Policy](./SECURITY.md)**

### 📌 Pull Request Checklist:
- [ ] Claim an issue tagged `good first issue`, `intermediate`, or `advanced`.
- [ ] Ensure `cargo test --all` passes 100% cleanly with 0 compiler warnings.
- [ ] Follow Conventional Commits format (`feat: ...`, `fix: ...`, `docs: ...`).

---

## 🔮 Future Contract Improvements & Upgrade Roadmap

- [ ] **Soroban Dynamic Gas Estimator Oracle**: Real-time gas price metering algorithm auto-adjusting fee caps based on Stellar ledger congestion.
- [ ] **Multi-Sig Paymaster Governance Vaults**: Multi-signature approval thresholds for depositing and withdrawing XLM gas reserves.
- [ ] **Upgradeable WASM Proxy Pattern**: Standardized proxy contract pattern for seamless contract upgrades.
