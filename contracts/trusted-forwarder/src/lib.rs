#![no_std]
use soroban_sdk::{
    contract, contractimpl, symbol_short, Address, Env, Symbol, Vec, Val
};

pub mod errors;
use errors::ForwarderError;

#[contract]
pub struct TrustedForwarderContract;

#[contractimpl]
impl TrustedForwarderContract {
    /// Initialize the Trusted Forwarder with domain parameters
    pub fn init(env: Env, admin: Address, version: Symbol) {
        if env.storage().instance().has(&symbol_short!("admin")) {
            panic!("{}", ForwarderError::AlreadyInitialized as u32);
        }
        admin.require_auth();
        env.storage().instance().set(&symbol_short!("admin"), &admin);
        env.storage().instance().set(&symbol_short!("ver"), &version);
    }

    /// Execute a forwarded meta-transaction on behalf of a user who signed the payload off-chain.
    pub fn execute_forwarded(
        env: Env,
        user: Address,
        target_contract: Address,
        function: Symbol,
        args: Vec<Val>,
        nonce: u64,
        deadline: u64,
    ) -> Val {
        // 1. Verify user authorization signature
        user.require_auth();

        // 2. Enforce deadline safety check against current ledger timestamp
        let current_time = env.ledger().timestamp();
        if current_time > deadline {
            panic!("{}", ForwarderError::ExpiredDeadline as u32);
        }

        // 3. Verify and consume sequential nonce to prevent replay attacks
        let nonce_key = (symbol_short!("nonce"), user.clone());
        let current_nonce: u64 = env.storage().persistent().get(&nonce_key).unwrap_or(0);
        if nonce != current_nonce {
            panic!("{}", ForwarderError::InvalidNonceSequence as u32);
        }
        env.storage().persistent().set(&nonce_key, &(current_nonce + 1));

        // 4. Dispatch invocation to target contract
        let result: Val = env.invoke_contract(&target_contract, &function, args);

        // 5. Emit Forwarded Event for relayer indexer tracking
        env.events().publish(
            (symbol_short!("forward"), user, target_contract),
            (function, nonce),
        );

        result
    }

    /// Read the current expected nonce for a given user address.
    pub fn get_nonce(env: Env, user: Address) -> u64 {
        let nonce_key = (symbol_short!("nonce"), user);
        env.storage().persistent().get(&nonce_key).unwrap_or(0)
    }

    /// Read protocol version
    pub fn version(env: Env) -> Symbol {
        env.storage().instance().get(&symbol_short!("ver")).unwrap_or(symbol_short!("v1_0"))
    }
}

#[cfg(test)]
mod test;
