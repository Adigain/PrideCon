//! Verification (SPEC 9, 10.1); rollovers are done on copies, so a rejection changes nothing.

use crate::apply::rolled;
use crate::crs::phi1;
use crate::encode::{batch_weight, chain_of, tx_tag, txhash};
use crate::types::{AccountState, Compaction, Crs, OpCounters, SpendWitness, Tx};
use crate::ProtocolError;
use ark_ff::One;
use btp_algebra::codec::fr_to_bytes32;
use btp_algebra::{Backend, GroupElem};
use std::collections::HashSet;

pub const MAX_RING: usize = 32;

pub type Pairs<B> = Vec<(<B as Backend>::G1, <B as Backend>::G2)>;

pub trait ChainView<B: Backend> {
    fn account(&self, id: u32) -> Option<&AccountState<B>>;
    fn tag_used(&self, tag: &[u8; 32]) -> bool;
    fn current_epoch(&self) -> u64;
    fn k_max(&self) -> u32;
}

pub enum SpendEval<'a, B: Backend> {
    Records,
    Native(&'a B::Gt),
}

pub fn check_basic<B: Backend, V: ChainView<B>>(
    view: &V,
    tx: &Tx<B>,
) -> Result<B::Fr, ProtocolError> {
    if tx.epoch != view.current_epoch() {
        return Err(ProtocolError::WrongEpoch);
    }
    let n = tx.ring.len();
    if n == 0 || n > MAX_RING || tx.recv.len() != n {
        return Err(ProtocolError::BadRing);
    }
    let distinct: HashSet<u32> = tx.ring.iter().copied().collect();
    if distinct.len() != n || distinct.contains(&tx.sender) {
        return Err(ProtocolError::BadRing);
    }
    let sender = view.account(tx.sender).ok_or(ProtocolError::BadRing)?;
    let ring: Vec<&AccountState<B>> = tx
        .ring
        .iter()
        .map(|id| view.account(*id).ok_or(ProtocolError::BadRing))
        .collect::<Result<_, _>>()?;

    let t = tx_tag(tx);
    if view.tag_used(&fr_to_bytes32(&t)) {
        return Err(ProtocolError::DuplicateTag);
    }

    if rolled(sender, tx.epoch).debited_epoch == tx.epoch {
        return Err(ProtocolError::AlreadyDebited);
    }
    if ring
        .iter()
        .any(|a| rolled(a, tx.epoch).credits_this_epoch >= view.k_max())
    {
        return Err(ProtocolError::CreditCap);
    }
    Ok(t)
}

pub fn batched_pairs<B: Backend>(crs: &Crs<B>, tx: &Tx<B>, t: &B::Fr) -> Pairs<B> {
    let h = txhash(tx);
    let w = |i: usize| batch_weight::<B>(&h, i as u32);
    let (w1, w2) = (w(0), w(1));
    let e = B::Fr::from(tx.epoch);
    let phi = phi1(crs, t, tx.epoch);

    let mut pts = vec![tx.r1, tx.r2];
    let mut sc = vec![w1, w2];
    for (j, r) in tx.recv.iter().enumerate() {
        pts.push(r.dpk);
        sc.push(-w(2 + 2 * j));
    }
    let mut pairs = vec![
        (B::msm_g1(&pts, &sc), crs.g2),
        (B::msm_g1(&[tx.c, tx.r1], &[-w1, -(w2 * t)]), crs.beta_2),
        (tx.r1.smul(&(-w2)), crs.eta_2),
        (tx.r1.smul(&(-(w2 * e))), crs.kappa_2),
    ];
    for (j, r) in tx.recv.iter().enumerate() {
        let (w3, w4) = (w(2 + 2 * j), w(3 + 2 * j));
        pairs.push((crs.g1.smul(&w3), r.h0));
        pairs.push((crs.g1.smul(&w4), r.h1));
        pairs.push((B::msm_g1(&[crs.beta2_1, phi], &[-w3, -w4]), r.h2));
    }
    pairs
}

pub fn verify_batched<B: Backend>(
    crs: &Crs<B>,
    tx: &Tx<B>,
    t: &B::Fr,
    counters: &mut OpCounters,
) -> Result<(), ProtocolError> {
    let pairs = batched_pairs(crs, tx, t);
    counters.pairings += pairs.len() as u64;
    counters.msm_terms += (8 + 5 * tx.recv.len()) as u64;
    if B::multi_pair(&pairs).is_zero() {
        Ok(())
    } else {
        Err(ProtocolError::HandleCheck)
    }
}

