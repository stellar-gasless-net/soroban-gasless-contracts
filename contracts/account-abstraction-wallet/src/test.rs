#![cfg(test)]

use super::*;
use soroban_sdk::{
    auth::ContractContext, crypto::Hash,
    testutils::{Address as _, Ledger as _},
    Address, Bytes, BytesN, Env, IntoVal, Symbol, Val, Vec,
};
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

// __check_auth tests below call the CustomAccountInterface method directly — this is the
// literal function the Soroban host invokes when something calls
// `env.current_contract_address().require_auth()` inside execute(), not a re-implementation
// or a mock. The host's own require_auth -> __check_auth dispatch is Soroban platform
// infrastructure, not code this project wrote, so it isn't what needs testing here — this
// function's actual verification logic is.

#[test]
fn test_check_auth_accepts_a_real_valid_passkey_signature() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signing_key) = init_wallet(&env);

    // Hash::from_bytes is private to soroban_sdk itself — the only legitimate way to get a
    // real Hash<32> here is via env.crypto().sha256(), same as the host would produce one.
    let seed = Bytes::from_array(&env, &[7u8; 32]);
    let payload: Hash<32> = env.crypto().sha256(&seed);
    let challenge_bytes = payload.to_array();

    let (_challenge, client_data_json, authenticator_data, signature) =
        build_assertion(&env, &signing_key, challenge_bytes);
    let sig = WalletSignature::Owner(PasskeySignature {
        client_data_json,
        authenticator_data,
        signature,
    });

    // __check_auth reads instance storage (the stored passkey pubkey), which the host
    // scopes to "whichever contract is currently executing" — calling it as a bare
    // function needs that context set explicitly via as_contract, same as the host would
    // when it dispatches a real require_auth() call to this contract.
    let result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(env.clone(), payload, sig, Vec::new(&env))
    });
    assert!(result.is_ok(), "a genuine passkey signature over the real challenge should authorize");
}

#[test]
fn test_check_auth_rejects_a_signature_over_a_different_payload() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, signing_key) = init_wallet(&env);

    // Sign challenge A, but present it as authorization for a different payload B — this
    // is exactly what execute() must prevent: a signature for one transaction being replayed
    // to authorize a different one. This is caught by the challenge-embedding check (the
    // signed challenge isn't in client_data_json for the *different* payload we're
    // presenting), so it returns a clean Err rather than panicking inside secp256r1_verify.
    let signed_seed = Bytes::from_array(&env, &[7u8; 32]);
    let signed_payload: Hash<32> = env.crypto().sha256(&signed_seed);
    let signed_challenge_bytes = signed_payload.to_array();
    let (_challenge, client_data_json, authenticator_data, signature) =
        build_assertion(&env, &signing_key, signed_challenge_bytes);

    let different_seed = Bytes::from_array(&env, &[9u8; 32]);
    let different_payload: Hash<32> = env.crypto().sha256(&different_seed);
    let sig = WalletSignature::Owner(PasskeySignature {
        client_data_json,
        authenticator_data,
        signature,
    });

    let result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(env.clone(), different_payload, sig, Vec::new(&env))
    });
    assert!(result.is_err(), "a signature over a different payload must not authorize");
}

// Session-key tests below exercise the real scoping enforcement in `check_session_scope`,
// not a mock of it — they call `__check_auth` directly with the exact `Context` shapes the
// host would build for a real `execute()` call, same principle as the owner-passkey tests
// above using a real signed WebAuthn-shaped assertion instead of a stand-in.

fn add_dapp_session_key(env: &Env, client: &SmartAccountWalletContractClient) -> (Address, Address) {
    let session_key = Address::generate(env);
    let dapp_contract = Address::generate(env);
    let allowed_functions: Vec<Symbol> = Vec::from_array(env, [Symbol::new(env, "noop")]);
    client.add_session_key(&session_key, &dapp_contract, &allowed_functions, &None, &10_000u64);
    (session_key, dapp_contract)
}

fn execute_context(env: &Env, wallet: &Address, target: &Address) -> Vec<Context> {
    execute_context_fn(env, wallet, target, "noop", Vec::new(env))
}

fn execute_context_fn(
    env: &Env,
    wallet: &Address,
    target: &Address,
    function: &str,
    call_args: Vec<Val>,
) -> Vec<Context> {
    let args: Vec<Val> = Vec::from_array(
        env,
        [
            target.to_val(),
            Symbol::new(env, function).to_val(),
            call_args.to_val(),
        ],
    );
    Vec::from_array(
        env,
        [Context::Contract(ContractContext {
            contract: wallet.clone(),
            fn_name: Symbol::new(env, "execute"),
            args,
        })],
    )
}

