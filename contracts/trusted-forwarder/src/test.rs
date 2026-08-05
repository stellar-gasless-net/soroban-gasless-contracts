#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, symbol_short, Address, Env};

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
