use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ForwarderError {
    AlreadyInitialized = 1,
    ExpiredDeadline = 2,
    InvalidNonceSequence = 3,
    BatchLengthMismatch = 4,
    UnauthorizedSigner = 5,
}
