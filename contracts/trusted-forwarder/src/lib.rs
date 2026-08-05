#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, vec, Address, BytesN, Env, Symbol, Vec};

#[contract]
pub struct TrustedForwarderContract;

#[contractimpl]
impl TrustedForwarderContract {
    /// Execute a forwarded meta-transaction on behalf of a user who signed the payload off-chain.
    pub fn execute_forwarded(
        env: Env,
        user: Address,
        target_contract: Address,
        function: Symbol,
        args: Vec<soroban_sdk::Val>,
        nonce: u64,
        deadline: u64,
    ) {
        // 1. Verify user authorization signature
        user.require_auth();

        // 2. Enforce deadline safety check
        let current_time = env.ledger().timestamp();
        if current_time > deadline {
            panic!("Forwarder: transaction expired deadline");
        }

        // 3. Verify and consume nonce to prevent replay attacks
        let nonce_key = (symbol_short!("nonce"), user.clone());
        let current_nonce: u64 = env.storage().persistent().get(&nonce_key).unwrap_or(0);
        if nonce != current_nonce {
            panic!("Forwarder: invalid nonce sequence");
        }
        env.storage().persistent().set(&nonce_key, &(current_nonce + 1));

        // 4. Dispatch call to target contract
        let _result: soroban_sdk::Val = env.invoke_contract(&target_contract, &function, args);

        // 5. Emit Forwarded Event for relayer indexer tracking
        env.events().publish(
            (symbol_short!("forward"), user, target_contract),
            (function, nonce),
        );
    }

    /// Read the current expected nonce for a given user address.
    pub fn get_nonce(env: Env, user: Address) -> u64 {
        let nonce_key = (symbol_short!("nonce"), user);
        env.storage().persistent().get(&nonce_key).unwrap_or(0)
    }
}

#[cfg(test)]
mod test;
