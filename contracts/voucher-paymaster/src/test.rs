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
    let leaf0 = VoucherPaymasterContract::compute_leaf(&env, &user, voucher_id, max_fee);
    let leaves = [
        leaf0,
        BytesN::from_array(&env, &[1u8; 32]),
        BytesN::from_array(&env, &[2u8; 32]),
        BytesN::from_array(&env, &[3u8; 32]),
    ];
    let tree = build_tree(&env, leaves);
    let version = client.register_voucher_batch(&sponsor, &tree.root);
    assert_eq!(version, 1, "a sponsor's first batch is version 1");

    let proof = proof_for_leaf0(&env, &tree);
    let result = client.validate_voucher(&sponsor, &user, &version, &voucher_id, &max_fee, &proof, &0u32);
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
    let leaf0 = VoucherPaymasterContract::compute_leaf(&env, &user, voucher_id, max_fee);
    let leaves = [
        leaf0,
        BytesN::from_array(&env, &[1u8; 32]),
        BytesN::from_array(&env, &[2u8; 32]),
        BytesN::from_array(&env, &[3u8; 32]),
    ];
    let tree = build_tree(&env, leaves);
    let version = client.register_voucher_batch(&sponsor, &tree.root);
    let proof = proof_for_leaf0(&env, &tree);

    client.validate_voucher(&sponsor, &user, &version, &voucher_id, &max_fee, &proof, &0u32);
    // Second redemption of the exact same voucher_id in the same batch must be rejected even
    // though the proof itself is still perfectly valid — replay protection, not proof validity.
    client.validate_voucher(&sponsor, &user, &version, &voucher_id, &max_fee, &proof, &0u32);
}

#[test]
fn test_validate_voucher_rejects_a_claimed_fee_the_sponsor_never_committed_to() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sponsor) = setup(&env);
    let user = Address::generate(&env);

    let voucher_id = 1001u64;
    let committed_max_fee = 500_000i128;
    let leaf0 = VoucherPaymasterContract::compute_leaf(&env, &user, voucher_id, committed_max_fee);
    let leaves = [
        leaf0,
        BytesN::from_array(&env, &[1u8; 32]),
        BytesN::from_array(&env, &[2u8; 32]),
        BytesN::from_array(&env, &[3u8; 32]),
    ];
    let tree = build_tree(&env, leaves);
    let version = client.register_voucher_batch(&sponsor, &tree.root);
    let proof = proof_for_leaf0(&env, &tree);

    // Same voucher_id and proof, but a caller-inflated max_fee the sponsor never signed off
    // on — the leaf this recomputes no longer matches what's in the tree, so it must fail.
    let claimed_max_fee = 5_000_000i128;
    let result = client.validate_voucher(&sponsor, &user, &version, &voucher_id, &claimed_max_fee, &proof, &0u32);
    assert!(!result, "a fee cap the sponsor never committed to must not verify");
}

#[test]
#[should_panic]
fn test_validate_voucher_rejects_a_sponsor_with_no_registered_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sponsor) = setup(&env);
    let user = Address::generate(&env);

    client.validate_voucher(&sponsor, &user, &1u32, &1001u64, &500_000i128, &Vec::new(&env), &0u32);
}

#[test]
fn test_validate_voucher_does_not_let_one_sponsors_redemption_block_another_sponsors_voucher_with_the_same_id() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, VoucherPaymasterContract);
    let client = VoucherPaymasterContractClient::new(&env, &contract_id);
    let sponsor_a = Address::generate(&env);
    let sponsor_b = Address::generate(&env);
    let user = Address::generate(&env);

    // Both sponsors independently choose to number their first voucher `1001` — they have
    // no way to coordinate on this, and shouldn't need to.
    let voucher_id = 1001u64;
    let max_fee = 500_000i128;

    let leaf_a = VoucherPaymasterContract::compute_leaf(&env, &user, voucher_id, max_fee);
    let tree_a = build_tree(
        &env,
        [
            leaf_a,
            BytesN::from_array(&env, &[1u8; 32]),
            BytesN::from_array(&env, &[2u8; 32]),
            BytesN::from_array(&env, &[3u8; 32]),
        ],
    );
    let version_a = client.register_voucher_batch(&sponsor_a, &tree_a.root);
    let proof_a = proof_for_leaf0(&env, &tree_a);
    assert!(client.validate_voucher(&sponsor_a, &user, &version_a, &voucher_id, &max_fee, &proof_a, &0u32));

    // Sponsor B's completely unrelated batch happens to also use voucher_id 1001. Redeeming
    // sponsor A's voucher above must not have any effect on sponsor B's.
    let leaf_b = VoucherPaymasterContract::compute_leaf(&env, &user, voucher_id, max_fee);
    let tree_b = build_tree(
        &env,
        [
            leaf_b,
            BytesN::from_array(&env, &[9u8; 32]),
            BytesN::from_array(&env, &[8u8; 32]),
            BytesN::from_array(&env, &[7u8; 32]),
        ],
    );
    let version_b = client.register_voucher_batch(&sponsor_b, &tree_b.root);
    let proof_b = proof_for_leaf0(&env, &tree_b);
    assert!(
        client.validate_voucher(&sponsor_b, &user, &version_b, &voucher_id, &max_fee, &proof_b, &0u32),
        "sponsor B's voucher must redeem independently of sponsor A's same-numbered voucher"
    );
}

