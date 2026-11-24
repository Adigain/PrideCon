//! Cross-terms, balance read by baby-step giant-step, and key update (SPEC 8.1, 12).

use crate::transact::side_decrypt;
use crate::types::{Record, Recv};
use crate::ProtocolError;
use ark_ff::{Field, Zero};
use btp_algebra::{Backend, GroupElem};
use std::collections::HashMap;

pub fn cross_term<B: Backend>(i: &Record<B>, m: &Record<B>) -> B::Gt {
    if !m.is_update {
        return B::Gt::zero();
    }
    let h = (m.t - i.t).inverse().expect("tags are distinct");
    B::pair(&i.c, &m.h0) - B::pair(&i.r1, &m.h1).smul(&h) + B::pair(&i.r2, &m.h2).smul(&h)
}

pub fn w_of_epoch<B: Backend>(records: &[Record<B>]) -> B::Gt {
    let mut w = B::Gt::zero();
    for (m, rm) in records.iter().enumerate() {
        for (i, ri) in records.iter().enumerate() {
            if i != m {
                w = w + cross_term(ri, rm);
            }
        }
    }
    w
}

pub fn w_of_epochs<'a, B: Backend>(epochs: impl IntoIterator<Item = &'a [Record<B>]>) -> B::Gt {
    epochs
        .into_iter()
        .fold(B::Gt::zero(), |acc, recs| acc + w_of_epoch(recs))
}

pub fn balance_gt<B: Backend>(c: &B::G1, d: &B::G1, sk: &B::Fr, w: &B::Gt) -> B::Gt {
    B::pair(&(*d - c.smul(sk)), &B::g2()) + *w
}

pub struct BsgsTable<B: Backend> {
    step: u64,
    max: u64,
    baby: HashMap<B::Gt, u64>,
    giant: B::Gt,
}

impl<B: Backend> BsgsTable<B> {
    pub fn new(max: u64) -> Self {
        let step = ((max as f64 + 1.0).sqrt().ceil() as u64).max(1);
        let mut baby = HashMap::with_capacity(step as usize);
        let mut cur = B::Gt::zero();
        for j in 0..step {
            baby.insert(cur, j);
            cur = cur + B::gt();
        }
        BsgsTable {
            step,
            max,
            baby,
            giant: cur,
        }
    }

    pub fn solve(&self, target: &B::Gt) -> Option<u64> {
        let mut cur = *target;
        for i in 0..=self.max / self.step {
            if let Some(j) = self.baby.get(&cur) {
                let b = i * self.step + j;
                return (b <= self.max).then_some(b);
            }
            cur = cur - self.giant;
        }
        None
    }
}

pub fn bsgs<B: Backend>(target: &B::Gt, max: u64) -> Option<u64> {
    BsgsTable::<B>::new(max).solve(target)
}

pub fn key_update<B: Backend>(
    sk: &B::Fr,
    credits: &[Recv<B>],
    logged_dpk: &B::G1,
) -> Result<B::Fr, ProtocolError> {
    let mut delta = B::Fr::zero();
    for rv in credits {
        let d = side_decrypt::<B>(&rv.side, &rv.up.0, sk);
        if B::g1().smul(&d) != rv.dpk {
            return Err(ProtocolError::KeyUpdate);
        }
        delta += d;
    }
    if B::g1().smul(&delta) != *logged_dpk {
        return Err(ProtocolError::KeyUpdate);
    }
    Ok(*sk + delta)
}
