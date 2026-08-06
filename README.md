# 📜 Soroban Gasless Smart Contracts (`soroban-gasless-contracts`)

[![Rust](https://img.shields.io/badge/Rust-1.78+-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Soroban SDK](https://img.shields.io/badge/Soroban_SDK-v21.2.0-7C3AED?style=for-the-badge&logo=rust&logoColor=white)](https://soroban.stellar.org/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg?style=for-the-badge)](./LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-Welcome-brightgreen.svg?style=for-the-badge)](./CONTRIBUTING.md)

**High-performance, secure Soroban WASM smart contracts powering Stellar Gasless Meta-Transactions, Account Abstraction (WebAuthn Passkeys), and Paymaster Fee Sponsorship on the Stellar Network.**

This repository contains the core WASM smart contracts powering the [`stellar-gasless-net`](https://github.com/stellar-gasless-net) ecosystem.

---

## 🛠️ Smart Contracts Architecture

* **[`trusted-forwarder`](./contracts/trusted-forwarder)**: EIP-712-style `SorobanDomainSeparator`, nonce bitmap verification, deadline enforcement, atomic batch execution (`execute_batch`), and custom `#[contracterror]` enums.
* **[`token-paymaster`](./contracts/token-paymaster)**: Dynamic SAC (Stellar Asset Contract) token fee deduction (e.g. USDC), sliding-scale fee discount tiers, emergency pause control, and fee refund logic.
* **[`voucher-paymaster`](./contracts/voucher-paymaster)**: Single-use ECDSA coupon validation, quota per address, and Merkle tree inclusion proof verifications.
* **[`account-abstraction-wallet`](./contracts/account-abstraction-wallet)**: Native Smart Account contract supporting browser WebAuthn / Passkeys (`secp256r1`), multi-owner recovery, and session key delegation.
* **[`gas-estimator`](./contracts/gas-estimator)**: Soroban resource metering and overhead gas estimation contract.

---

## 🧪 Local Testing & Build Verification

All contracts are fully unit-tested using Soroban SDK `Env` mocking and pinned Cargo dependencies (`resolver = "2"`):

```bash
cargo test --all
```

To compile WASM release binaries:
```bash
cargo build --all --target wasm32-unknown-unknown --release
```

---

## 🤝 Contributing & Governance

Please read our enterprise contributor guidelines before submitting PRs:
* 📖 **[Smart Contracts Contributor Guide](./CONTRIBUTING.md)**
* 🛡️ **[Security Policy](./SECURITY.md)**
