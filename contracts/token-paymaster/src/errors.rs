use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PaymasterError {
    AlreadyInitialized = 1,
    InsufficientUserBalance = 2,
    InvalidFeeToken = 3,
    EmergencyPaused = 4,
    UnauthorizedAdmin = 5,
}
