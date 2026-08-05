#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, Symbol, Vec, Val
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionData {
    pub allowed_contract: Address,
    pub expires_at: u64,
}

#[contract]
pub struct SmartAccountWalletContract;

#[contractimpl]
impl SmartAccountWalletContract {
    /// Initialize Smart Account with owner key & WebAuthn Passkey public key
    pub fn init(env: Env, owner: Address, passkey_pubkey: BytesN<64>) {
        if env.storage().instance().has(&symbol_short!("owner")) {
            panic!("Wallet already initialized");
        }
        owner.require_auth();
        env.storage().instance().set(&symbol_short!("owner"), &owner);
        env.storage().instance().set(&symbol_short!("passkey"), &passkey_pubkey);
        env.storage().instance().set(&symbol_short!("seq"), &0u64);
    }

    /// Add temporary session key with specific contract whitelist & expiration
    pub fn add_session_key(
        env: Env,
        session_key: Address,
        allowed_contract: Address,
        expires_at: u64,
    ) {
        let owner: Address = env.storage().instance().get(&symbol_short!("owner")).unwrap();
        owner.require_auth();

        let session_data = SessionData {
            allowed_contract: allowed_contract.clone(),
            expires_at,
        };
        
        let key = (symbol_short!("sess"), session_key.clone());
        env.storage().persistent().set(&key, &session_data);

        env.events().publish(
            (symbol_short!("sess_add"), session_key),
            (allowed_contract, expires_at),
        );
    }

    /// Execute transaction through Smart Account
    pub fn execute(
        env: Env,
        target: Address,
        function: Symbol,
        args: Vec<Val>,
    ) -> Val {
        let owner: Address = env.storage().instance().get(&symbol_short!("owner")).unwrap();
        owner.require_auth();

        let result: Val = env.invoke_contract(&target, &function, args);
        
        env.events().publish(
            (symbol_short!("wal_exec"), target),
            (function, result),
        );

        result
    }

    /// Get current primary owner
    pub fn get_owner(env: Env) -> Address {
        env.storage().instance().get(&symbol_short!("owner")).unwrap()
    }
}

#[cfg(test)]
mod test;
