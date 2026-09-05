#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger as _}, symbol_short, Address, Env, IntoVal, Vec};

// Minimal target contract used only to exercise execute_forwarded's dispatch path —
// the forwarder itself doesn't ship any target, so tests need one to invoke against.
#[contract]
struct DummyTarget;

#[contractimpl]
impl DummyTarget {
    pub fn increment(_env: Env, x: u32) -> u32 {
        x + 1
    }

    // Used only to prove execute_batch is atomic: a batch containing this call must leave no
    // trace of any earlier call in the same batch, including the nonce bump.
    pub fn will_fail(_env: Env) -> u32 {
        panic!("DummyTarget::will_fail always panics")
    }
}

#[test]
fn test_forwarder_execution_and_nonce_increment() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TrustedForwarderContract);
    let client = TrustedForwarderContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let _dummy_target = Address::generate(&env);

    client.init(&admin, &symbol_short!("v1_0"));

    assert_eq!(client.get_nonce(&user), 0);
    assert_eq!(client.version(), symbol_short!("v1_0"));
}

#[test]
fn test_execute_forwarded_dispatches_to_target_and_increments_nonce() {
    let env = Env::default();
    env.mock_all_auths();

    let forwarder_id = env.register_contract(None, TrustedForwarderContract);
    let forwarder = TrustedForwarderContractClient::new(&env, &forwarder_id);
    let target_id = env.register_contract(None, DummyTarget);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    forwarder.init(&admin, &symbol_short!("v1_0"));

    let args: Vec<Val> = Vec::from_array(&env, [41u32.into_val(&env)]);
    let deadline = env.ledger().timestamp() + 1000;

    let result: Val = forwarder.execute_forwarded(
        &user,
        &target_id,
        &symbol_short!("increment"),
        &args,
        &0u64,
        &deadline,
    );
    let result_u32: u32 = result.into_val(&env);

    assert_eq!(result_u32, 42);
    assert_eq!(forwarder.get_nonce(&user), 1);
}

#[test]
#[should_panic]
fn test_execute_forwarded_rejects_a_replayed_nonce() {
    let env = Env::default();
    env.mock_all_auths();

    let forwarder_id = env.register_contract(None, TrustedForwarderContract);
    let forwarder = TrustedForwarderContractClient::new(&env, &forwarder_id);
    let target_id = env.register_contract(None, DummyTarget);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    forwarder.init(&admin, &symbol_short!("v1_0"));

    let args: Vec<Val> = Vec::from_array(&env, [1u32.into_val(&env)]);
    let deadline = env.ledger().timestamp() + 1000;

    // First call at nonce 0 succeeds and advances the stored nonce to 1.
    forwarder.execute_forwarded(&user, &target_id, &symbol_short!("increment"), &args, &0u64, &deadline);
    // Replaying nonce 0 again must be rejected — this is the whole point of the guard.
    forwarder.execute_forwarded(&user, &target_id, &symbol_short!("increment"), &args, &0u64, &deadline);
}

#[test]
#[should_panic]
fn test_execute_forwarded_rejects_an_expired_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(10_000);

    let forwarder_id = env.register_contract(None, TrustedForwarderContract);
    let forwarder = TrustedForwarderContractClient::new(&env, &forwarder_id);
    let target_id = env.register_contract(None, DummyTarget);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    forwarder.init(&admin, &symbol_short!("v1_0"));

    let args: Vec<Val> = Vec::from_array(&env, [1u32.into_val(&env)]);

    // Deadline is in the past relative to the current ledger timestamp.
    forwarder.execute_forwarded(&user, &target_id, &symbol_short!("increment"), &args, &0u64, &5_000u64);
}

