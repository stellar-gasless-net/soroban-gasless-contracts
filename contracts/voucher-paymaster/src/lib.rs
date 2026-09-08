#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env, Vec};

/// Bound on Merkle proof depth (supports batches up to 2^32 vouchers) so verification cost
/// stays predictable.
const MAX_PROOF_DEPTH: usize = 32;
/// Stellar strkey addresses (both `G...` account keys and `C...` contract keys) are always
/// exactly 56 ASCII characters — 32-byte payload + 1 version byte + 2-byte checksum, base32
/// encoded. This is a protocol constant, not a guess (same constant used in zkident's
/// credential_verifier for the same reason).
const STRKEY_LEN: usize = 56;

#[contracttype]
pub enum DataKey {
    /// Root for one specific (sponsor, batch_version) pair. Each `register_voucher_batch`
    /// call gets its own version instead of overwriting the previous one, so vouchers from
    /// an earlier batch stay redeemable after a sponsor rotates to a new batch — only the
    /// sponsor's "latest" pointer moves.
    SponsorRoot(Address, u32),
    /// The most recently registered batch version for a sponsor. The next
    /// `register_voucher_batch` call assigns latest + 1, starting at 1 for a sponsor's first
    /// batch (0 means "no batch registered yet").
    SponsorLatestVersion(Address),
    /// Keyed by (sponsor, batch_version, voucher_id) — including batch_version means two
    /// different batches from the same sponsor (or two different sponsors) can safely reuse
    /// the same voucher_id without colliding, and a voucher only blocks redemption of the
    /// exact batch it actually belongs to.
    Used(Address, u32, u64),
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
    ///
    /// Returns the new batch's version number, which callers must hand out alongside each
    /// voucher's id/proof so redeemers know which registered root to check it against — this
    /// call never overwrites or invalidates a previous batch, it only adds a new one.
    pub fn register_voucher_batch(env: Env, sponsor: Address, root: BytesN<32>) -> u32 {
        sponsor.require_auth();

        let latest_key = DataKey::SponsorLatestVersion(sponsor.clone());
        let prev_version: u32 = env.storage().persistent().get(&latest_key).unwrap_or(0);
        let new_version = prev_version + 1;

        env.storage()
            .persistent()
            .set(&DataKey::SponsorRoot(sponsor.clone(), new_version), &root);
        env.storage().persistent().set(&latest_key, &new_version);

        env.events()
            .publish((symbol_short!("vb_reg"), sponsor), (new_version, root));
        new_version
    }

    /// The most recently registered batch version for `sponsor`, or 0 if they've never
    /// registered one. Tooling issuing new vouchers reads this before calling
    /// `register_voucher_batch` again, and off-chain code preparing a redemption reads it to
    /// find the current batch — though any earlier version is still independently valid.
    pub fn get_latest_batch_version(env: Env, sponsor: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::SponsorLatestVersion(sponsor))
            .unwrap_or(0)
    }

    /// Verifies `(voucher_id, max_fee)` was genuinely included in `sponsor`'s batch numbered
    /// `batch_version`, via a real Merkle path — not just an opaque ID the caller could
    /// invent. Any batch version the sponsor has ever registered can still be redeemed
    /// against, not just their latest — rotating to a new batch no longer orphans unredeemed
    /// vouchers from an older one. This replaces the old design, which only checked a
    /// `sponsor.require_auth()` signature live on every single redemption (defeating the
    /// point of a pre-approved batch) and never verified `max_fee` against anything the
    /// sponsor actually committed to, so a caller could claim any fee cap for a given
    /// voucher ID.
    pub fn validate_voucher(
        env: Env,
        sponsor: Address,
        user: Address,
        batch_version: u32,
        voucher_id: u64,
        max_fee: i128,
        merkle_proof: Vec<BytesN<32>>,
        leaf_index: u32,
    ) -> bool {
        // `user` is bound directly into the leaf hash (see compute_leaf), not just passed as
        // an argument here — require_auth() alone only proves the caller controls whichever
        // address they pass in, it does NOT prove that address is the voucher's intended
        // recipient. Without the leaf binding, anyone observing a valid (voucher_id, max_fee,
        // proof) combination could call this with their own address instead of the real
        // user's, and it would still verify: the Merkle root never encoded who the voucher
        // was for. Binding `user` into the leaf means a proof only verifies for the specific
        // address the sponsor actually committed to.
        user.require_auth();

        if (merkle_proof.len() as usize) > MAX_PROOF_DEPTH {
            panic!("Merkle proof is deeper than the supported maximum");
        }

        let used_key = DataKey::Used(sponsor.clone(), batch_version, voucher_id);
        if env.storage().persistent().has(&used_key) {
            panic!("Voucher already claimed");
        }

        let root: BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::SponsorRoot(sponsor.clone(), batch_version))
            .expect("sponsor has no registered voucher batch with this version");

        let leaf = Self::compute_leaf(&env, &user, voucher_id, max_fee);
        let computed_root = Self::compute_root(&env, &leaf, &merkle_proof, leaf_index);
        if computed_root != root {
            return false;
        }

        env.storage().persistent().set(&used_key, &true);
        env.events().publish(
            (symbol_short!("voucher"), sponsor, user),
            (batch_version, voucher_id, max_fee),
        );

        true
    }

    /// Leaf is bound to `user` so a Merkle proof only verifies for the specific address the
    /// sponsor committed to — see the comment in `validate_voucher` for why this matters.
    fn compute_leaf(env: &Env, user: &Address, voucher_id: u64, max_fee: i128) -> BytesN<32> {
        let addr_str = user.to_string();
        if addr_str.len() as usize != STRKEY_LEN {
            panic!("unexpected address strkey length");
        }
        let mut addr_buf = [0u8; STRKEY_LEN];
        addr_str.copy_into_slice(&mut addr_buf);

        let mut data = Bytes::from_slice(env, b"gasless:voucher-leaf:v2:");
        data.append(&Bytes::from_slice(env, &addr_buf));
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