/// Builds the auth-context tree for a session key authorizing a SEP-41 `transfer` directly
/// on the target token — i.e. `transfer`'s own require_auth() on `from`, not routed through
/// this wallet's `execute()`. This is the shape a real token transfer's authorization tree
/// actually takes: the nested Context::Contract node IS the transfer call itself.
fn transfer_context(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) -> Vec<Context> {
    let args: Vec<Val> = Vec::from_array(env, [from.to_val(), to.to_val(), amount.into_val(env)]);
    Vec::from_array(
        env,
        [Context::Contract(ContractContext {
            contract: token.clone(),
            fn_name: Symbol::new(env, "transfer"),
            args,
        })],
    )
}

#[test]
fn test_check_auth_accepts_a_session_key_calling_its_allowed_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signing_key) = init_wallet(&env);
    let (session_key, dapp_contract) = add_dapp_session_key(&env, &client);

    let contexts = execute_context(&env, &client.address, &dapp_contract);
    let payload: Hash<32> = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));
    let sig = WalletSignature::Session(session_key);

    let result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(env.clone(), payload, sig, contexts)
    });
    assert!(result.is_ok(), "a session key calling its own allowed contract must authorize");
}

#[test]
fn test_check_auth_rejects_a_session_key_calling_a_different_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signing_key) = init_wallet(&env);
    let (session_key, _dapp_contract) = add_dapp_session_key(&env, &client);
    let some_other_contract = Address::generate(&env);

    let contexts = execute_context(&env, &client.address, &some_other_contract);
    let payload: Hash<32> = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));
    let sig = WalletSignature::Session(session_key);

    let result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(env.clone(), payload, sig, contexts)
    });
    assert!(
        result.is_err(),
        "a session key must not authorize a call to a contract outside its whitelist, even with a real host-verified signature"
    );
}

#[test]
fn test_check_auth_rejects_an_expired_session_key() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signing_key) = init_wallet(&env);
    let session_key = Address::generate(&env);
    let dapp_contract = Address::generate(&env);
    let allowed_functions: Vec<Symbol> = Vec::from_array(&env, [Symbol::new(&env, "noop")]);
    // expires_at = 0: any ledger timestamp at or past genesis is already expired.
    client.add_session_key(&session_key, &dapp_contract, &allowed_functions, &None, &0u64);
    env.ledger().set_timestamp(1);

    let contexts = execute_context(&env, &client.address, &dapp_contract);
    let payload: Hash<32> = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));
    let sig = WalletSignature::Session(session_key);

    let result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(env.clone(), payload, sig, contexts)
    });
    assert!(result.is_err(), "an expired session key must not authorize anything");
}

#[test]
fn test_check_auth_rejects_an_unregistered_session_key() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signing_key) = init_wallet(&env);
    let stranger = Address::generate(&env);
    let dapp_contract = Address::generate(&env);

    let contexts = execute_context(&env, &client.address, &dapp_contract);
    let payload: Hash<32> = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));
    let sig = WalletSignature::Session(stranger);

    let result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(env.clone(), payload, sig, contexts)
    });
    assert!(result.is_err(), "an address with no registered session key must not authorize anything");
}

// Per-function allowlist tests below exercise real scoping narrower than just the target
// contract — a session key approved for one function on a contract must not be usable for
// a different function on that SAME, otherwise-whitelisted contract.

#[test]
fn test_check_auth_accepts_a_session_key_calling_an_allowed_function() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signing_key) = init_wallet(&env);
    let session_key = Address::generate(&env);
    let dapp_contract = Address::generate(&env);
    let allowed_functions: Vec<Symbol> = Vec::from_array(&env, [Symbol::new(&env, "swap")]);
    client.add_session_key(&session_key, &dapp_contract, &allowed_functions, &None, &10_000u64);

    let contexts = execute_context_fn(&env, &client.address, &dapp_contract, "swap", Vec::new(&env));
    let payload: Hash<32> = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));
    let sig = WalletSignature::Session(session_key);

    let result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(env.clone(), payload, sig, contexts)
    });
    assert!(result.is_ok(), "a session key calling a function it's explicitly allowed must authorize");
}