#[test]
fn test_execute_batch_dispatches_every_call_and_increments_nonce_once() {
    let env = Env::default();
    env.mock_all_auths();

    let forwarder_id = env.register_contract(None, TrustedForwarderContract);
    let forwarder = TrustedForwarderContractClient::new(&env, &forwarder_id);
    let target_id = env.register_contract(None, DummyTarget);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    forwarder.init(&admin, &symbol_short!("v1_0"));

    let deadline = env.ledger().timestamp() + 1000;
    let calls: Vec<BatchCall> = Vec::from_array(
        &env,
        [
            BatchCall {
                target: target_id.clone(),
                function: symbol_short!("increment"),
                args: Vec::from_array(&env, [1u32.into_val(&env)]),
            },
            BatchCall {
                target: target_id.clone(),
                function: symbol_short!("increment"),
                args: Vec::from_array(&env, [41u32.into_val(&env)]),
            },
        ],
    );

    let results = forwarder.execute_batch(&user, &calls, &0u64, &deadline);

    assert_eq!(results.len(), 2);
    let first: u32 = results.get(0).unwrap().into_val(&env);
    let second: u32 = results.get(1).unwrap().into_val(&env);
    assert_eq!(first, 2u32);
    assert_eq!(second, 42u32);
    // One batch, one signature, one nonce consumed — not one per call.
    assert_eq!(forwarder.get_nonce(&user), 1);
}

#[test]
fn test_execute_batch_is_atomic_a_failing_call_rolls_back_the_whole_batch_including_the_nonce() {
    let env = Env::default();
    env.mock_all_auths();

    let forwarder_id = env.register_contract(None, TrustedForwarderContract);
    let forwarder = TrustedForwarderContractClient::new(&env, &forwarder_id);
    let target_id = env.register_contract(None, DummyTarget);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    forwarder.init(&admin, &symbol_short!("v1_0"));

    let deadline = env.ledger().timestamp() + 1000;
    let calls: Vec<BatchCall> = Vec::from_array(
        &env,
        [
            BatchCall {
                target: target_id.clone(),
                function: symbol_short!("increment"),
                args: Vec::from_array(&env, [1u32.into_val(&env)]),
            },
            BatchCall {
                target: target_id.clone(),
                function: symbol_short!("will_fail"),
                args: Vec::new(&env),
            },
        ],
    );

    let outcome = forwarder.try_execute_batch(&user, &calls, &0u64, &deadline);
    assert!(outcome.is_err(), "a batch containing a failing call must fail as a whole");

    // Nothing from the failed batch — including the nonce bump the first call's own success
    // would otherwise have triggered — actually took effect.
    assert_eq!(forwarder.get_nonce(&user), 0);
}

#[test]
#[should_panic]
fn test_execute_batch_rejects_an_empty_batch() {
    let env = Env::default();
    env.mock_all_auths();

    let forwarder_id = env.register_contract(None, TrustedForwarderContract);
    let forwarder = TrustedForwarderContractClient::new(&env, &forwarder_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    forwarder.init(&admin, &symbol_short!("v1_0"));

    let deadline = env.ledger().timestamp() + 1000;
    let empty_calls: Vec<BatchCall> = Vec::new(&env);

    forwarder.execute_batch(&user, &empty_calls, &0u64, &deadline);
}

#[test]
#[should_panic]
fn test_execute_batch_rejects_a_replayed_nonce() {
    let env = Env::default();
    env.mock_all_auths();

    let forwarder_id = env.register_contract(None, TrustedForwarderContract);
    let forwarder = TrustedForwarderContractClient::new(&env, &forwarder_id);
    let target_id = env.register_contract(None, DummyTarget);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    forwarder.init(&admin, &symbol_short!("v1_0"));

    let deadline = env.ledger().timestamp() + 1000;
    let calls: Vec<BatchCall> = Vec::from_array(
        &env,
        [BatchCall {
            target: target_id.clone(),
            function: symbol_short!("increment"),
            args: Vec::from_array(&env, [1u32.into_val(&env)]),
        }],
    );

    forwarder.execute_batch(&user, &calls, &0u64, &deadline);
    // Replaying the same nonce a second batch must be rejected, same as execute_forwarded.
    forwarder.execute_batch(&user, &calls, &0u64, &deadline);
}