#[test]
fn test_rotating_to_a_new_batch_does_not_orphan_an_unredeemed_voucher_from_the_old_one() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sponsor) = setup(&env);
    let user = Address::generate(&env);

    // Batch 1: sponsor's first ever voucher batch.
    let old_voucher_id = 1001u64;
    let max_fee = 500_000i128;
    let old_leaf = VoucherPaymasterContract::compute_leaf(&env, &user, old_voucher_id, max_fee);
    let old_tree = build_tree(
        &env,
        [
            old_leaf,
            BytesN::from_array(&env, &[1u8; 32]),
            BytesN::from_array(&env, &[2u8; 32]),
            BytesN::from_array(&env, &[3u8; 32]),
        ],
    );
    let old_version = client.register_voucher_batch(&sponsor, &old_tree.root);
    assert_eq!(client.get_latest_batch_version(&sponsor), old_version);

    // Sponsor rotates to a brand new batch (e.g. next month's voucher run) before the user
    // above ever redeemed their batch-1 voucher.
    let new_voucher_id = 1001u64; // deliberately reuses the same id — must not collide
    let new_leaf = VoucherPaymasterContract::compute_leaf(&env, &user, new_voucher_id, max_fee);
    let new_tree = build_tree(
        &env,
        [
            new_leaf,
            BytesN::from_array(&env, &[9u8; 32]),
            BytesN::from_array(&env, &[8u8; 32]),
            BytesN::from_array(&env, &[7u8; 32]),
        ],
    );
    let new_version = client.register_voucher_batch(&sponsor, &new_tree.root);
    assert_eq!(new_version, old_version + 1);
    assert_eq!(client.get_latest_batch_version(&sponsor), new_version);

    // The whole point: the OLD, still-unredeemed voucher from batch 1 must still redeem
    // successfully, even though the sponsor has since moved on to batch 2.
    let old_proof = proof_for_leaf0(&env, &old_tree);
    assert!(
        client.validate_voucher(&sponsor, &user, &old_version, &old_voucher_id, &max_fee, &old_proof, &0u32),
        "an unredeemed voucher from a previous batch must remain redeemable after rotation"
    );

    // And the new batch's same-numbered voucher redeems independently too.
    let new_proof = proof_for_leaf0(&env, &new_tree);
    assert!(
        client.validate_voucher(&sponsor, &user, &new_version, &new_voucher_id, &max_fee, &new_proof, &0u32),
        "the new batch's voucher must redeem independently of the old batch's same-numbered one"
    );
}

#[test]
fn test_validate_voucher_rejects_a_different_user_redeeming_the_same_proof() {
    // Regression test for a real front-running/theft bug: the leaf used to be
    // sha256(voucher_id || max_fee) with no user binding, so anyone who observed a valid
    // (voucher_id, max_fee, proof) combination in transit (e.g. a relayer forwarding it)
    // could call validate_voucher with their OWN address instead of the intended user's, and
    // it would still verify — stealing the voucher's sponsored transaction and permanently
    // denying the real recipient (require_auth() only proves the caller controls whichever
    // address they choose to pass in, it never proved that address was who the voucher was
    // actually for). Binding `user` into the leaf fixes this: the same proof must only verify
    // for the specific address the sponsor committed to.
    let env = Env::default();
    env.mock_all_auths();
    let (client, sponsor) = setup(&env);
    let real_user = Address::generate(&env);
    let attacker = Address::generate(&env);

    let voucher_id = 1001u64;
    let max_fee = 500_000i128;
    let leaf0 = VoucherPaymasterContract::compute_leaf(&env, &real_user, voucher_id, max_fee);
    let leaves = [
        leaf0,
        BytesN::from_array(&env, &[1u8; 32]),
        BytesN::from_array(&env, &[2u8; 32]),
        BytesN::from_array(&env, &[3u8; 32]),
    ];
    let tree = build_tree(&env, leaves);
    let version = client.register_voucher_batch(&sponsor, &tree.root);
    let proof = proof_for_leaf0(&env, &tree);

    // The attacker observed (voucher_id, max_fee, proof) and tries to redeem it as themselves.
    let stolen = client.validate_voucher(&sponsor, &attacker, &version, &voucher_id, &max_fee, &proof, &0u32);
    assert!(!stolen, "a proof committed to real_user must not verify for a different address");

    // The real user's own redemption of the exact same proof must still succeed.
    let genuine = client.validate_voucher(&sponsor, &real_user, &version, &voucher_id, &max_fee, &proof, &0u32);
    assert!(genuine, "the actual intended recipient must still be able to redeem their own voucher");
}

#[test]
fn test_get_latest_batch_version_is_zero_before_any_batch_is_registered() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, sponsor) = setup(&env);

    assert_eq!(client.get_latest_batch_version(&sponsor), 0);
}