#[test]
fn test_check_auth_rejects_a_session_key_calling_a_non_whitelisted_function_on_an_allowed_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signing_key) = init_wallet(&env);
    let session_key = Address::generate(&env);
    let dapp_contract = Address::generate(&env);
    // Approved for `swap` only — must not be usable for `withdraw_all` on the same contract.
    let allowed_functions: Vec<Symbol> = Vec::from_array(&env, [Symbol::new(&env, "swap")]);
    client.add_session_key(&session_key, &dapp_contract, &allowed_functions, &None, &10_000u64);

    let contexts = execute_context_fn(&env, &client.address, &dapp_contract, "withdraw_all", Vec::new(&env));
    let payload: Hash<32> = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));
    let sig = WalletSignature::Session(session_key);

    let result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(env.clone(), payload, sig, contexts)
    });
    assert!(
        result.is_err(),
        "a session key must not authorize a function outside its per-function allowlist, even on an otherwise-allowed contract"
    );
}

// Spend cap tests below exercise real cumulative tracking against SEP-41 transfer/transfer_from
// calls made directly on the allowed contract (not routed through execute()) — the shape a
// real token transfer's own authorization actually takes.

#[test]
fn test_check_auth_accepts_a_transfer_under_the_spend_cap_and_records_it() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signing_key) = init_wallet(&env);
    let session_key = Address::generate(&env);
    let token = Address::generate(&env);
    let recipient = Address::generate(&env);
    let allowed_functions: Vec<Symbol> = Vec::from_array(&env, [Symbol::new(&env, "transfer")]);
    client.add_session_key(&session_key, &token, &allowed_functions, &Some(1_000i128), &10_000u64);

    let contexts = transfer_context(&env, &token, &client.address, &recipient, 400i128);
    let payload: Hash<32> = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));
    let sig = WalletSignature::Session(session_key);

    let result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(env.clone(), payload, sig, contexts)
    });
    assert!(result.is_ok(), "a transfer within the spend cap must authorize");
}

#[test]
fn test_check_auth_rejects_a_transfer_that_would_exceed_the_spend_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signing_key) = init_wallet(&env);
    let session_key = Address::generate(&env);
    let token = Address::generate(&env);
    let recipient = Address::generate(&env);
    let allowed_functions: Vec<Symbol> = Vec::from_array(&env, [Symbol::new(&env, "transfer")]);
    client.add_session_key(&session_key, &token, &allowed_functions, &Some(1_000i128), &10_000u64);

    let contexts = transfer_context(&env, &token, &client.address, &recipient, 1_001i128);
    let payload: Hash<32> = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));
    let sig = WalletSignature::Session(session_key);

    let result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(env.clone(), payload, sig, contexts)
    });
    assert!(result.is_err(), "a single transfer over the cap must not authorize");
}

#[test]
fn test_check_auth_rejects_a_second_transfer_once_cumulative_spend_would_exceed_the_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _signing_key) = init_wallet(&env);
    let session_key = Address::generate(&env);
    let token = Address::generate(&env);
    let recipient = Address::generate(&env);
    let allowed_functions: Vec<Symbol> = Vec::from_array(&env, [Symbol::new(&env, "transfer")]);
    client.add_session_key(&session_key, &token, &allowed_functions, &Some(1_000i128), &10_000u64);

    // First transfer of 700 is under the cap on its own and must succeed, leaving 300 of
    // headroom — this also proves `spent` actually persists across separate __check_auth
    // calls, not just within one.
    let first_contexts = transfer_context(&env, &token, &client.address, &recipient, 700i128);
    let payload: Hash<32> = env.crypto().sha256(&Bytes::from_array(&env, &[1u8; 32]));
    let first_result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(
            env.clone(),
            payload.clone(),
            WalletSignature::Session(session_key.clone()),
            first_contexts,
        )
    });
    assert!(first_result.is_ok(), "the first transfer, alone under the cap, must authorize");

    // Second transfer of 400 would bring the cumulative total to 1,100 — over the 1,000 cap —
    // even though 400 alone is well under it.
    let second_contexts = transfer_context(&env, &token, &client.address, &recipient, 400i128);
    let second_result = env.as_contract(&client.address, || {
        SmartAccountWalletContract::__check_auth(
            env.clone(),
            payload,
            WalletSignature::Session(session_key),
            second_contexts,
        )
    });
    assert!(
        second_result.is_err(),
        "cumulative spend across separate authorizations must still be capped, not reset per call"
    );
}
