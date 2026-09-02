#![no_std]
use soroban_sdk::{
    auth::{Context, CustomAccountInterface},
    contract, contractimpl, contracttype, crypto::Hash, symbol_short, Address, Bytes, BytesN, Env,
    Symbol, Vec, Val,
};

pub mod errors;
use errors::WalletError;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionData {
    pub allowed_contract: Address,
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

    /// Add temporary session key with specific contract whitelist & expiration.
    /// Still owner-gated (admin/recovery key), not passkey-gated — session keys aren't
    /// enforced anywhere yet (see README), this is unchanged scope for today.
    pub fn add_session_key(
        env: Env,
        session_key: Address,
        allowed_contract: Address,
        expires_at: u64,
    ) {
        let owner: Address = env.storage().instance().get(&symbol_short!("owner")).unwrap();
        owner.require_auth();

        let session_data = SessionData {
            allowed_contract: allowed_contract.clone(),
            expires_at,
        };

        let key = (symbol_short!("sess"), session_key.clone());
        env.storage().persistent().set(&key, &session_data);

        env.events().publish(
            (symbol_short!("sess_add"), session_key),
            (allowed_contract, expires_at),
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
    type Signature = PasskeySignature;
    type Error = WalletError;

    /// Called automatically by the host whenever something calls
    /// `require_auth()`/`require_auth_for_args()` on this wallet's own address — see
    /// `execute()` above. `signature_payload` is the 32-byte digest the host derived from
    /// the actual transaction; that digest is what the passkey must have signed, i.e. it's
    /// used as the WebAuthn `challenge`.
    ///
    /// `auth_contexts` (which calls this authorization actually covers) isn't inspected
    /// here — this wallet doesn't yet enforce per-call scoping (e.g. session-key contract
    /// whitelists). A valid passkey signature currently authorizes whatever it's attached
    /// to, same limitation already disclosed for session keys in the README.
    fn __check_auth(
        env: Env,
        signature_payload: Hash<32>,
        signature: PasskeySignature,
        _auth_contexts: Vec<Context>,
    ) -> Result<(), WalletError> {
        let challenge: BytesN<32> = signature_payload.to_bytes();
        let digest = passkey_message_digest(
            &env,
            &challenge,
            &signature.client_data_json,
            &signature.authenticator_data,
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
            .secp256r1_verify(&passkey_pubkey, &digest, &signature.signature);
        Ok(())
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
