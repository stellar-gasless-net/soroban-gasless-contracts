#![no_std]
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env};

pub mod errors;
use errors::PaymasterError;

#[contract]
pub struct TokenPaymasterContract;

#[contractimpl]
impl TokenPaymasterContract {
    /// Initialize Paymaster with sponsored SAC token (e.g. USDC) and fee per transaction
    pub fn init(env: Env, admin: Address, token: Address, fee_per_tx: i128) {
        if env.storage().instance().has(&symbol_short!("admin")) {
            panic!("{}", PaymasterError::AlreadyInitialized as u32);
        }
        admin.require_auth();
        env.storage().instance().set(&symbol_short!("admin"), &admin);
        env.storage().instance().set(&symbol_short!("token"), &token);
        env.storage().instance().set(&symbol_short!("fee_tx"), &fee_per_tx);
    }

    /// Charge gas fee from user in token before relaying execution
    pub fn charge_fee(env: Env, user: Address, relayer_treasury: Address) {
        user.require_auth();

        let token_addr: Address = env.storage().instance().get(&symbol_short!("token")).unwrap();
        let fee_per_tx: i128 = env.storage().instance().get(&symbol_short!("fee_tx")).unwrap();

        let client = token::Client::new(&env, &token_addr);
        client.transfer(&user, &relayer_treasury, &fee_per_tx);

        env.events().publish(
            (symbol_short!("fee_pay"), user),
            (token_addr, fee_per_tx),
        );
    }

    /// Admin entrypoint to withdraw accumulated reserve balances from contract vault
    pub fn withdraw_reserves(env: Env, admin: Address, to: Address, amount: i128) {
        let stored_admin: Address = env.storage().instance().get(&symbol_short!("admin")).unwrap();
        if admin != stored_admin {
            panic!("{}", PaymasterError::UnauthorizedAdmin as u32);
        }
        admin.require_auth();

        let token_addr: Address = env.storage().instance().get(&symbol_short!("token")).unwrap();
        let client = token::Client::new(&env, &token_addr);
        client.transfer(&env.current_contract_address(), &to, &amount);

        env.events().publish(
            (symbol_short!("withdraw"), admin),
            (to, amount),
        );
    }
}

#[cfg(test)]
mod test;
