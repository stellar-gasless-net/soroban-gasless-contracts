#![no_std]
use soroban_sdk::{
    auth::{Context, CustomAccountInterface},
    contract, contractimpl, contracttype, crypto::Hash, symbol_short, Address, Bytes, BytesN, Env,
    Symbol, TryFromVal, Vec, Val,
};

pub mod errors;
use errors::WalletError;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionData {
    pub allowed_contract: Address,
    /// The exact functions on `allowed_contract` this key may call — e.g. `swap` but not
    /// `withdraw_all` on the same DEX contract. Previously any function on the whitelisted
    /// contract was allowed once the contract itself matched; this is what actually narrows
    /// that down.
    pub allowed_functions: Vec<Symbol>,
    /// Cumulative cap, in the token's own base units, this key may move via SEP-41
    /// `transfer`/`transfer_from` calls on `allowed_contract` — `None` means no cap. Soroban
    /// has no standard "amount" argument position for arbitrary functions, so cap
    /// enforcement is deliberately scoped to the one universal convention every real
    /// Stellar asset actually follows (SEP-41's `transfer(from, to, amount)` and
    /// `transfer_from(spender, from, to, amount)`, not guessed at for arbitrary calls.
    pub spend_cap: Option<i128>,
    /// Running total already moved under `spend_cap`, updated on every SEP-41
    /// transfer/transfer_from this key authorizes. Resets to 0 whenever `add_session_key`
    /// re-registers this key, the same way `allowed_contract`/`expires_at` are fully
    /// replaced rather than merged — re-authorizing a key is the owner's deliberate act.
    pub spent: i128,
    pub expires_at: u64,
}

/// The signature type this wallet's `__check_auth` expects: a WebAuthn passkey assertion,
/// matching the parameters `verify_passkey_signature` already validates below.
#[contracttype]
#[derive(Clone)]
pub struct PasskeySignature {
    pub client_data_json: Bytes,
    pub authenticator_data: Bytes,
    pub signature: BytesN<64>,
}

/// `__check_auth`'s signature is one of two shapes: the owner's real WebAuthn passkey
/// (full authority, whatever it's attached to), or a claim to be a specific session key
/// (scoped authority, enforced against that key's registered `SessionData` below).
#[contracttype]
#[derive(Clone)]
pub enum WalletSignature {
    Owner(PasskeySignature),
    Session(Address),
}

#[contract]
pub struct SmartAccountWalletContract;

#[contractimpl]
impl SmartAccountWalletContract {
    /// Initialize Smart Account with owner key & WebAuthn Passkey public key.
    ///
    /// `passkey_pubkey` is the SEC-1 uncompressed secp256r1 public key (0x04 prefix byte +
    /// 32-byte X + 32-byte Y = 65 bytes), matching what `env.crypto().secp256r1_verify()`
    /// expects. This is the format a browser's WebAuthn `getPublicKey()` / COSE key, once
    /// converted, naturally produces.
    ///
    /// `owner` remains a plain Stellar keypair used for admin/recovery operations
    /// (`add_session_key`, future key-rotation). Day-to-day transaction authorization goes
    /// through the passkey via `execute()` below, not through `owner`.
    pub fn init(env: Env, owner: Address, passkey_pubkey: BytesN<65>) {
        if env.storage().instance().has(&symbol_short!("owner")) {
            panic!("Wallet already initialized");
        }
        owner.require_auth();
        env.storage().instance().set(&symbol_short!("owner"), &owner);
        env.storage().instance().set(&symbol_short!("passkey"), &passkey_pubkey);
        env.storage().instance().set(&symbol_short!("seq"), &0u64);
    }

    /// Add temporary session key with a specific contract + per-function whitelist, an
    /// optional cumulative SEP-41 spend cap, and an expiration. Still owner-gated
    /// (admin/recovery key), not passkey-gated. Enforcement of all of this happens in
    /// `__check_auth` below when a `WalletSignature::Session` claims this key's authority.
    pub fn add_session_key(
        env: Env,
        session_key: Address,
        allowed_contract: Address,
        allowed_functions: Vec<Symbol>,
        spend_cap: Option<i128>,
        expires_at: u64,
    ) {
        let owner: Address = env.storage().instance().get(&symbol_short!("owner")).unwrap();
        owner.require_auth();

        let session_data = SessionData {
            allowed_contract: allowed_contract.clone(),
            allowed_functions: allowed_functions.clone(),
            spend_cap,
            spent: 0,
            expires_at,
        };

        let key = (symbol_short!("sess"), session_key.clone());
        env.storage().persistent().set(&key, &session_data);

        env.events().publish(
            (symbol_short!("sess_add"), session_key),
            (allowed_contract, allowed_functions, spend_cap, expires_at),
        );
    }

