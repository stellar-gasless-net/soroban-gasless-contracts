# Soroban Gasless Smart Contracts (`soroban-gasless-contracts`)

[![Rust](https://img.shields.io/badge/Rust-1.78+-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Soroban SDK](https://img.shields.io/badge/Soroban_SDK-v21.2.0-7C3AED?style=for-the-badge&logo=rust&logoColor=white)](https://soroban.stellar.org/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg?style=for-the-badge)](./LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-Welcome-brightgreen.svg?style=for-the-badge)](./CONTRIBUTING.md)

**Stellar Gasless Network** is a high-throughput, enterprise-grade Gasless Meta-Transaction & Fee Delegation Protocol built natively for Stellar and Soroban WASM smart contracts. It empowers dApp developers to offer **100% zero-gas, 1-click user onboarding experiences** using WebAuthn Passkeys (TouchID/FaceID), native `FeeBumpTransaction` relayers, and dynamic Paymaster gas vaults.

---

## Problem Statement & Technical Solution

### Traditional Friction in Blockchain Onboarding
On traditional networks, a user must purchase and hold native gas tokens (e.g. XLM) in a crypto wallet before they can interact with any dApp. If a user has 0 XLM balance, they cannot transfer tokens, mint NFTs, or execute smart contracts—creating a high drop-off rate for Web2 users.

### How Stellar Gasless Network Solves It
Our protocol leverages **Stellar Native Fee-Bump Transactions (`FeeBumpTransaction`)** and **Soroban Authorization Entries (`SorobanAuthorizationEntry`)** to decouple transaction signing from fee payment:

* **Zero-Gas User Intent**: The user signs a cryptographic intent off-chain using browser TouchID/FaceID (WebAuthn Passkeys) without holding any XLM.
* **Relayer Sponsorship**: The backend Relayer engine wraps the user's intent inside a `FeeBumpTransaction`, signs as the Fee Sponsor, and pays the 0.0001 XLM gas fee from its keypair pool.
* **Paymaster Fee Deduction**: On-chain Paymaster WASM contracts deduct fees in custom SAC tokens (e.g., USDC) or promotional vouchers, reimbursing the Relayer treasury.

---

## System Architecture & Data Flow

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                   CLIENT FRONTEND LAYER                                 │
│  ┌───────────────────────────┐                      ┌────────────────────────────────┐  │
│  │   Browser WebAuthn API    │                      │     @stellar-gasless/sdk       │  │
│  │ (TouchID / FaceID Passkey)│                      │ (GaslessClient & Wallet Adapters)│  │
│  └─────────────┬─────────────┘                      └───────────────┬────────────────┘  │
└────────────────┼────────────────────────────────────────────────────┼───────────────────┘
                 │ (1) Sign Off-Chain Auth Entry (0 XLM Spent)        │
                 └─────────────────────────┬──────────────────────────┘
                                           │ (2) POST /v1/relay
                                           v
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                  BACKEND RELAYER LAYER                                  │
│  ┌───────────────────────────────────────────────────────────────────────────────────┐  │
│  │                      stellar-gasless-relayer Service                              │  │
│  │  - Multi-Keypair Queue Pool (Rotates Account Keys to Prevent Sequence Collisions)  │  │
│  │  - Soroban RPC Pre-flight Simulator (Validates payloads before broadcasting)      │  │
│  │  - FeeBumpTransaction Builder (Wraps inner payload & signs as Fee Sponsor)       │  │
│  │  - Prometheus Telemetry Exporter (/metrics endpoint for monitoring & alert logs)  │  │
│  └───────────────────────────────────────┬───────────────────────────────────────────┘  │
└──────────────────────────────────────────┼──────────────────────────────────────────────┘
                                           │ (3) Broadcast FeeBump Tx over Horizon RPC
                                           v
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                ON-CHAIN SMART CONTRACT LAYER                            │
│  ┌───────────────────────────────────────────────────────────────────────────────────┐  │
│  │                      soroban-gasless-contracts WASM Suite                         │  │
│  │  - trusted-forwarder: EIP-712 Domain Separator, Nonce Bitmap Replay Guard         │  │
│  │  - token-paymaster: SAC USDC Dynamic Fee Swap & Discount Tier Settlement          │  │
│  │  - voucher-paymaster: Single-use ECDSA Coupon & Merkle Inclusion Proofs           │  │
│  │  - account-abstraction-wallet: Native secp256r1 Passkey Smart Accounts            │  │
│  └───────────────────────────────────────┬───────────────────────────────────────────┘  │
└──────────────────────────────────────────┼──────────────────────────────────────────────┘
                                           │ (4) Ledger Confirmation
                                           v
                               ┌───────────────────────┐
                               │ Stellar Ledger Block  │
                               └───────────────────────┘
```

---

## Core Protocol Features — What's Actually Implemented

This describes what each contract's code does *today*, verified against the source, not the original aspirational spec. Where a feature is planned but not built, it's called out explicitly instead of implied.

### 1. Trusted Forwarder (`trusted-forwarder`)
* **Implemented**: sequential nonce replay guard, a deadline expiry check, single-call dispatch to a target contract via `env.invoke_contract`, and a `Forwarded` event.
* **Not implemented yet**: no EIP-712-style domain separator (no chain/contract-bound signing domain), and no `execute_batch` — only one call per forwarded transaction, not atomic batches.

### 2. Paymasters (`token-paymaster`, `voucher-paymaster`)
* **`token-paymaster`, implemented**: a flat per-transaction fee charged in a single configured SAC token, transferred straight to the relayer treasury.
* **`token-paymaster`, not implemented yet**: no USDC→XLM auto-swap, no dynamic/volume-based discount tiers (see open issues).
* **`voucher-paymaster`, implemented**: single-use voucher IDs — the contract records a voucher ID as spent and rejects reuse.
* **`voucher-paymaster`, not implemented yet**: no Merkle tree/proof verification — vouchers are simple IDs marked used, not cryptographic Merkle-inclusion coupons.

### 3. Smart Account Wallet (`account-abstraction-wallet`)
* **Implemented**: an owner-controlled wallet that dispatches calls via `execute()`, plus a `SessionData` record (allowed contract + expiry) written by `add_session_key`.
* **Not implemented yet, and this is the most important gap in the whole repo**: the contract stores a `passkey_pubkey` field but never verifies it against anything. There is no `secp256r1`/WebAuthn signature check anywhere in this contract, and no `CustomAccountInterface`/`__check_auth` implementation — auth is just the standard Soroban `Address.require_auth()` on the owner. Passkey signing isn't wired into on-chain auth yet. Session keys are stored but `execute()` doesn't check or enforce them (any call still requires the owner, not a session key).

### 4. Gas Estimator (`gas-estimator`)
* **Not implemented**: `estimate_execution_overhead` returns a hardcoded formula (`5000 + args.len() * 100`) and ignores the target contract and function entirely. It does not measure real Soroban resource usage. Treat this contract as a placeholder — see the Phase 2 roadmap below.

### 5. Relayer Keypair Rotation (`stellar-gasless-relayer`)
* **Implemented**: rotates through the configured `RELAYER_SECRETS` pool per request, and pre-flight simulates every inner transaction against Soroban RPC before sponsoring the fee.

### 6. Console UI (`gasless-relayer-dashboard`)
* **UI mockup only, no backend.** See that repo's README — the relayer it's designed to talk to isn't deployed anywhere.

---

## Integration & Usage Code Examples

### Example 1: Initialize Client SDK & Submit Meta-Transaction
```typescript
import { GaslessClient } from '@stellar-gasless/sdk';

// 1. Initialize Gasless Client
const client = new GaslessClient({
  relayerUrl: 'https://your-relayer-domain.example', // your own deployed stellar-gasless-relayer
  dappApiKey: 'YOUR_DAPP_API_KEY',
});

// 2. Submit signed Soroban authorization payload
const result = await client.submitGaslessTransaction({
  userAddress: 'GBUSER_WALLET_ADDRESS_HERE',
  targetContract: 'CCFORWARDER_TRUSTED_CONTRACT_ID',
  payloadXdr: 'AAAAAgAAAA...',
});

console.log('Gasless Transaction Hash:', result.hash);
```

### Example 2: Request WebAuthn Passkey Biometric Signature
```typescript
import { PasskeyAdapter } from '@stellar-gasless/sdk';

// Request TouchID / FaceID signature off-chain (0 XLM spent by user)
const credential = await PasskeyAdapter.signChallenge(challengeHex);
console.log('Biometric Passkey Credential ID:', credential.id);
```

### Example 3: React Hook
```tsx
import { GaslessClient, useGaslessTransaction } from '@stellar-gasless/sdk';

const client = new GaslessClient({
  relayerUrl: 'https://your-relayer-domain.example',
  dappApiKey: 'YOUR_DAPP_API_KEY',
});

// signedInnerTxXdr must already be built (e.g. via Contract.call()) and signed
// by the user — there is no build+sign-in-one-call helper yet (see roadmap).
function GaslessSubmitButton({ signedInnerTxXdr }: { signedInnerTxXdr: string }) {
  const { submit, isSubmitting, txHash } = useGaslessTransaction(client);

  return (
    <button onClick={() => submit(signedInnerTxXdr)} disabled={isSubmitting}>
      {isSubmitting ? 'Submitting...' : 'Submit Gasless Tx'}
    </button>
  );
}
```

---

## Comprehensive Repository Suite

| Repository | Tech Stack | Primary Responsibilities | Governance Guide |
| :--- | :--- | :--- | :--- |
| **[`soroban-gasless-contracts`](https://github.com/stellar-gasless-net/soroban-gasless-contracts)** | Rust, Soroban SDK v21 | WASM Smart Contracts: `trusted-forwarder`, `token-paymaster`, `voucher-paymaster`, `account-abstraction-wallet`. | [Contracts Guide](./CONTRIBUTING.md) |
| **[`stellar-gasless-relayer`](https://github.com/stellar-gasless-net/stellar-gasless-relayer)** | TypeScript, Node.js, Express | Backend Relayer Engine: Multi-keypair queue pool, Horizon RPC simulation, FeeBump builder, Prometheus telemetry. | [Relayer Guide](https://github.com/stellar-gasless-net/stellar-gasless-relayer/blob/main/CONTRIBUTING.md) |
| **[`stellar-gasless-sdk`](https://github.com/stellar-gasless-net/stellar-gasless-sdk)** | TypeScript, WebAuthn, React | Client Library & Wallet Adapters: `GaslessClient`, WebAuthn Passkey biometric signer, Freighter / xBull adapters. | [SDK Guide](https://github.com/stellar-gasless-net/stellar-gasless-sdk/blob/main/CONTRIBUTING.md) |
| **[`gasless-relayer-dashboard`](https://github.com/stellar-gasless-net/gasless-relayer-dashboard)** | Vanilla JS, HTML5, CSS3 | Protocol Console: Paymaster XLM deposits, dApp API key portal, live Horizon RPC broadcast simulator. | [Console Guide](https://github.com/stellar-gasless-net/gasless-relayer-dashboard/blob/main/CONTRIBUTING.md) |

---

## Open-Source Governance & Contributing

We welcome contributions from developer community members, security researchers, and Stellar ecosystem builders.

Before contributing code or opening pull requests, please review our contributor guidelines:
* **[Smart Contracts Contributor Guide](./CONTRIBUTING.md)**
* **[Backend Relayer Contributor Guide](https://github.com/stellar-gasless-net/stellar-gasless-relayer/blob/main/CONTRIBUTING.md)**
* **[Client SDK Contributor Guide](https://github.com/stellar-gasless-net/stellar-gasless-sdk/blob/main/CONTRIBUTING.md)**
* **[Protocol Console Contributor Guide](https://github.com/stellar-gasless-net/gasless-relayer-dashboard/blob/main/CONTRIBUTING.md)**

### Pull Request Workflow Rules
1. **Claim an Issue**: Browse open issues tagged `good first issue`, `intermediate`, or `advanced`.
2. **Local Test Verification**: Ensure all local test suites pass 100% cleanly before submitting:
   * Contracts: `cargo test --all`
   * Relayer & SDK: `npm test` && `npm run build`
3. **Conventional Commits**: Use standardized git commit messages (`feat: ...`, `fix: ...`, `docs: ...`).

---

## Protocol Roadmap & Future Upgrades

```
  ┌──────────────────────┐       ┌──────────────────────┐       ┌──────────────────────┐
  │  Phase 1 (Current)   │ ────► │   Phase 2 (Q4 2026)  │ ────► │   Phase 3 (2027)     │
  │ Core Contracts, SDK, │       │ Dynamic Gas Oracle,  │       │ Mainnet Bundler Pool,│
  │ Console, Relayer.    │       │ Multi-Sig Governance.│       │ Mobile SDK Adapters. │
  └──────────────────────┘       └──────────────────────┘       └──────────────────────┘
```

### Phase 1 (Built, unaudited)
- [x] All 5 contracts compile with a passing `cargo test --all` (9 tests total). `trusted-forwarder`'s replay-guard and deadline-expiry logic and `gas-estimator`'s formula are directly tested; the paymaster/voucher/wallet contracts each have one initialization-level test — see "What's Actually Implemented" above for what each one really does today. **No third-party or self-audit has been performed.** Treat this as early, unaudited code, not production-ready.
- [x] Relayer engine with multi-keypair rotation and real Soroban RPC pre-flight simulation (see `stellar-gasless-relayer`).
- [x] WebAuthn passkey signature request (browser-side only — see the "not implemented yet" note on `account-abstraction-wallet` above; the signature isn't verified on-chain yet).
- [x] Console UI mockup (no backend — see `gasless-relayer-dashboard`).

### Phase 2 (Upcoming)
- [ ] **On-chain passkey verification**: wire `secp256r1`/WebAuthn signature checking into `account-abstraction-wallet`'s auth — currently the stored passkey key is unused.
- [ ] **Soroban Gas Estimator**: replace the current hardcoded placeholder formula in `gas-estimator` with a real resource-usage measurement.
- [ ] **Merkle-proof vouchers**: replace `voucher-paymaster`'s simple used-ID check with real Merkle inclusion proofs.
- [ ] **Multi-Sig Paymaster Governance Vaults**: Multi-signature approval thresholds for depositing and withdrawing XLM gas reserves.
- [ ] **React Native & Flutter Adapters**: Mobile SDK adapters supporting mobile WebAuthn passkey enclaves.

### Phase 3 (Ecosystem Scaling & Mainnet Expansion)
- [ ] **Decentralized Bundler Node Network**: Peer-to-peer relayer node network incentivized via fee splits.
- [ ] **Security audit**, then Soroban mainnet deployment. No audit has happened yet — this is explicitly a prerequisite, not a completed step.

---

## Security Policy & License

* **Security Vulnerability Reporting**: See [`SECURITY.md`](./SECURITY.md) for responsible disclosure guidelines.
* **License**: All repositories under **Stellar Gasless Network** are licensed under the **MIT License**. See [`LICENSE`](./LICENSE) for full details.
