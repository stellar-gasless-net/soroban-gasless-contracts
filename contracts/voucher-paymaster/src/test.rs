#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn combine(env: &Env, index: u32, node: &BytesN<32>, sibling: &BytesN<32>) -> BytesN<32> {
    let mut data = Bytes::new(env);
    if index % 2 == 0 {
        data.append(&Bytes::from_array(env, &node.to_array()));
        data.append(&Bytes::from_array(env, &sibling.to_array()));
    } else {
        data.append(&Bytes::from_array(env, &sibling.to_array()));
        data.append(&Bytes::from_array(env, &node.to_array()));
    }
    BytesN::from_array(env, &env.crypto().sha256(&data).to_array())
}

struct Tree {
    root: BytesN<32>,
    leaves: [BytesN<32>; 4],
}

fn build_tree(env: &Env, leaves: [BytesN<32>; 4]) -> Tree {
    let h01 = combine(env, 0, &leaves[0], &leaves[1]);
    let h23 = combine(env, 2, &leaves[2], &leaves[3]);
    let root = combine(env, 0, &h01, &h23);
    Tree { root, leaves }
}

fn proof_for_leaf0(env: &Env, tree: &Tree) -> Vec<BytesN<32>> {
    let h23 = combine(env, 2, &tree.leaves[2], &tree.leaves[3]);
    Vec::from_array(env, [tree.leaves[1].clone(), h23])
}

fn setup(env: &Env) -> (VoucherPaymasterContractClient<'static>, Address) {
    let contract_id = env.register_contract(None, VoucherPaymasterContract);
    let client = VoucherPaymasterContractClient::new(env, &contract_id);
    let sponsor = Address::generate(env);
    (client, sponsor)
}

#[test]
fn test_validate_voucher_accepts_a_real_valid_merkle_proof() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sponsor) = setup(&env);
    let user = Address::generate(&env);

    let voucher_id = 1001u64;
    let max_fee = 500_000i128;
    let leaf0 = VoucherPaymasterContract::compute_leaf(&env, voucher_id, max_fee);
    let leaves = [
        leaf0,
        BytesN::from_array(&env, &[1u8; 32]),
        BytesN::from_array(&env, &[2u8; 32]),
        BytesN::from_array(&env, &[3u8; 32]),
    ];
    let tree = build_tree(&env, leaves);
    client.register_voucher_batch(&sponsor, &tree.root);

    let proof = proof_for_leaf0(&env, &tree);
    let result = client.validate_voucher(&sponsor, &user, &voucher_id, &max_fee, &proof, &0u32);
    assert!(result, "a genuine Merkle path to the sponsor's real registered batch must verify");
}

#[test]
#[should_panic(expected = "Voucher already claimed")]
fn test_validate_voucher_rejects_a_replayed_voucher_id() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sponsor) = setup(&env);
    let user = Address::generate(&env);

    let voucher_id = 1001u64;
    let max_fee = 500_000i128;
    let leaf0 = VoucherPaymasterContract::compute_leaf(&env, voucher_id, max_fee);
    let leaves = [
        leaf0,
        BytesN::from_array(&env, &[1u8; 32]),
        BytesN::from_array(&env, &[2u8; 32]),
        BytesN::from_array(&env, &[3u8; 32]),
    ];
    let tree = build_tree(&env, leaves);
    client.register_voucher_batch(&sponsor, &tree.root);
    let proof = proof_for_leaf0(&env, &tree);

    client.validate_voucher(&sponsor, &user, &voucher_id, &max_fee, &proof, &0u32);
    // Second redemption of the exact same voucher_id must be rejected even though the
    // proof itself is still perfectly valid — replay protection, not proof validity.
    client.validate_voucher(&sponsor, &user, &voucher_id, &max_fee, &proof, &0u32);
}

#[test]
fn test_validate_voucher_rejects_a_claimed_fee_the_sponsor_never_committed_to() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sponsor) = setup(&env);
    let user = Address::generate(&env);

    let voucher_id = 1001u64;
    let committed_max_fee = 500_000i128;
    let leaf0 = VoucherPaymasterContract::compute_leaf(&env, voucher_id, committed_max_fee);
    let leaves = [
        leaf0,
        BytesN::from_array(&env, &[1u8; 32]),
        BytesN::from_array(&env, &[2u8; 32]),
        BytesN::from_array(&env, &[3u8; 32]),
    ];
    let tree = build_tree(&env, leaves);
    client.register_voucher_batch(&sponsor, &tree.root);
    let proof = proof_for_leaf0(&env, &tree);

    // Same voucher_id and proof, but a caller-inflated max_fee the sponsor never signed off
    // on — the leaf this recomputes no longer matches what's in the tree, so it must fail.
    let claimed_max_fee = 5_000_000i128;
    let result = client.validate_voucher(&sponsor, &user, &voucher_id, &claimed_max_fee, &proof, &0u32);
    assert!(!result, "a fee cap the sponsor never committed to must not verify");
}

#[test]
#[should_panic]
fn test_validate_voucher_rejects_a_sponsor_with_no_registered_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sponsor) = setup(&env);
    let user = Address::generate(&env);

    client.validate_voucher(&sponsor, &user, &1001u64, &500_000i128, &Vec::new(&env), &0u32);
}
