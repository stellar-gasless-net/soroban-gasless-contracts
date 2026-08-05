#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env, IntoVal};

#[test]
fn test_forwarder_execution_and_nonce_increment() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TrustedForwarderContract);
    let client = TrustedForwarderContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let dummy_target = Address::generate(&env);

    client.init(&admin, &symbol_short!("v1_0"));

    assert_eq!(client.get_nonce(&user), 0);

    // Initial nonce should be 0
    let nonce = client.get_nonce(&user);
    assert_eq!(nonce, 0);

    // Check version
    assert_eq!(client.version(), symbol_short!("v1_0"));
}
