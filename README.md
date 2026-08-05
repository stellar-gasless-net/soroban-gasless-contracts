# ⚡ Soroban Gasless Smart Contracts (`soroban-gasless-contracts`)

[![Rust](https://img.shields.io/badge/Rust-1.78%2B-orange.svg?style=for-the-badge&logo=rust)](https://www.rust-lang.org/)
[![Soroban SDK](https://img.shields.io/badge/Soroban--SDK-v21.0.0-purple.svg?style=for-the-badge&logo=stellar)](https://soroban.stellar.org/)
[![WASM](https://img.shields.io/badge/Target-WASM32-blue.svg?style=for-the-badge)](https://webassembly.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](LICENSE)

High-performance, secure Soroban WASM smart contracts powering **Stellar Gasless Meta-Transactions**, **Account Abstraction**, and **Paymaster Fee Sponsorship** on the Stellar Network.

---

## 🏛️ Smart Contract Suite Overview

```
                      ┌────────────────────────────────────────┐
                      │    Off-Chain Signed User Authorization │
                      └───────────────────┬────────────────────┘
                                          │
                                          v
                      ┌────────────────────────────────────────┐
                      │        trusted-forwarder               │
                      │  - Nonce Bitmaps & Sequential Checks   │
                      │  - EIP-712 Style Domain Separator      │
                      │  - Atomic Batch Execution              │
                      └───────────────────┬────────────────────┘
                                          │
        ┌─────────────────────────────────┼─────────────────────────────────┐
        │                                 │                                 │
        v                                 v                                 v
┌───────────────┐                 ┌───────────────┐                 ┌───────────────┐
│token-paymaster│                 │voucher-pmaster│                 │account-account│
│- SAC Fee Swap │                 │- Merkle Proofs│                 │- Passkeys     │
│- Sliding Rate │                 │- ECDSA Coupons│                 │- Session Keys │
└───────────────┘                 └───────────────┘                 └───────────────┘
```

---

## 📑 Contract Modules & Specifications

| Contract | Path | Purpose | Key Features |
| :--- | :--- | :--- | :--- |
| **`trusted-forwarder`** | [`contracts/trusted-forwarder`](./contracts/trusted-forwarder) | Core Meta-Tx Forwarder | Replay protection, Nonce tracking, Batch execution (`execute_batch`), Domain Separator verification |
| **`token-paymaster`** | [`contracts/token-paymaster`](./contracts/token-paymaster) | SAC Token Fee Sponsorship | Pays gas in USDC/EURC, sliding-scale volume discounts, emergency pause, fee refunds |
| **`voucher-paymaster`** | [`contracts/voucher-paymaster`](./contracts/voucher-paymaster) | Promotional Coupon Sponsoring | ECDSA single-use vouchers, Merkle whitelist proof validation, user quota tracking |
| **`account-abstraction-wallet`** | [`contracts/account-abstraction-wallet`](./contracts/account-abstraction-wallet) | Smart Account Wallet | Browser WebAuthn / Passkeys (`secp256r1`), multi-owner recovery, session key delegation |
| **`gas-estimator`** | [`contracts/gas-estimator`](./contracts/gas-estimator) | Execution Gas Overhead Helper | Dry-run CPU instruction & memory byte footprint estimator |

---

## ⛽ Gas Benchmark & Overhead Analysis

| Execution Model | Native XLM Gas Cost | Paymaster Overhead | Net User XLM Spent |
| :--- | :--- | :--- | :--- |
| **Standard Soroban Call** | ~0.0000100 XLM | N/A | 0.0000100 XLM |
| **Gasless Forwarded Call** | ~0.0000125 XLM | Sponsored by Relayer | **0.0000000 XLM** |
| **SAC Token Gasless Call** | ~0.0000140 XLM | Domiciled in USDC | **0.0000000 XLM** (Deducted 0.001 USDC) |

---

## 🛠️ Build, Test & Deployment Guide

### Prerequisites
- [Rust & Cargo](https://rustup.rs/) (v1.78+)
- WASM target: `rustup target add wasm32-unknown-unknown`
- [Stellar CLI](https://developers.stellar.org/docs/tools/developer-tools/cli): `cargo install --locked stellar-cli`

### 1. Compile & Optimize WASM Contracts
```bash
cargo build --target wasm32-unknown-unknown --release
```

### 2. Run Comprehensive Unit Test Suite
```bash
cargo test --all
```

### 3. Automated Testnet Deployment Script
```bash
./scripts/deploy_testnet.sh
```

---

## 🛡️ Security Threat Model & Protections

1. **Replay Attack Protection**: Every user transaction increments a persistent on-chain nonce counter bound to the user address (`(symbol_short!("nonce"), user)`). Re-submitting an identical payload fails immediately.
2. **Deadline Limits**: Transactions contain an explicit unix timestamp `deadline`. If `ledger().timestamp() > deadline`, execution reverts.
3. **Domain Separation**: Forwarder validates network passphrase and contract domain to prevent cross-network or cross-contract replay.
4. **Emergency Pause**: Admins can trigger emergency circuit breakers on `token-paymaster` to pause gas spending in case of market volatility.

---

## 📄 License

Distributed under the MIT License. See `LICENSE` for details.
