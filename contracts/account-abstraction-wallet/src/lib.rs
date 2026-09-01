#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env, Symbol, Vec, Val
};

pub mod errors;
use errors::WalletError;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionData {
    pub allowed_contract: Address,
    pub expires_at: u64,
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
    pub fn init(env: Env, owner: Address, passkey_pubkey: BytesN<65>) {
        if env.storage().instance().has(&symbol_short!("owner")) {
            panic!("Wallet already initialized");
        }
        owner.require_auth();
        env.storage().instance().set(&symbol_short!("owner"), &owner);
        env.storage().instance().set(&symbol_short!("passkey"), &passkey_pubkey);
        env.storage().instance().set(&symbol_short!("seq"), &0u64);
    }

    /// Add temporary session key with specific contract whitelist & expiration
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

    /// Execute transaction through Smart Account
    pub fn execute(
        env: Env,
        target: Address,
        function: Symbol,
        args: Vec<Val>,
    ) -> Val {
        let owner: Address = env.storage().instance().get(&symbol_short!("owner")).unwrap();
        owner.require_auth();

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

    /// Verify a WebAuthn passkey assertion against this wallet's stored public key.
    ///
    /// Implements the real WebAuthn signature-verification algorithm
    /// (https://www.w3.org/TR/webauthn-2/#sctn-verifying-assertion):
    /// 1. Confirms `challenge` is actually the one embedded in `client_data_json`'s
    ///    base64url-encoded "challenge" field, so a signature over a stale/different
    ///    challenge can't be replayed.
    /// 2. Reconstructs the exact bytes the authenticator signed:
    ///    `authenticator_data || SHA-256(client_data_json)`.
    /// 3. Verifies the secp256r1 signature over SHA-256 of that payload using the
    ///    wallet's stored passkey public key.
    ///
    /// Panics with `WalletError::InvalidPasskeySignature` if the challenge isn't present
    /// in `client_data_json`. Panics via the host's own trap if the cryptographic
    /// signature itself is invalid (that's how `secp256r1_verify` behaves — it has no
    /// "return false" path, only "trap on failure").
    ///
    /// This does not (yet) replace `require_auth()` in `execute()` — it's a standalone,
    /// independently-callable and independently-testable verification primitive. Wiring
    /// it into the account's actual auth flow (via Soroban's CustomAccountInterface) is
    /// separate, larger follow-up work.
    ///
    /// IMPORTANT for whoever wires this to real browser WebAuthn signatures: Soroban's
    /// `secp256r1_verify` requires the signature's `s` value to be low-S normalized
    /// (standard anti-malleability rule). Not every authenticator/browser guarantees this —
    /// normalize `s` client-side (or server-side in the relayer) before submitting, or
    /// verification will fail on an otherwise-valid signature. See `p256::ecdsa::Signature::normalize_s()`
    /// for the Rust-side equivalent used in this contract's own tests.
    pub fn verify_passkey_signature(
        env: Env,
        challenge: BytesN<32>,
        client_data_json: Bytes,
        authenticator_data: Bytes,
        signature: BytesN<64>,
    ) {
        let challenge_array = challenge.to_array();
        let challenge_b64 = base64_url_encode_no_pad(&env, &challenge_array);
        if !bytes_contains(&client_data_json, &challenge_b64) {
            panic!("{}", WalletError::InvalidPasskeySignature as u32);
        }

        let client_data_hash = env.crypto().sha256(&client_data_json);
        let mut signed_data = authenticator_data.clone();
        let hash_bytes: Bytes = client_data_hash.into();
        signed_data.append(&hash_bytes);
        let message_digest = env.crypto().sha256(&signed_data);

        let passkey_pubkey: BytesN<65> = env
            .storage()
            .instance()
            .get(&symbol_short!("passkey"))
            .unwrap();

        env.crypto().secp256r1_verify(&passkey_pubkey, &message_digest, &signature);
    }
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