    /// Execute transaction through Smart Account.
    ///
    /// Requires auth on the wallet's OWN address (not `owner`) — since this contract
    /// implements `CustomAccountInterface` below, that routes through this contract's own
    /// `__check_auth`, which verifies a real WebAuthn passkey signature. This is what
    /// actually gates execution by the passkey now, not just an independently-callable
    /// verification function sitting next to unrelated auth logic.
    pub fn execute(env: Env, target: Address, function: Symbol, args: Vec<Val>) -> Val {
        env.current_contract_address().require_auth();

        let result: Val = env.invoke_contract(&target, &function, args);

        env.events().publish(
            (symbol_short!("wal_exec"), target),
            (function, result),
        );

        result
    }

    /// Get current primary owner
    pub fn get_owner(env: Env) -> Address {
        env.storage().instance().get(&symbol_short!("owner")).unwrap()
    }

    /// Verify a WebAuthn passkey assertion against this wallet's stored public key,
    /// independent of the account-auth flow (useful for e.g. a relayer pre-checking a
    /// signature before submitting a transaction). Shares its core logic with
    /// `__check_auth` below via `passkey_message_digest` — one real implementation, two
    /// entry points, not two separate things that could silently drift apart.
    ///
    /// Panics with `WalletError::InvalidPasskeySignature` if the challenge isn't present
    /// in `client_data_json`. Panics via the host's own trap if the cryptographic
    /// signature itself is invalid (that's how `secp256r1_verify` behaves — it has no
    /// "return false" path, only "trap on failure").
    ///
    /// IMPORTANT for whoever wires this to real browser WebAuthn signatures: Soroban's
    /// `secp256r1_verify` requires the signature's `s` value to be low-S normalized
    /// (standard anti-malleability rule). Not every authenticator/browser guarantees this —
    /// normalize `s` client-side (or server-side in the relayer) before submitting, or
    /// verification will fail on an otherwise-valid signature. See
    /// `p256::ecdsa::Signature::normalize_s()` for the Rust-side equivalent used in this
    /// contract's own tests.
    pub fn verify_passkey_signature(
        env: Env,
        challenge: BytesN<32>,
        client_data_json: Bytes,
        authenticator_data: Bytes,
        signature: BytesN<64>,
    ) {
        let digest = passkey_message_digest(&env, &challenge, &client_data_json, &authenticator_data)
            .unwrap_or_else(|| panic!("{}", WalletError::InvalidPasskeySignature as u32));

        let passkey_pubkey: BytesN<65> = env
            .storage()
            .instance()
            .get(&symbol_short!("passkey"))
            .unwrap();

        env.crypto().secp256r1_verify(&passkey_pubkey, &digest, &signature);
    }
}

#[contractimpl]
impl CustomAccountInterface for SmartAccountWalletContract {
    type Signature = WalletSignature;
    type Error = WalletError;

