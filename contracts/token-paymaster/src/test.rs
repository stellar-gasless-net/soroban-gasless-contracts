#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Env};

#[test]
fn test_token_paymaster_initialization() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TokenPaymasterContract);
    let client = TokenPaymasterContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = Address::generate(&env);

    client.init(&admin, &token, &100_000i128);
}

fn setup_with_real_token(env: &Env) -> (TokenPaymasterContractClient<'static>, Address, Address, token::Client<'static>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_address = env.register_stellar_asset_contract_v2(token_admin.clone()).address();

    let contract_id = env.register_contract(None, TokenPaymasterContract);
    let client = TokenPaymasterContractClient::new(env, &contract_id);
    client.init(&admin, &token_address, &100_000i128);

    (client, admin, token_address.clone(), token::Client::new(env, &token_address))
}

#[test]
fn test_charge_fee_actually_moves_real_tokens_from_user_to_treasury() {
    let env = Env::default();
    let (client, _admin, token_address, token) = setup_with_real_token(&env);

    let user = Address::generate(&env);
    let treasury = Address::generate(&env);
    StellarAssetClient::new(&env, &token_address).mint(&user, &1_000_000i128);

    client.charge_fee(&user, &treasury);

    assert_eq!(token.balance(&user), 900_000i128, "the configured fee_per_tx must be deducted from the user");
    assert_eq!(token.balance(&treasury), 100_000i128, "the fee must land in the relayer treasury, not disappear");
}

#[test]
#[should_panic]
fn test_charge_fee_rejects_a_user_with_insufficient_balance() {
    let env = Env::default();
    let (client, _admin, token_address, _token) = setup_with_real_token(&env);

    let user = Address::generate(&env);
    let treasury = Address::generate(&env);
    // Mint less than fee_per_tx (100_000) — the underlying SAC transfer must panic rather
    // than silently short-paying the treasury.
    StellarAssetClient::new(&env, &token_address).mint(&user, &50_000i128);

    client.charge_fee(&user, &treasury);
}

#[test]
fn test_withdraw_reserves_lets_the_real_admin_move_real_contract_funds() {
    let env = Env::default();
    let (client, admin, token_address, token) = setup_with_real_token(&env);
    let contract_id = client.address.clone();

    StellarAssetClient::new(&env, &token_address).mint(&contract_id, &500_000i128);

    let payee = Address::generate(&env);
    client.withdraw_reserves(&admin, &payee, &200_000i128);

    assert_eq!(token.balance(&payee), 200_000i128);
    assert_eq!(token.balance(&contract_id), 300_000i128, "only the withdrawn amount should leave the contract's reserve");
}

#[test]
#[should_panic]
fn test_withdraw_reserves_rejects_a_caller_who_is_not_the_real_admin() {
    let env = Env::default();
    let (client, _admin, token_address, _token) = setup_with_real_token(&env);
    let contract_id = client.address.clone();
    StellarAssetClient::new(&env, &token_address).mint(&contract_id, &500_000i128);

    let impostor = Address::generate(&env);
    let payee = Address::generate(&env);
    // impostor never authorized as admin — must be rejected even though mock_all_auths()
    // would otherwise happily approve any require_auth() call.
    client.withdraw_reserves(&impostor, &payee, &200_000i128);
}
