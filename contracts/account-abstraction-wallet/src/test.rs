#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env};
use p256::ecdsa::signature::hazmat::PrehashSigner;
use p256::ecdsa::{Signature, SigningKey};

/// Fixed, deterministic test keypair (not a real secret — for reproducible tests only).
fn test_keypair() -> (SigningKey, [u8; 65]) {
    let secret_bytes: [u8; 32] = [7u8; 32];
    let signing_key = SigningKey::from_bytes((&secret_bytes).into()).expect("valid scalar");
    let verifying_key = signing_key.verifying_key();
    let encoded = verifying_key.to_encoded_point(false); // uncompressed SEC-1, 65 bytes
    let mut pubkey_bytes = [0u8; 65];
    pubkey_bytes.copy_from_slice(encoded.as_bytes());
    (signing_key, pubkey_bytes)
}

fn init_wallet(env: &Env) -> (SmartAccountWalletContractClient<'static>, SigningKey) {
    let contract_id = env.register_contract(None, SmartAccountWalletContract);
    let client = SmartAccountWalletContractClient::new(env, &contract_id);
    let owner = Address::generate(env);
    let (signing_key, pubkey_bytes) = test_keypair();
    let passkey = BytesN::from_array(env, &pubkey_bytes);
    client.init(&owner, &passkey);
    (client, signing_key)
}

/// Builds a real, correctly-signed WebAuthn-shaped assertion for the given challenge,
/// using the exact same authenticatorData || SHA256(clientDataJSON) construction the
/// contract expects.
fn build_assertion(
    env: &Env,
    signing_key: &SigningKey,
    challenge_bytes: [u8; 32],
) -> (BytesN<32>, Bytes, Bytes, BytesN<64>) {
    let challenge = BytesN::from_array(env, &challenge_bytes);
    let challenge_b64 = base64_url_encode_no_pad(env, &challenge_bytes);

    let mut client_data_json =
        Bytes::from_slice(env, b"{\"type\":\"webauthn.get\",\"challenge\":\"");
    client_data_json.append(&challenge_b64);
    client_data_json.append(&Bytes::from_slice(
        env,
        b"\",\"origin\":\"https://example.test\"}",
    ));

    // Minimal but realistic authenticatorData: 32-byte rpIdHash + 1-byte flags + 4-byte counter.
    let mut auth_data_array = [0u8; 37];
    auth_data_array[32] = 0x01; // user-present flag
    let authenticator_data = Bytes::from_array(env, &auth_data_array);

    let client_data_hash = env.crypto().sha256(&client_data_json);
    let mut signed_data = authenticator_data.clone();
    let hash_bytes: Bytes = client_data_hash.into();
    signed_data.append(&hash_bytes);
    let message_digest_array = env.crypto().sha256(&signed_data).to_array();

    let sig: Signature = signing_key
        .sign_prehash(&message_digest_array)
        .expect("sign_prehash should succeed for a valid 32-byte digest");
    // Soroban's secp256r1_verify requires the signature's 's' value in low-S normalized
    // form (standard anti-malleability rule) — raw ECDSA signing doesn't guarantee this.
    let sig = sig.normalize_s().unwrap_or(sig);
    let sig_bytes: [u8; 64] = sig.to_bytes().into();
    let signature = BytesN::from_array(env, &sig_bytes);

    (challenge, client_data_json, authenticator_data, signature)
}

#[test]
fn test_smart_account_init_and_owner() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SmartAccountWalletContract);
    let client = SmartAccountWalletContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let (_signing_key, pubkey_bytes) = test_keypair();
    let passkey = BytesN::from_array(&env, &pubkey_bytes);

    client.init(&owner, &passkey);

    assert_eq!(client.get_owner(), owner);
}

#[test]
fn test_verify_passkey_signature_accepts_a_real_valid_assertion() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signing_key) = init_wallet(&env);

    let (challenge, client_data_json, authenticator_data, signature) =
        build_assertion(&env, &signing_key, [42u8; 32]);

    // Should not panic.
    client.verify_passkey_signature(&challenge, &client_data_json, &authenticator_data, &signature);
}

#[test]
#[should_panic]
fn test_verify_passkey_signature_rejects_a_challenge_not_in_client_data_json() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signing_key) = init_wallet(&env);

    let (_correct_challenge, client_data_json, authenticator_data, signature) =
        build_assertion(&env, &signing_key, [42u8; 32]);

    // Present a different challenge than the one actually embedded in client_data_json.
    let wrong_challenge = BytesN::from_array(&env, &[99u8; 32]);

    client.verify_passkey_signature(
        &wrong_challenge,
        &client_data_json,
        &authenticator_data,
        &signature,
    );
}

#[test]
#[should_panic]
fn test_verify_passkey_signature_rejects_a_tampered_authenticator_data() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signing_key) = init_wallet(&env);

    let (challenge, client_data_json, authenticator_data, signature) =
        build_assertion(&env, &signing_key, [42u8; 32]);

    // Flip a byte in authenticatorData after signing — the signature no longer covers
    // this exact payload, so verification must fail even though the challenge is intact.
    let mut tampered_array = [0u8; 37];
    for i in 0..37u32 {
        tampered_array[i as usize] = authenticator_data.get(i).unwrap();
    }
    tampered_array[0] ^= 0xFF;
    let tampered_authenticator_data = Bytes::from_array(&env, &tampered_array);

    client.verify_passkey_signature(
        &challenge,
        &client_data_json,
        &tampered_authenticator_data,
        &signature,
    );
}
