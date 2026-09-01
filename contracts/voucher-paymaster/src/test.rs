#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Env};

#[test]
fn test_voucher_paymaster_validation() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, VoucherPaymasterContract);
    let client = VoucherPaymasterContractClient::new(&env, &contract_id);

    let sponsor = Address::generate(&env);
    let user = Address::generate(&env);

    let result = client.validate_voucher(&sponsor, &user, &1001u64, &500_000i128);
    assert_eq!(result, true);
}
