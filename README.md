# Soroban Gasless Smart Contracts (`soroban-gasless-contracts`)

[![CI](https://github.com/stellar-gasless-net/soroban-gasless-contracts/actions/workflows/ci.yml/badge.svg)](https://github.com/stellar-gasless-net/soroban-gasless-contracts/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.78+-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Soroban SDK](https://img.shields.io/badge/Soroban_SDK-v21.2.0-7C3AED?style=for-the-badge&logo=rust&logoColor=white)](https://soroban.stellar.org/)
[![License](https://img.shields.io/badge/License-MIT-blue.svg?style=for-the-badge)](./LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-Welcome-brightgreen.svg?style=for-the-badge)](./CONTRIBUTING.md)

**Stellar Gasless Network** is a Gasless Meta-Transaction & Fee Delegation protocol for Stellar and Soroban: real, tested, unaudited testnet contracts (see Current Status below) that let a dApp sponsor a user's transaction fee, so the user never needs to hold XLM to interact with it. Uses WebAuthn Passkeys (TouchID/FaceID), native `FeeBumpTransaction` relayers, and Paymaster gas vaults.

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
│  │  - trusted-forwarder: Sequential Nonce Replay Guard, Atomic Batch Dispatch        │  │
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
* **Implemented**: sequential nonce replay guard, a deadline expiry check, single-call dispatch via `execute_forwarded`, atomic multi-call dispatch via `execute_batch` (added 2026-09-06 — one signature and one nonce covering the whole `Vec<BatchCall>`; if any call in the batch panics, the entire invocation rolls back, including the nonce bump, since Soroban transactions are atomic by default), and `Forwarded`/batch-forward events.
* **No separate EIP-712-style domain separator, and this isn't a gap.** That roadmap item was carried over from an Ethereum-shaped mental model that doesn't map onto how Soroban authorization actually works: `user.require_auth()` is validated by the host against the *current network's* passphrase and the *exact* invocation tree being authorized — including this specific contract's address at its position in that tree. A signed authorization for calling this contract cannot be replayed against a different contract instance or a different network, which is exactly what EIP-712's `verifyingContract`/`chainId` fields exist to bolt onto Ethereum's otherwise-domainless signing. Soroban has that binding natively; there was never a real gap here to close.

### 2. Paymasters (`token-paymaster`, `voucher-paymaster`)
* **`token-paymaster`, implemented**: a flat per-transaction fee charged in a single configured SAC token, transferred straight to the relayer treasury.
* **`token-paymaster`, not implemented yet**: no USDC→XLM auto-swap, no dynamic/volume-based discount tiers (see open issues).
* **`voucher-paymaster`, implemented**: real Merkle-inclusion coupons, not opaque IDs. A sponsor calls `register_voucher_batch()` (their one live signature) to commit to a whole batch via its Merkle root; individual vouchers then redeem through `validate_voucher()` with a real path proving `(voucher_id, max_fee)` was actually in that batch — no further sponsor signature needed per redemption, which is the actual point of a voucher system. This replaced the old design, which required the sponsor to live-sign *every single redemption* (defeating the "pre-approve a batch" model entirely) and never checked `max_fee` against anything, so a caller could claim any fee cap for a given voucher ID. Single-use replay protection (a voucher ID can't be redeemed twice, scoped per batch) is unchanged. **Real batch versioning (2026-09-06)**: `register_voucher_batch()` no longer overwrites a sponsor's previous root — each call gets its own version number (returned to the caller) and every version a sponsor has ever registered stays independently redeemable via `validate_voucher()`'s new `batch_version` parameter. Rotating to a new batch no longer orphans unredeemed vouchers from an old one, and `get_latest_batch_version()` lets tooling look up a sponsor's current batch.

### 3. Smart Account Wallet (`account-abstraction-wallet`)
* **Implemented and wired for real**: `execute()` calls `env.current_contract_address().require_auth()` — since this contract implements Soroban's `CustomAccountInterface`, that routes through this contract's own `__check_auth`, which now accepts one of two real signature kinds. `WalletSignature::Owner` verifies a real WebAuthn/secp256r1 passkey signature (challenge-embedding check, `authenticatorData || SHA-256(clientDataJSON)` reconstruction, `secp256r1_verify` against the stored key) and grants full authority — correctly so, since the host already binds the signed digest to the exact set of calls being authorized. `WalletSignature::Session` is scoped: a session key added via `add_session_key` targets one contract, one **per-function allowlist** (added 2026-09-06 — "can call `swap` but not `withdraw_all`" on the same contract is now real, not just described in the roadmap), an optional **cumulative spend cap** (also added 2026-09-06, tracked per session key and persisted across separate authorizations — not reset per call), and an expiration. `__check_auth` inspects `auth_contexts` to confirm every call the session key is being used for — including reading the real target *and function* out of `execute()`'s own arguments, not just its own contract address — matches that allowlist, rejects it if expired, enforces the spend cap for SEP-41 `transfer`/`transfer_from` calls specifically (the one argument-position convention every real Stellar asset actually follows — there's no general way to know where "amount" lives in an arbitrary function's arguments), and only then asks the host to verify the session key's own real signature via `require_auth()`. `verify_passkey_signature()` remains available as a standalone entry point and shares its core logic with `__check_auth`'s owner path via one function (`passkey_message_digest`). Covered by 15 tests using real cryptographic material: a real P-256 keypair for the owner-passkey tests, and real `Context` values matching what the host actually builds for the session-key tests (accepts an in-scope call, rejects an out-of-scope call/function even with a genuinely valid signature, rejects an expired key, rejects an unregistered key, enforces the spend cap both for a single over-cap transfer and for cumulative spend across two separate authorizations).

### 4. Relayer Keypair Rotation (`stellar-gasless-relayer`)
* **Implemented**: rotates through the configured `RELAYER_SECRETS` pool per request, and pre-flight simulates every inner transaction against Soroban RPC before sponsoring the fee.

### 5. Console UI (`gasless-relayer-dashboard`)
* **UI mockup for most panels, two real integrations, live at [stellar-gasless-net.github.io/gasless-relayer-dashboard](https://stellar-gasless-net.github.io/gasless-relayer-dashboard/).** See that repo's README for exactly which parts are real (a live relayer status poll and an end-to-end gasless transaction demo) versus mockup — the relayer it's designed to talk to isn't deployed anywhere public, so those two features need a locally-running relayer to actually light up.

### Removed: Gas Estimator (2026-09-05)
This workspace used to include a `gas-estimator` contract whose `estimate_execution_overhead` returned a hardcoded formula (`5000 + args.len() * 100`), ignoring the target contract and function entirely. Making that a genuine on-chain measurement turned out to be infeasible, not just unbuilt: a contract can only learn a call's real resource cost by actually invoking it (via `env.budget()` before/after), which means actually performing whatever side effects that call has — a token transfer would actually transfer tokens just to "estimate" its cost. That's unsafe and defeats the point of an estimate. Real Soroban gas/resource estimation is correctly an off-chain RPC preflight-simulation concern, and `stellar-gasless-relayer`'s `SorobanSimulator` already does this for real via `simulateTransaction` before ever sponsoring a fee — this protocol didn't need a second, on-chain, unavoidably-fake version of the same thing. The contract was removed from this workspace rather than left as a permanent placeholder; its old testnet address is kept in `deployments/testnet.json`'s notes purely as a historical record (a deployed Soroban contract can't be deleted from the ledger), not presented as a working feature.

---

## Deployment

All four contracts are live on Stellar testnet (deployed 2026-09-03, see
[`deployments/testnet.json`](deployments/testnet.json) — independently checkable on
[stellar.expert](https://stellar.expert/explorer/testnet)):

| Contract | Address |
|---|---|
| `trusted_forwarder` | `CDFBWUWRZJ7VEEVLOBE7C5LLUBFNTI5NJ6FQLIPQSBSJPC3DPLQEGAHW` |
| `token_paymaster` | `CAOWMY7YUKA4RNOA43SRFMFYDXRQNQB7EGVWHPTXRQEGK5UOMLZDCYPU` |
| `voucher_paymaster` | `CBB6LYJDAAFS2Q6DI3F46FNPNSSLBIHAEFCRLKH5E2BC7MX64CWXVQK3` |
| `account_abstraction_wallet` | `CB6CXIQDMOR2JSYVUOHQRFOPZ3TZVKY45RKQXAEQFCWXIPBLI3ST4FR3` |

`token_paymaster` is initialized against testnet's real native XLM Stellar Asset Contract
(`CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`), not a placeholder token.
`account_abstraction_wallet` is initialized with a real generated secp256r1 demo passkey —
see [`docs/DEPLOYMENT_GUIDE.md`](docs/DEPLOYMENT_GUIDE.md) for what that means and how it
was generated. `scripts/deploy.sh` reproduces all of this from scratch.

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
2. **Local Test Verification**: Run the same checks CI runs before submitting:
   * Contracts: `bash scripts/check-source-artifacts.sh` && `cargo test --all`
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
- [x] All 4 contracts compile with a passing `cargo test --all` (35 tests total). `trusted-forwarder`'s replay-guard, deadline-expiry, and atomic-batch logic, `account-abstraction-wallet`'s passkey auth, per-function session-key scoping, and spend caps, `voucher-paymaster`'s Merkle-coupon verification and batch versioning (including a real front-running fix and per-sponsor scoping), and `token-paymaster`'s fee-charging and reserve-withdrawal are all directly tested — see "What's Actually Implemented" above for what each one really does today. **No third-party or self-audit has been performed.** Treat this as early, unaudited code, not production-ready.
- [x] Relayer engine with multi-keypair rotation and real Soroban RPC pre-flight simulation (see `stellar-gasless-relayer`).
- [x] WebAuthn passkey authentication, wired end to end: browser signature request (SDK) → on-chain secp256r1 verification → actually gates `execute()` via `CustomAccountInterface`/`__check_auth`, not just a standalone verifier — see `account-abstraction-wallet` above.
- [x] Session-key scoping, enforced for real: `__check_auth` inspects `auth_contexts` and confirms every call a session key is used for targets that key's one whitelisted contract *and* one of its allowed functions, with an optional cumulative SEP-41 spend cap — see `account-abstraction-wallet` above.
- [x] Real Merkle-inclusion voucher redemption with real batch versioning, replacing the old opaque-ID, single-batch design — see `voucher-paymaster` above.
- [x] Console UI mockup (no backend — see `gasless-relayer-dashboard`).

### Phase 2 (Upcoming)
- [ ] **Multi-Sig Paymaster Governance Vaults**: Multi-signature approval thresholds for depositing and withdrawing XLM gas reserves.
- [ ] **React Native & Flutter Adapters**: Mobile SDK adapters supporting mobile WebAuthn passkey enclaves.

### Phase 3 (Ecosystem Scaling & Mainnet Expansion)
- [ ] **Decentralized Bundler Node Network**: Peer-to-peer relayer node network incentivized via fee splits.
- [ ] **Security audit**, then Soroban mainnet deployment. No audit has happened yet — this is explicitly a prerequisite, not a completed step.

---

## Security Policy & License

* **Security Vulnerability Reporting**: See [`SECURITY.md`](./SECURITY.md) for responsible disclosure guidelines.
* **License**: All repositories under **Stellar Gasless Network** are licensed under the **MIT License**. See [`LICENSE`](./LICENSE) for full details.
