#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env, Vec};

/// Bound on Merkle proof depth (supports batches up to 2^32 vouchers) so verification cost
/// stays predictable.
const MAX_PROOF_DEPTH: usize = 32;

#[contracttype]
pub enum DataKey {
    /// One active voucher-batch root per sponsor. Registering a new one replaces it —
    /// there's no versioning; a sponsor rotating batches mid-flight would invalidate any
    /// unredeemed vouchers from the previous batch.
    SponsorRoot(Address),
    /// Keyed by (sponsor, voucher_id), not voucher_id alone — two independent sponsors
    /// numbering their vouchers the same way (e.g. both starting at id=1) must not be able
    /// to grief each other by exhausting a voucher_id that belongs to a different sponsor's
    /// batch entirely.
    Used(Address, u64),
}

#[contract]
pub struct VoucherPaymasterContract;

#[contractimpl]
impl VoucherPaymasterContract {
    /// A sponsor commits to a whole batch of vouchers at once by publishing the Merkle root
    /// over their leaves. This is the sponsor's one live signature for the batch — individual
    /// vouchers then redeem via `validate_voucher` without the sponsor needing to be present
    /// or sign again, which is the actual point of a voucher system (bulk-approve now,
    /// redeem later, potentially by a relayer on the user's behalf).
    pub fn register_voucher_batch(env: Env, sponsor: Address, root: BytesN<32>) {
        sponsor.require_auth();
        env.storage().persistent().set(&DataKey::SponsorRoot(sponsor.clone()), &root);
        env.events().publish((symbol_short!("vb_reg"), sponsor), root);
    }

    /// Verifies `(voucher_id, max_fee)` was genuinely included in `sponsor`'s registered
    /// batch, via a real Merkle path — not just an opaque ID the caller could invent. This
    /// replaces the old design, which only checked a `sponsor.require_auth()` signature
    /// live on every single redemption (defeating the point of a pre-approved batch) and
    /// never verified `max_fee` against anything the sponsor actually committed to, so a
    /// caller could claim any fee cap for a given voucher ID.
    pub fn validate_voucher(
        env: Env,
        sponsor: Address,
        user: Address,
        voucher_id: u64,
        max_fee: i128,
        merkle_proof: Vec<BytesN<32>>,
        leaf_index: u32,
    ) -> bool {
        // Without this, anyone observing a would-be-valid (user, voucher_id, max_fee, proof)
        // combination before it lands on-chain could front-run and call this themselves,
        // permanently marking the voucher used and denying the real user their subsidized
        // transaction — a griefing vector, not a fund-theft one, but a real one.
        user.require_auth();

        if (merkle_proof.len() as usize) > MAX_PROOF_DEPTH {
            panic!("Merkle proof is deeper than the supported maximum");
        }

        let used_key = DataKey::Used(sponsor.clone(), voucher_id);
        if env.storage().persistent().has(&used_key) {
            panic!("Voucher already claimed");
        }

        let root: BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::SponsorRoot(sponsor.clone()))
            .expect("sponsor has no registered voucher batch");

        let leaf = Self::compute_leaf(&env, voucher_id, max_fee);
        let computed_root = Self::compute_root(&env, &leaf, &merkle_proof, leaf_index);
        if computed_root != root {
            return false;
        }

        env.storage().persistent().set(&used_key, &true);
        env.events().publish(
            (symbol_short!("voucher"), sponsor, user),
            (voucher_id, max_fee),
        );

        true
    }

    fn compute_leaf(env: &Env, voucher_id: u64, max_fee: i128) -> BytesN<32> {
        let mut data = Bytes::from_slice(env, b"gasless:voucher-leaf:v1:");
        data.append(&Bytes::from_array(env, &voucher_id.to_be_bytes()));
        data.append(&Bytes::from_array(env, &max_fee.to_be_bytes()));
        BytesN::from_array(env, &env.crypto().sha256(&data).to_array())
    }

    fn compute_root(
        env: &Env,
        leaf: &BytesN<32>,
        proof: &Vec<BytesN<32>>,
        leaf_index: u32,
    ) -> BytesN<32> {
        let mut current = leaf.clone();
        let mut index = leaf_index;
        for sibling in proof.iter() {
            let mut combined = Bytes::new(env);
            if index % 2 == 0 {
                combined.append(&Bytes::from_array(env, &current.to_array()));
                combined.append(&Bytes::from_array(env, &sibling.to_array()));
            } else {
                combined.append(&Bytes::from_array(env, &sibling.to_array()));
                combined.append(&Bytes::from_array(env, &current.to_array()));
            }
            current = BytesN::from_array(env, &env.crypto().sha256(&combined).to_array());
            index /= 2;
        }
        current
    }
}

#[cfg(test)]
mod test;
