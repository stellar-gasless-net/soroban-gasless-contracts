#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec, Val
};

pub mod errors;
use errors::ForwarderError;

/// One call within an `execute_batch` request. Bundling target/function/args together (rather
/// than three parallel Vecs the caller has to keep in sync) makes a length mismatch between
/// them structurally impossible, not just checked for.
#[derive(Clone)]
#[contracttype]
pub struct BatchCall {
    pub target: Address,
    pub function: Symbol,
    pub args: Vec<Val>,
}

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
        user.require_auth();
        Self::check_and_consume_nonce(&env, &user, nonce, deadline);

        let result: Val = env.invoke_contract(&target_contract, &function, args);

        env.events().publish(
            (symbol_short!("forward"), user, target_contract),
            (function, nonce),
        );

        result
    }

    /// Execute a batch of forwarded calls atomically on behalf of a user who signed the whole
    /// batch off-chain in one payload — one signature, one nonce, several dispatches. Atomicity
    /// falls out of Soroban's own transaction semantics: if any call in the batch panics, the
    /// entire invocation (including the nonce bump) rolls back, the same way a single
    /// `execute_forwarded` call already does. There is no per-call rollback-and-continue; a
    /// caller who wants that should split the batch into separate meta-transactions instead.
    pub fn execute_batch(
        env: Env,
        user: Address,
        calls: Vec<BatchCall>,
        nonce: u64,
        deadline: u64,
    ) -> Vec<Val> {
        user.require_auth();

        if calls.is_empty() {
            panic!("{}", ForwarderError::BatchLengthMismatch as u32);
        }

        Self::check_and_consume_nonce(&env, &user, nonce, deadline);

        let mut results: Vec<Val> = Vec::new(&env);
        for call in calls.iter() {
            let result: Val = env.invoke_contract(&call.target, &call.function, call.args.clone());
            results.push_back(result);
        }

        env.events().publish(
            (symbol_short!("batch_fwd"), user),
            (results.len() as u32, nonce),
        );

        results
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

    /// Shared by execute_forwarded and execute_batch: enforces the deadline, verifies the
    /// caller-supplied nonce matches what's actually stored, then bumps and persists it. Kept
    /// as one function rather than duplicated in both entrypoints so this security-critical
    /// check can't silently drift between the single-call and batch paths.
    fn check_and_consume_nonce(env: &Env, user: &Address, nonce: u64, deadline: u64) {
        let current_time = env.ledger().timestamp();
        if current_time > deadline {
            panic!("{}", ForwarderError::ExpiredDeadline as u32);
        }

        let nonce_key = (symbol_short!("nonce"), user.clone());
        let current_nonce: u64 = env.storage().persistent().get(&nonce_key).unwrap_or(0);
        if nonce != current_nonce {
            panic!("{}", ForwarderError::InvalidNonceSequence as u32);
        }

        let new_nonce = current_nonce + 1;
        env.storage().persistent().set(&nonce_key, &new_nonce);

        // Extend storage TTL by 100,000 ledgers (approx 5 days) to keep persistent nonce active
        env.storage().persistent().extend_ttl(&nonce_key, 100_000, 100_000);
    }
}

#[cfg(test)]
mod test;