    /// Called automatically by the host whenever something calls
    /// `require_auth()`/`require_auth_for_args()` on this wallet's own address — see
    /// `execute()` above. `signature_payload` is the 32-byte digest the host derived from
    /// the actual transaction; for the owner path that digest is what the passkey must have
    /// signed, i.e. it's used as the WebAuthn `challenge`.
    ///
    /// `WalletSignature::Owner` grants full authority — same as before, and correctly so:
    /// the host already binds `signature_payload` to the exact `auth_contexts` being
    /// authorized, so a real owner signature can't be replayed against a different call.
    /// `WalletSignature::Session` is new: it's scoped, so `auth_contexts` genuinely needs
    /// inspecting here — a session key's own signature is real and host-verified via
    /// `require_auth()` below, but that alone says nothing about whether this wallet is
    /// willing to let *this particular* session key authorize *this particular* call.
    fn __check_auth(
        env: Env,
        signature_payload: Hash<32>,
        signature: WalletSignature,
        auth_contexts: Vec<Context>,
    ) -> Result<(), WalletError> {
        match signature {
            WalletSignature::Owner(passkey_sig) => {
                let challenge: BytesN<32> = signature_payload.to_bytes();
                let digest = passkey_message_digest(
                    &env,
                    &challenge,
                    &passkey_sig.client_data_json,
                    &passkey_sig.authenticator_data,
                )
                .ok_or(WalletError::InvalidPasskeySignature)?;

                let passkey_pubkey: BytesN<65> = env
                    .storage()
                    .instance()
                    .get(&symbol_short!("passkey"))
                    .unwrap();

                // Panics (host trap) on an invalid signature, same as verify_passkey_signature —
                // that's still a valid failure signal to the host for a custom account auth check.
                env.crypto()
                    .secp256r1_verify(&passkey_pubkey, &digest, &passkey_sig.signature);
                Ok(())
            }
            WalletSignature::Session(session_key) => {
                let key = (symbol_short!("sess"), session_key.clone());
                let session_data: SessionData = env
                    .storage()
                    .persistent()
                    .get(&key)
                    .ok_or(WalletError::UnknownSessionKey)?;

                if env.ledger().timestamp() > session_data.expires_at {
                    return Err(WalletError::SessionExpired);
                }

                check_session_scope(&env, &session_key, &session_data, &auth_contexts)?;

                // The session key is a real Stellar address with its own signing key; this
                // asks the host to verify ITS signature was genuinely provided for this same
                // transaction, exactly like any other multi-party Soroban authorization —
                // no bespoke signature-verification code needed for the session key itself.
                session_key.require_auth();
                Ok(())
            }
        }
    }
}

/// Enforces that every authorization context a session key is being used for actually
/// targets `allowed_contract`, calls one of `allowed_functions`, and — if a `spend_cap` is
/// set — doesn't push cumulative SEP-41 transfers past it. This wallet only ever calls
/// `require_auth()` on its own address from inside `execute()`, so the *only* context that
/// can appear here with `contract == this wallet` is that root `execute` call — and since
/// `execute`'s own args are `(target, function, args)`, that's where the real destination
/// and function have to be read from, not from `ctx.contract`/`ctx.fn_name` (which describe
/// the call to `execute()` itself, not the call `execute()` makes). Any other context (e.g.
/// a downstream call that itself needs this wallet's authorization, like a token `transfer`
/// where `from` is this wallet) is checked directly: its own `ctx.contract`/`ctx.fn_name`
/// really are the target contract and function in that case.
fn check_session_scope(
    env: &Env,
    session_key: &Address,
    session_data: &SessionData,
    auth_contexts: &Vec<Context>,
) -> Result<(), WalletError> {
    let this_wallet = env.current_contract_address();
    let mut new_spent = session_data.spent;

    for context in auth_contexts.iter() {
        match context {
            Context::Contract(ctx) => {
                if ctx.contract == this_wallet {
                    if ctx.fn_name != symbol_short!("execute") {
                        return Err(WalletError::ContractNotWhitelisted);
                    }
                    let target_val = ctx
                        .args
                        .get(0)
                        .ok_or(WalletError::ContractNotWhitelisted)?;
                    let target = Address::try_from_val(env, &target_val)
                        .map_err(|_| WalletError::ContractNotWhitelisted)?;
                    if &target != &session_data.allowed_contract {
                        return Err(WalletError::ContractNotWhitelisted);
                    }

                    let function_val = ctx
                        .args
                        .get(1)
                        .ok_or(WalletError::FunctionNotWhitelisted)?;
                    let function = Symbol::try_from_val(env, &function_val)
                        .map_err(|_| WalletError::FunctionNotWhitelisted)?;
                    if !session_data.allowed_functions.contains(&function) {
                        return Err(WalletError::FunctionNotWhitelisted);
                    }
                } else {
                    if &ctx.contract != &session_data.allowed_contract {
                        return Err(WalletError::ContractNotWhitelisted);
                    }
                    if !session_data.allowed_functions.contains(&ctx.fn_name) {
                        return Err(WalletError::FunctionNotWhitelisted);
                    }
                    if let Some(cap) = session_data.spend_cap {
                        if let Some(amount) = sep41_transfer_amount(env, &ctx.fn_name, &ctx.args) {
                            new_spent = new_spent
                                .checked_add(amount)
                                .ok_or(WalletError::SpendCapExceeded)?;
                            if new_spent > cap {
                                return Err(WalletError::SpendCapExceeded);
                            }
                        }
                    }
                }
            }
            Context::CreateContractHostFn(_) => {
                return Err(WalletError::ContractNotWhitelisted);
            }
        }
    }

    if new_spent != session_data.spent {
        let mut updated = session_data.clone();
        updated.spent = new_spent;
        env.storage()
            .persistent()
            .set(&(symbol_short!("sess"), session_key.clone()), &updated);
    }

    Ok(())
}

