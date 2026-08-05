#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env, BytesN};

#[test]
fn test_smart_account_init_and_owner() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, SmartAccountWalletContract);
    let client = SmartAccountWalletContractClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let passkey = BytesN::from_array(&env, &[0u8; 64]);

    client.init(&owner, &passkey);

    assert_eq!(client.get_owner(), owner);
}
