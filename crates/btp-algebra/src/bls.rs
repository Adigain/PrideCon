//! BLS12-381 backend on arkworks 0.5.

use crate::{codec, Backend, GroupElem};
use ark_bls12_381::{g1, g2, Bls12_381, Fr};
use ark_ec::pairing::{Pairing, PairingOutput};
use ark_ec::short_weierstrass::Projective;
use ark_ec::{CurveGroup, PrimeGroup, VariableBaseMSM};
use ark_std::Zero;

// Concrete curve configs: rustc's coherence check cannot tell the crate's Bls12Config aliases apart (E0119).
pub type G1Projective = Projective<g1::Config>;
pub type G2Projective = Projective<g2::Config>;

pub type Gt = PairingOutput<Bls12_381>;

impl GroupElem<Fr> for G1Projective {
    fn zero() -> Self {
        <G1Projective as Zero>::zero()
    }
    fn generator() -> Self {
        <G1Projective as PrimeGroup>::generator()
    }
    fn smul(&self, s: &Fr) -> Self {
        *self * *s
    }
}

impl GroupElem<Fr> for G2Projective {
    fn zero() -> Self {
        <G2Projective as Zero>::zero()
    }
    fn generator() -> Self {
        <G2Projective as PrimeGroup>::generator()
    }
    fn smul(&self, s: &Fr) -> Self {
        *self * *s
    }
}

impl GroupElem<Fr> for Gt {
    fn zero() -> Self {
        <Gt as Zero>::zero()
    }
    fn generator() -> Self {
        Bls12_381::pairing(
            <G1Projective as PrimeGroup>::generator(),
            <G2Projective as PrimeGroup>::generator(),
        )
    }
    fn smul(&self, s: &Fr) -> Self {
        *self * *s
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Bls;

impl Backend for Bls {
    type Fr = Fr;
    type G1 = G1Projective;
    type G2 = G2Projective;
    type Gt = Gt;

    const NAME: &'static str = "bls12-381";

    fn pair(a: &Self::G1, b: &Self::G2) -> Self::Gt {
        Bls12_381::pairing(*a, *b)
    }

    fn enc_g1(p: &Self::G1) -> Vec<u8> {
        codec::g1_to_eip2537(p).to_vec()
    }

    fn enc_g2(p: &Self::G2) -> Vec<u8> {
        codec::g2_to_eip2537(p).to_vec()
    }

    fn multi_pair(pairs: &[(Self::G1, Self::G2)]) -> Self::Gt {
        let a: Vec<G1Projective> = pairs.iter().map(|(a, _)| *a).collect();
        let b: Vec<G2Projective> = pairs.iter().map(|(_, b)| *b).collect();
        Bls12_381::multi_pairing(
            G1Projective::normalize_batch(&a),
            G2Projective::normalize_batch(&b),
        )
    }

    fn msm_g1(points: &[Self::G1], scalars: &[Self::Fr]) -> Self::G1 {
        assert_eq!(points.len(), scalars.len(), "msm_g1: length mismatch");
        let bases = G1Projective::normalize_batch(points);
        G1Projective::msm_unchecked(&bases, scalars)
    }
}
