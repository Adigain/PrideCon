//! Exponent backend: a group element is stored as its discrete logarithm and a pairing multiplies logs.

use crate::{Backend, GroupElem};
use ark_ff::PrimeField;
use std::marker::PhantomData;
use std::ops::{Add, Neg, Sub};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct G1Tag;
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct G2Tag;
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct GtTag;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExpElem<F, T> {
    pub log: F,
    _tag: PhantomData<T>,
}

impl<F, T> ExpElem<F, T> {
    pub fn from_log(log: F) -> Self {
        Self {
            log,
            _tag: PhantomData,
        }
    }
}

impl<F: PrimeField, T> Add for ExpElem<F, T> {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::from_log(self.log + o.log)
    }
}
impl<F: PrimeField, T> Sub for ExpElem<F, T> {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::from_log(self.log - o.log)
    }
}
impl<F: PrimeField, T> Neg for ExpElem<F, T> {
    type Output = Self;
    fn neg(self) -> Self {
        Self::from_log(-self.log)
    }
}

impl<F, T> GroupElem<F> for ExpElem<F, T>
where
    F: PrimeField,
    T: Copy + Clone + std::fmt::Debug + PartialEq + Eq + std::hash::Hash + Send + Sync + 'static,
{
    fn zero() -> Self {
        Self::from_log(F::zero())
    }
    fn generator() -> Self {
        Self::from_log(F::one())
    }
    fn smul(&self, s: &F) -> Self {
        Self::from_log(self.log * s)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ExpBackend<F>(PhantomData<F>);

impl<F: PrimeField> Backend for ExpBackend<F> {
    type Fr = F;
    type G1 = ExpElem<F, G1Tag>;
    type G2 = ExpElem<F, G2Tag>;
    type Gt = ExpElem<F, GtTag>;

    const NAME: &'static str = "exponent";

    fn pair(a: &Self::G1, b: &Self::G2) -> Self::Gt {
        ExpElem::from_log(a.log * b.log)
    }

    fn enc_g1(p: &Self::G1) -> Vec<u8> {
        crate::codec::fr_to_bytes32(&p.log).to_vec()
    }

    fn enc_g2(p: &Self::G2) -> Vec<u8> {
        crate::codec::fr_to_bytes32(&p.log).to_vec()
    }
}

pub type Exp61 = ExpBackend<crate::f61::F61>;
pub type ExpBls = ExpBackend<ark_bls12_381::Fr>;
