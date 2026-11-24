//! keccak256 with domain tags (SPEC 4).

use ark_ff::PrimeField;
use sha3::{Digest, Keccak256};

pub const TAG_TX: &[u8] = b"BTP/tag/v1";
pub const TAG_WEIGHT: &[u8] = b"BTP/w/v1";
pub const TAG_RECORD: &[u8] = b"BTP/rec/v1";
pub const TAG_SIDE: &[u8] = b"BTP/side/v1";
pub const TAG_FS: &[u8] = b"BTP/fs/v1";
pub const TAG_CRS: &[u8] = b"BTP/crs/v1";

pub fn keccak(tag: &[u8], data: &[u8]) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(tag);
    h.update(data);
    h.finalize().into()
}

pub fn hash_to_fr<F: PrimeField>(tag: &[u8], data: &[u8]) -> F {
    F::from_be_bytes_mod_order(&keccak(tag, data))
}
