#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, symbol_short, Address, Env, IntoVal, Vec};

#[test]
fn test_estimate_execution_overhead_with_no_args_returns_base_overhead() {
    let env = Env::default();
    let contract_id = env.register_contract(None, GasEstimatorContract);
    let client = GasEstimatorContractClient::new(&env, &contract_id);

    let target = Address::generate(&env);
    let empty_args: Vec<Val> = Vec::new(&env);

    // Documents current (placeholder) behavior: base_overhead(5000) + 0 args * 100.
    assert_eq!(
        client.estimate_execution_overhead(&target, &symbol_short!("noop"), &empty_args),
        5000
    );
}

#[test]
fn test_estimate_execution_overhead_scales_linearly_with_arg_count() {
    let env = Env::default();
    let contract_id = env.register_contract(None, GasEstimatorContract);
    let client = GasEstimatorContractClient::new(&env, &contract_id);

    let target = Address::generate(&env);
    let three_args: Vec<Val> = Vec::from_array(&env, [1i128.into_val(&env), 2i128.into_val(&env), 3i128.into_val(&env)]);

    // Documents current (placeholder) behavior: 5000 + 3 * 100 = 5300. This is NOT a real
    // gas measurement — see the README/roadmap note that this contract is a stub.
    assert_eq!(
        client.estimate_execution_overhead(&target, &symbol_short!("noop"), &three_args),
        5300
    );
}