/// The aggregates S_m, A_m, B_m are computed here and never taken from the caller (SPEC 9.5).
pub fn deferred_w_pairs<B: Backend>(
    acct: &AccountState<B>,
    e: u64,
    witness: &SpendWitness<B>,
    counters: &mut OpCounters,
) -> Result<Pairs<B>, ProtocolError> {
    let mut pairs = Vec::new();
    for &ep in acct.open_epochs.iter().filter(|&&ep| ep < e) {
        let ew = witness
            .epochs
            .iter()
            .find(|w| w.epoch == ep)
            .ok_or(ProtocolError::RecordChain)?;
        if Some(&chain_of(&ew.records)) != acct.chain.get(&ep) {
            return Err(ProtocolError::RecordChain);
        }
        let recs = &ew.records;
        if ew.hints.len() != recs.len() {
            return Err(ProtocolError::BadHint);
        }
        for (m, rm) in recs.iter().enumerate() {
            if !rm.is_update || recs.len() < 2 {
                continue;
            }
            let hints = &ew.hints[m];
            if hints.len() != recs.len() - 1 {
                return Err(ProtocolError::BadHint);
            }
            let others: Vec<_> = recs
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != m)
                .map(|(_, r)| r)
                .collect();
            for (h, ri) in hints.iter().zip(&others) {
                if *h * (rm.t - ri.t) != B::Fr::one() {
                    return Err(ProtocolError::BadHint);
                }
            }
            let s_m = others.iter().fold(B::G1::zero(), |acc, r| acc + r.c);
            let r1s: Vec<B::G1> = others.iter().map(|r| r.r1).collect();
            let r2s: Vec<B::G1> = others.iter().map(|r| r.r2).collect();
            let neg: Vec<B::Fr> = hints.iter().map(|h| -*h).collect();
            pairs.push((s_m, rm.h0));
            pairs.push((B::msm_g1(&r1s, &neg), rm.h1));
            pairs.push((B::msm_g1(&r2s, hints), rm.h2));
            counters.g1_adds += others.len() as u64;
            counters.msm_terms += 2 * others.len() as u64;
        }
    }
    Ok(pairs)
}

fn normalisation_check<B: Backend>(
    crs: &Crs<B>,
    acct: &AccountState<B>,
    e: u64,
    lhs: B::G1,
    witness: &SpendWitness<B>,
    eval: &SpendEval<B>,
    counters: &mut OpCounters,
) -> Result<(), ProtocolError> {
    let ok = match eval {
        SpendEval::Records => {
            let mut pairs = deferred_w_pairs(acct, e, witness, counters)?;
            pairs.push((lhs, crs.g2));
            counters.pairings += pairs.len() as u64;
            B::multi_pair(&pairs).is_zero()
        }
        SpendEval::Native(w_base) => (B::pair(&lhs, &crs.g2) + **w_base).is_zero(),
    };
    if ok {
        Ok(())
    } else {
        Err(ProtocolError::SpendCheck)
    }
}

pub fn spend_check<B: Backend>(
    crs: &Crs<B>,
    sender: &AccountState<B>,
    tx: &Tx<B>,
    eval: &SpendEval<B>,
    counters: &mut OpCounters,
) -> Result<(), ProtocolError> {
    let lhs = sender.d_base + tx.d0 - tx.n0;
    normalisation_check(crs, sender, tx.epoch, lhs, &tx.spend, eval, counters)
}

pub fn verify_tx<B: Backend, V: ChainView<B>>(
    crs: &Crs<B>,
    view: &V,
    tx: &Tx<B>,
    eval: &SpendEval<B>,
    counters: &mut OpCounters,
) -> Result<B::Fr, ProtocolError> {
    let t = check_basic(view, tx)?;
    verify_batched(crs, tx, &t, counters)?;
    let sender = rolled(view.account(tx.sender).expect("checked in 9.1"), tx.epoch);
    spend_check(crs, &sender, tx, eval, counters)?;
    Ok(t)
}

pub fn verify_compaction<B: Backend, V: ChainView<B>>(
    crs: &Crs<B>,
    view: &V,
    comp: &Compaction<B>,
    eval: &SpendEval<B>,
    counters: &mut OpCounters,
) -> Result<(), ProtocolError> {
    if comp.epoch != view.current_epoch() {
        return Err(ProtocolError::WrongEpoch);
    }
    let acct = rolled(
        view.account(comp.account)
            .ok_or(ProtocolError::UnknownAccount)?,
        comp.epoch,
    );
    let lhs = acct.d_base - comp.d_prime;
    normalisation_check(crs, &acct, comp.epoch, lhs, &comp.witness, eval, counters)
}