/// Extracts the amount from a call that matches SEP-41's fixed `transfer(from, to, amount)`
/// or `transfer_from(spender, from, to, amount)` shape — the one argument-position
/// convention every real Stellar asset actually follows, not a guess. Returns `None` for
/// any other function, including one that merely happens to also be named `transfer` on a
/// non-token contract with a different signature — `args.len()` has to match too.
fn sep41_transfer_amount(env: &Env, fn_name: &Symbol, args: &Vec<Val>) -> Option<i128> {
    if *fn_name == symbol_short!("transfer") && args.len() == 3 {
        args.get(2).and_then(|v| i128::try_from_val(env, &v).ok())
    } else if *fn_name == Symbol::new(env, "transfer_from") && args.len() == 4 {
        args.get(3).and_then(|v| i128::try_from_val(env, &v).ok())
    } else {
        None
    }
}

/// Shared core of WebAuthn signature verification, used by both
/// `verify_passkey_signature` and `__check_auth` so there's exactly one implementation of
/// the actual algorithm, not two that could drift apart.
///
/// Confirms `challenge` is embedded in `client_data_json`, then reconstructs the exact
/// bytes a WebAuthn authenticator signs: `SHA-256(authenticator_data || SHA-256(client_data_json))`.
/// Returns `None` if the challenge isn't present (caller decides how to signal that —
/// panic vs. typed `Err`). Does not itself call `secp256r1_verify` — callers do that with
/// the digest this returns, since panic-vs-Err handling differs between the two entry points.
fn passkey_message_digest(
    env: &Env,
    challenge: &BytesN<32>,
    client_data_json: &Bytes,
    authenticator_data: &Bytes,
) -> Option<Hash<32>> {
    let challenge_array = challenge.to_array();
    let challenge_b64 = base64_url_encode_no_pad(env, &challenge_array);
    if !bytes_contains(client_data_json, &challenge_b64) {
        return None;
    }

    let client_data_hash = env.crypto().sha256(client_data_json);
    let mut signed_data = authenticator_data.clone();
    let hash_bytes: Bytes = client_data_hash.into();
    signed_data.append(&hash_bytes);
    Some(env.crypto().sha256(&signed_data))
}

/// Encodes 32 bytes as unpadded base64url, matching how a browser encodes a WebAuthn
/// challenge inside clientDataJSON. Hand-rolled because this crate is `no_std` and has
/// no base64 dependency.
fn base64_url_encode_no_pad(env: &Env, input: &[u8; 32]) -> Bytes {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = Bytes::new(env);
    let mut i: usize = 0;
    while i + 3 <= input.len() {
        let b0 = input[i];
        let b1 = input[i + 1];
        let b2 = input[i + 2];
        out.push_back(ALPHABET[(b0 >> 2) as usize]);
        out.push_back(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize]);
        out.push_back(ALPHABET[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize]);
        out.push_back(ALPHABET[(b2 & 0x3f) as usize]);
        i += 3;
    }
    let rem = input.len() - i;
    if rem == 2 {
        let b0 = input[i];
        let b1 = input[i + 1];
        out.push_back(ALPHABET[(b0 >> 2) as usize]);
        out.push_back(ALPHABET[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize]);
        out.push_back(ALPHABET[((b1 & 0x0f) << 2) as usize]);
    } else if rem == 1 {
        let b0 = input[i];
        out.push_back(ALPHABET[(b0 >> 2) as usize]);
        out.push_back(ALPHABET[((b0 & 0x03) << 4) as usize]);
    }
    out
}

/// Naive substring search over Soroban `Bytes`. `client_data_json` is small (a few hundred
/// bytes at most), so the O(n*m) cost here is fine; not worth pulling in a real search
/// algorithm for buffers this size.
fn bytes_contains(haystack: &Bytes, needle: &Bytes) -> bool {
    let h_len = haystack.len();
    let n_len = needle.len();
    if n_len == 0 || n_len > h_len {
        return false;
    }
    let mut i: u32 = 0;
    while i + n_len <= h_len {
        let mut matched = true;
        let mut j: u32 = 0;
        while j < n_len {
            if haystack.get(i + j) != needle.get(j) {
                matched = false;
                break;
            }
            j += 1;
        }
        if matched {
            return true;
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod test;
