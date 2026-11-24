//! Concurrent PriDeFUL CT with tagged cross-term handles (SPEC 5-12).

pub mod abi;
pub mod apply;
pub mod crs;
pub mod driver;
pub mod encode;
pub mod hash;
pub mod ledger;
pub mod params;
pub mod read;
pub mod transact;
pub mod types;
pub mod verify;
pub mod wallet;
pub mod workload;

pub use params::Params;
pub use types::*;

#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("wrong epoch")]
    WrongEpoch,
    #[error("tag already used")]
    DuplicateTag,
    #[error("sender already debited this epoch")]
    AlreadyDebited,
    #[error("receiver over the per-epoch credit cap")]
    CreditCap,
    #[error("ring is malformed")]
    BadRing,
    #[error("batched handle check (V1-V4) failed")]
    HandleCheck,
    #[error("spend check (V5) failed")]
    SpendCheck,
    #[error("records do not match the stored hash chain")]
    RecordChain,
    #[error("bad inverse hint")]
    BadHint,
    #[error("insufficient balance")]
    Overdraft,
    #[error("unknown account")]
    UnknownAccount,
    #[error("key update does not match the logged dPK")]
    KeyUpdate,
    #[error("registration needs a non-zero pk and C")]
    BadRegistration,
}
