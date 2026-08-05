#![no_std]
use soroban_sdk::{
    contract, contractimpl, symbol_short, vec, Address, BytesN, Env, Symbol, Vec, Val
};

#[contract]
pub struct TrustedForwarderContract;

#[contractimpl]
impl TrustedForwarderContract {
    /// Initialize the Trusted Forwarder with domain parameters
    pub fn init(env: Env, admin: Address, version: Symbol) {
        if env.storage().instance().has(&symbol_short!("admin")) {
            panic!("Already initialized");
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
            panic!("Forwarder: transaction expired deadline");
        }

        // 3. Verify and consume sequential nonce to prevent replay attacks
        let nonce_key = (symbol_short!("nonce"), user.clone());
        let current_nonce: u64 = env.storage().persistent().get(&nonce_key).unwrap_or(0);
        if nonce != current_nonce {
            panic!("Forwarder: invalid nonce sequence");
        }
        env.storage().persistent().set(&nonce_key, &(current_nonce + 1));

        // 4. Dispatch invocation to target contract
        let result: Val = env.invoke_contract(&target_contract, &function, args);

        // 5. Emit Forwarded Event for relayer indexer tracking
        env.events().publish(
            (symbol_short!("forward"), user, target_contract),
            (function, nonce, deadline),
        );

        result
    }

    /// Execute multiple meta-transactions atomically in a single batch
    pub fn execute_batch(
        env: Env,
        user: Address,
        targets: Vec<Address>,
        functions: Vec<Symbol>,
        args_list: Vec<Vec<Val>>,
        start_nonce: u64,
        deadline: u64,
    ) {
        user.require_auth();

        let len = targets.len();
        if len != functions.len() || len != args_list.len() {
            panic!("Batch parameters length mismatch");
        }

        let mut current_nonce = start_nonce;
        for i in 0..len {
            let target = targets.get(i).unwrap();
            let func = functions.get(i).unwrap();
            let args = args_list.get(i).unwrap();

            Self::execute_forwarded(
                env.clone(),
                user.clone(),
                target,
                func,
                args,
                current_nonce,
                deadline,
            );
            current_nonce += 1;
        }
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
