//! EIP-2537 byte encodings (SPEC 3); never use ark_serialize for EVM bytes (compressed, little-endian).

use crate::bls::{G1Projective, G2Projective};
use ark_bls12_381::{g1, g2, Fq, Fq2};
use ark_ec::short_weierstrass::Affine;
use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::{BigInt, BigInteger, PrimeField};

pub fn fr_to_bytes32<F: PrimeField>(x: &F) -> [u8; 32] {
    let be = x.into_bigint().to_bytes_be();
    let mut out = [0u8; 32];
    out[32 - be.len()..].copy_from_slice(&be);
    out
}

pub fn fr_from_be_bytes_mod<F: PrimeField>(bytes: &[u8]) -> F {
    F::from_be_bytes_mod_order(bytes)
}

pub fn fp_to_64(x: &Fq) -> [u8; 64] {
    let be = x.into_bigint().to_bytes_be();
    let mut out = [0u8; 64];
    out[64 - be.len()..].copy_from_slice(&be);
    out
}

pub fn fp_from_64(b: &[u8]) -> Option<Fq> {
    if b.len() != 64 || b[..16].iter().any(|&x| x != 0) {
        return None;
    }
    let mut limbs = [0u64; 6];
    for (i, limb) in limbs.iter_mut().enumerate() {
        let end = 64 - 8 * i;
        *limb = u64::from_be_bytes(b[end - 8..end].try_into().unwrap());
    }
    Fq::from_bigint(BigInt(limbs))
}

pub fn g1_to_eip2537(p: &G1Projective) -> [u8; 128] {
    let a = p.into_affine();
    let mut out = [0u8; 128];
    if a.is_zero() {
        return out;
    }
    out[..64].copy_from_slice(&fp_to_64(&a.x));
    out[64..].copy_from_slice(&fp_to_64(&a.y));
    out
}

pub fn g1_from_eip2537(b: &[u8; 128]) -> Option<G1Projective> {
    if b.iter().all(|&x| x == 0) {
        return Some(G1Projective::default());
    }
    let a = Affine::<g1::Config>::new_unchecked(fp_from_64(&b[..64])?, fp_from_64(&b[64..])?);
    checked(a).map(|a| a.into_group())
}

pub fn g2_to_eip2537(p: &G2Projective) -> [u8; 256] {
    let a = p.into_affine();
    let mut out = [0u8; 256];
    if a.is_zero() {
        return out;
    }
    out[..64].copy_from_slice(&fp_to_64(&a.x.c0));
    out[64..128].copy_from_slice(&fp_to_64(&a.x.c1));
    out[128..192].copy_from_slice(&fp_to_64(&a.y.c0));
    out[192..].copy_from_slice(&fp_to_64(&a.y.c1));
    out
}

pub fn g2_from_eip2537(b: &[u8; 256]) -> Option<G2Projective> {
    if b.iter().all(|&x| x == 0) {
        return Some(G2Projective::default());
    }
    let x = Fq2::new(fp_from_64(&b[..64])?, fp_from_64(&b[64..128])?);
    let y = Fq2::new(fp_from_64(&b[128..192])?, fp_from_64(&b[192..])?);
    checked(Affine::<g2::Config>::new_unchecked(x, y)).map(|a| a.into_group())
}

fn checked<P: ark_ec::short_weierstrass::SWCurveConfig>(a: Affine<P>) -> Option<Affine<P>> {
    (a.is_on_curve() && a.is_in_correct_subgroup_assuming_on_curve()).then_some(a)
}

pub fn to_hex(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}
