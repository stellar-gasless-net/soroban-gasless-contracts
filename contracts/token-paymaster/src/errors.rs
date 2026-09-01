use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PaymasterError {
    AlreadyInitialized = 1,
    UnauthorizedAdmin = 2,
    InsufficientBalance = 3,
    InvalidFeeAmount = 4,
}
