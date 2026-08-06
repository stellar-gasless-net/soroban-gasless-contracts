# Contributing to Soroban Gasless Smart Contracts (`soroban-gasless-contracts`)

First off, thank you for taking the time to contribute! 🎉 

This repository houses the core **Soroban WASM Smart Contracts** powering **Stellar Gasless Network** (`stellar-gasless-net`). We welcome contributions from developer community members, security researchers, and Stellar ecosystem builders.

---

## 📋 Table of Contents

1. [Ecosystem & Repository Overview](#-ecosystem--repository-overview)
2. [Prerequisites & Development Setup](#-prerequisites--development-setup)
3. [Contract Architecture & Code Layout](#-contract-architecture--code-layout)
4. [Step-by-Step Contribution Workflow](#-step-by-step-contribution-workflow)
5. [Smart Contract Standards & Safety Rules](#-smart-contract-standards--safety-rules)
6. [Testing & Verification Requirements](#-testing--verification-requirements)
7. [Git Commit & Pull Request Guidelines](#-git-commit--pull-request-guidelines)
8. [Getting Help & Community](#-getting-help--community)

---

## 🏛️ Ecosystem & Repository Overview

`soroban-gasless-contracts` is part of the modular **Stellar Gasless Network** suite:

* 📜 **`soroban-gasless-contracts`** (This Repo): Soroban WASM smart contracts for Forwarder, Paymasters & Account Abstraction.
* ⚡ [**`stellar-gasless-relayer`**](https://github.com/stellar-gasless-net/stellar-gasless-relayer): TypeScript backend relayer engine.
* 📦 [**`stellar-gasless-sdk`**](https://github.com/stellar-gasless-net/stellar-gasless-sdk): TypeScript client library & React hooks.
* 🖥️ [**`gasless-relayer-dashboard`**](https://github.com/stellar-gasless-net/gasless-relayer-dashboard): Developer portal & Paymaster management SPA UI.

---

## 🛠️ Prerequisites & Development Setup

### 1. Install Toolchain Requirements
Ensure you have the following installed on your machine:
- **Rust Toolchain** (v1.78+):
  ```bash
  rustup update stable
  ```
- **WASM Compilation Target**:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- **Stellar Developer CLI** (v21+):
  ```bash
  cargo install --locked stellar-cli
  ```

### 2. Fork & Clone the Repository
```bash
git clone https://github.com/YOUR-USERNAME/soroban-gasless-contracts.git
cd soroban-gasless-contracts
git remote add upstream https://github.com/stellar-gasless-net/soroban-gasless-contracts.git
```

---

## 📂 Contract Architecture & Code Layout

```
contracts/
├── trusted-forwarder/            # Meta-Tx Forwarder (Domain Separator, Nonces, Batch calls)
│   ├── src/lib.rs                # Main contract logic
│   ├── src/errors.rs             # #[contracterror] enums
│   └── src/test.rs               # Soroban Env unit tests
├── token-paymaster/              # SAC Token Fee Sponsorship (USDC, EURC)
│   ├── src/lib.rs
│   ├── src/errors.rs
│   └── src/test.rs
├── voucher-paymaster/            # Promotional ECDSA & Merkle Coupon Sponsoring
│   ├── src/lib.rs
│   ├── src/errors.rs
│   └── src/test.rs
├── account-abstraction-wallet/   # Passkey Smart Account (WebAuthn, Session Keys)
│   ├── src/lib.rs
│   ├── src/errors.rs
│   └── src/test.rs
└── gas-estimator/                # Overhead & Gas Estimation Helper
    └── src/lib.rs
```

---

## 🔄 Step-by-Step Contribution Workflow

### Step 1: Claim or Pick an Issue
Look through our open [GitHub Issues](https://github.com/stellar-gasless-net/soroban-gasless-contracts/issues). Filter by labels:
- `good first issue`: Ideal for first-time contributors.
- `intermediate`: Modular features, tests, or contract enhancements.
- `advanced`: Core protocol cryptography, Passkey verification, or security auditing.

Comment on the issue you wish to work on so maintainers can assign it to you.

### Step 2: Create a Feature Branch
```bash
git checkout main
git pull upstream main
git checkout -b feat/issue-42-describe-your-feature
```

### Step 3: Implement Your Changes & Add Unit Tests
- Follow `no_std` rules required for Soroban WASM.
- Define custom errors using `#[contracterror]`.
- Always write corresponding unit tests in `src/test.rs`.

---

## 🛡️ Smart Contract Standards & Safety Rules

1. **Replay Attack Defense**: All forwarded contract calls MUST check and increment on-chain persistent nonces (`(symbol_short!("nonce"), user)`).
2. **Deadline Timestamp Checks**: Verify `env.ledger().timestamp() <= deadline` to prevent delayed transaction execution.
3. **Explicit Error Enums**: Use `#[contracterror]` enums instead of raw panic strings for client SDK error parsing.
4. **Typed Events**: Emit structured events via `env.events().publish(...)` for indexer tracking.

---

## 🧪 Testing & Verification Requirements

Before submitting your Pull Request, verify that all tests compile and pass locally:

```bash
# 1. Run unit tests across all workspace contract crates
cargo test --all

# 2. Build release WASM binaries
cargo build --target wasm32-unknown-unknown --release
```

---

## 📝 Git Commit & Pull Request Guidelines

### Commit Message Format
We enforce **Conventional Commits**:
- `feat: add Merkle tree whitelist proof verification in voucher paymaster`
- `fix: correct deadline validation logic in trusted forwarder`
- `test: add unit tests for token paymaster fee refund`
- `docs: update gas benchmark table in README`

### Pull Request Checklist
When submitting a PR, verify:
- [ ] `cargo test --all` passes 100% cleanly without errors or warnings.
- [ ] Code is formatted with `cargo fmt`.
- [ ] Appropriate unit tests are added in `src/test.rs`.
- [ ] Relevant documentation or README section is updated.

---

## 💬 Getting Help & Community

If you have questions or need technical guidance:
- Open a [GitHub Discussion](https://github.com/stellar-gasless-net/soroban-gasless-contracts/discussions).
- Reach out on the **Stellar Developer Community Discord**.

Thank you for building the future of gasless UX on Stellar! 🚀
