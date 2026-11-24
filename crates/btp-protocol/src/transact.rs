//! Building a transaction and a compaction message (SPEC 7, 10.1).

use crate::apply::rolled;
use crate::crs::{bphi1, phi2};
use crate::encode::{side_mask, tx_tag};
use crate::types::{
    AccountState, Compaction, Crs, EpochWitness, Keypair, Record, Recv, SpendWitness, Tx, TxSecrets,
};
use crate::ProtocolError;
use ark_ff::{Field, UniformRand};
use btp_algebra::codec::{fr_from_be_bytes_mod, fr_to_bytes32};
use btp_algebra::{Backend, GroupElem};
use rand::RngCore;
use std::collections::BTreeMap;

pub fn keygen<B: Backend, R: RngCore>(rng: &mut R) -> Keypair<B> {
    let sk = B::Fr::rand(rng);
    Keypair {
        sk,
        pk: B::g1().smul(&sk),
    }
}

pub fn fresh_account<B: Backend, R: RngCore>(pk: &B::G1, rng: &mut R) -> (B::G1, B::G1) {
    let x = B::Fr::rand(rng);
    (B::g1().smul(&x), pk.smul(&x))
}

pub fn side_encrypt<B: Backend>(delta: &B::Fr, shared: &B::G1) -> [u8; 32] {
    xor32(&fr_to_bytes32(delta), &side_mask::<B>(shared))
}

pub fn side_decrypt<B: Backend>(side: &[u8; 32], u1: &B::G1, sk: &B::Fr) -> B::Fr {
    let bytes = xor32(side, &side_mask::<B>(&u1.smul(sk)));
    fr_from_be_bytes_mod(&bytes)
}

fn xor32(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    std::array::from_fn(|i| a[i] ^ b[i])
}

pub fn epoch_hints<B: Backend>(records: &[Record<B>]) -> Vec<Vec<B::Fr>> {
    records
        .iter()
        .enumerate()
        .map(|(m, rm)| {
            if !rm.is_update || records.len() < 2 {
                return Vec::new();
            }
            records
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != m)
                .map(|(_, ri)| (rm.t - ri.t).inverse().expect("tags are distinct"))
                .collect()
        })
        .collect()
}

pub fn build_witness<B: Backend>(
    acct: &AccountState<B>,
    e: u64,
    records: &BTreeMap<u64, Vec<Record<B>>>,
) -> SpendWitness<B> {
    let epochs = acct
        .open_epochs
        .iter()
        .filter(|&&ep| ep < e)
        .map(|&ep| {
            let recs = records.get(&ep).cloned().unwrap_or_default();
            EpochWitness {
                epoch: ep,
                hints: epoch_hints(&recs),
                records: recs,
            }
        })
        .collect();
    SpendWitness { epochs }
}

pub struct SenderView<'a, B: Backend> {
    pub crs: &'a Crs<B>,
    pub epoch: u64,
    pub sender_id: u32,
    pub sender_keys: &'a Keypair<B>,
    pub sender_balance: u64,
    pub sender_state: &'a AccountState<B>,
    pub sender_records: &'a BTreeMap<u64, Vec<Record<B>>>,
    pub ring: Vec<(u32, &'a AccountState<B>)>,
    pub payloads: Vec<u64>,
}

pub fn transact<B: Backend, R: RngCore>(
    view: &SenderView<B>,
    rng: &mut R,
) -> Result<(Tx<B>, TxSecrets<B>), ProtocolError> {
    let (crs, e, n) = (view.crs, view.epoch, view.ring.len());
    if view.payloads.len() != n || n == 0 {
        return Err(ProtocolError::BadRing);
    }
    let fr = |v: u64| B::Fr::from(v);
    let g1 = B::g1();

    let sender = rolled(view.sender_state, e);
    let ring: Vec<AccountState<B>> = view.ring.iter().map(|(_, a)| rolled(a, e)).collect();
    let pl0: u64 = view.payloads.iter().sum();
    if pl0 > view.sender_balance {
        return Err(ProtocolError::Overdraft);
    }

    let r = B::Fr::rand(rng);
    let c = g1.smul(&r);
    let d0 = sender.pk_base.smul(&r) - g1.smul(&fr(pl0));

    let mut secrets = TxSecrets {
        r,
        delta: vec![],
        rho: vec![],
        tau: vec![],
    };
    let mut recv = Vec::with_capacity(n);
    for (acct, &pl) in ring.iter().zip(&view.payloads) {
        let (delta, rho, tau) = (B::Fr::rand(rng), B::Fr::rand(rng), B::Fr::rand(rng));
        let shared = acct.pk_base.smul(&rho);
        recv.push(Recv {
            d: acct.pk_base.smul(&r) + g1.smul(&fr(pl)),
            up: (g1.smul(&rho), shared + g1.smul(&delta)),
            dpk: g1.smul(&delta),
            e: (acct.c_base + c).smul(&delta),
            side: side_encrypt::<B>(&delta, &shared),
            h0: B::G2::zero(),
            h1: B::G2::zero(),
            h2: B::G2::zero(),
        });
        secrets.delta.push(delta);
        secrets.rho.push(rho);
        secrets.tau.push(tau);
    }

    let mut tx = Tx {
        epoch: e,
        sender: view.sender_id,
        ring: view.ring.iter().map(|(id, _)| *id).collect(),
        c,
        d0,
        n0: B::G1::zero(),
        r1: B::G1::zero(),
        r2: B::G1::zero(),
        recv,
        spend: SpendWitness { epochs: vec![] },
    };

    let t = tx_tag(&tx);

    tx.r1 = crs.beta1.smul(&r);
    tx.r2 = bphi1(crs, &t, e).smul(&r);

    let p2 = phi2(crs, &t, e);
    for (rv, (delta, tau)) in tx
        .recv
        .iter_mut()
        .zip(secrets.delta.iter().zip(&secrets.tau))
    {
        rv.h0 = crs.g2.smul(delta) + crs.beta2_2.smul(tau);
        rv.h1 = p2.smul(tau);
        rv.h2 = crs.g2.smul(tau);
    }

    let s0 = view.sender_keys.sk;
    tx.n0 = (sender.c_base + c).smul(&s0) + g1.smul(&fr(view.sender_balance - pl0));

    tx.spend = build_witness(&sender, e, view.sender_records);
    Ok((tx, secrets))
}

pub fn build_compaction<B: Backend>(
    account: u32,
    e: u64,
    keys: &Keypair<B>,
    snapshot_balance: u64,
    state: &AccountState<B>,
    records: &BTreeMap<u64, Vec<Record<B>>>,
) -> Compaction<B> {
    let acct = rolled(state, e);
    Compaction {
        account,
        epoch: e,
        d_prime: acct.c_base.smul(&keys.sk) + B::g1().smul(&B::Fr::from(snapshot_balance)),
        witness: build_witness(&acct, e, records),
    }
}
