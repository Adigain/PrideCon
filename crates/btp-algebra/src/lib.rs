//! Algebra layer: one interface for G1, G2, GT and the pairing, with exponent and BLS12-381 backends.

use ark_ff::PrimeField;
use std::fmt::Debug;
use std::ops::{Add, Neg, Sub};

pub mod bls;
pub mod codec;
pub mod exp;
pub mod f61;

pub trait GroupElem<F: PrimeField>:
    Copy
    + Clone
    + Debug
    + PartialEq
    + Eq
    + std::hash::Hash
    + Send
    + Sync
    + 'static
    + Add<Output = Self>
    + Sub<Output = Self>
    + Neg<Output = Self>
{
    fn zero() -> Self;
    fn generator() -> Self;
    fn smul(&self, s: &F) -> Self;
    fn is_zero(&self) -> bool {
        *self == Self::zero()
    }
}

pub trait Backend: Copy + Clone + Debug + PartialEq + Eq + Send + Sync + 'static {
    type Fr: PrimeField;
    type G1: GroupElem<Self::Fr>;
    type G2: GroupElem<Self::Fr>;
    type Gt: GroupElem<Self::Fr>;

    const NAME: &'static str;

    fn pair(a: &Self::G1, b: &Self::G2) -> Self::Gt;

    fn enc_g1(p: &Self::G1) -> Vec<u8>;

    fn enc_g2(p: &Self::G2) -> Vec<u8>;

    fn multi_pair(pairs: &[(Self::G1, Self::G2)]) -> Self::Gt {
        pairs
            .iter()
            .fold(Self::Gt::zero(), |acc, (a, b)| acc + Self::pair(a, b))
    }

    fn msm_g1(points: &[Self::G1], scalars: &[Self::Fr]) -> Self::G1 {
        assert_eq!(points.len(), scalars.len(), "msm_g1: length mismatch");
        points
            .iter()
            .zip(scalars)
            .fold(Self::G1::zero(), |acc, (p, s)| acc + p.smul(s))
    }

    fn g1() -> Self::G1 {
        Self::G1::generator()
    }
    fn g2() -> Self::G2 {
        Self::G2::generator()
    }
    fn gt() -> Self::Gt {
        Self::Gt::generator()
    }
}
