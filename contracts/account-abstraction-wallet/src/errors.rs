use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum WalletError {
    AlreadyInitialized = 1,
    UnauthorizedOwner = 2,
    SessionExpired = 3,
    ContractNotWhitelisted = 4,
    InvalidPasskeySignature = 5,
    UnknownSessionKey = 6,
    FunctionNotWhitelisted = 7,
    SpendCapExceeded = 8,
}
