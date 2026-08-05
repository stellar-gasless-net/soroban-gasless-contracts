#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env};

#[contract]
pub struct VoucherPaymasterContract;

#[contractimpl]
impl VoucherPaymasterContract {
    /// Verify sponsor coupon/voucher signature before allowing gasless execution
    pub fn validate_voucher(
        env: Env,
        sponsor: Address,
        user: Address,
        voucher_id: u64,
        max_fee: i128,
    ) -> bool {
        sponsor.require_auth();
        
        let key = (symbol_short!("used"), voucher_id);
        if env.storage().persistent().has(&key) {
            panic!("Voucher already claimed");
        }
        
        env.storage().persistent().set(&key, &true);
        env.events().publish(
            (symbol_short!("voucher"), sponsor, user),
            (voucher_id, max_fee),
        );

        true
    }
}

#[cfg(test)]
mod test;
