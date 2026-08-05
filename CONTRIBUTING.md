# Contributing to Soroban Gasless Smart Contracts

Thank you for your interest in contributing to **`soroban-gasless-contracts`**! We welcome open-source contributions, security improvements, and feature pull requests.

---

## 🛠️ Development Setup

1. **Fork and Clone the Repository**:
   ```bash
   git clone https://github.com/stellar-gasless-net/soroban-gasless-contracts.git
   cd soroban-gasless-contracts
   ```

2. **Install Toolchain Prerequisites**:
   - Rust (v1.78+): `rustup update stable`
   - WASM target: `rustup target add wasm32-unknown-unknown`
   - Soroban CLI: `cargo install --locked stellar-cli`

3. **Run Unit Tests**:
   ```bash
   cargo test --all
   ```

4. **Build Release WASM Binaries**:
   ```bash
   cargo build --target wasm32-unknown-unknown --release
   ```

---

## 📌 Pull Request Guidelines

- Ensure `cargo test --all` passes 100% cleanly before submitting PRs.
- Keep commits atomic with clear conventional commit messages (`feat: ...`, `fix: ...`, `docs: ...`, `test: ...`).
- Add unit tests for any new contract methods or error handling logic.

---

## 💬 Community & Support

For questions or discussions, open an Issue or join the Stellar Developer Community Discord.
