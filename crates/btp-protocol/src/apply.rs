//! Rollover, apply, compaction and mint (SPEC 6.2, 10).

use crate::encode::chain_step;
use crate::types::{AccountState, KeyUpdate, Record, Tx};
use btp_algebra::{Backend, GroupElem};
use std::collections::BTreeMap;

pub fn new_account<B: Backend>(pk: B::G1, c: B::G1, d: B::G1, epoch: u64) -> AccountState<B> {
    let z = B::G1::zero();
    AccountState {
        pk_base: pk,
        c,
        d,
        c_base: c,
        d_base: d,
        dpk: z,
        up_agg: (z, z),
        snap_epoch: epoch,
        credits_this_epoch: 0,
        debited_epoch: 0,
        open_epochs: Vec::new(),
        chain: BTreeMap::new(),
    }
}

pub fn rollover<B: Backend>(acct: &mut AccountState<B>, e: u64) -> Option<KeyUpdate<B>> {
    assert!(acct.snap_epoch <= e, "rollover into the past");
    if acct.snap_epoch == e {
        return None;
    }
    let z = B::G1::zero();
    let log = KeyUpdate {
        epoch: acct.snap_epoch,
        dpk: acct.dpk,
        up_agg: acct.up_agg,
    };
    acct.pk_base = acct.pk_base + acct.dpk;
    acct.dpk = z;
    acct.up_agg = (z, z);
    acct.credits_this_epoch = 0;
    acct.c_base = acct.c;
    acct.d_base = acct.d;
    acct.snap_epoch = e;
    Some(log)
}

pub fn rolled<B: Backend>(acct: &AccountState<B>, e: u64) -> AccountState<B> {
    let mut a = acct.clone();
    rollover(&mut a, e);
    a
}

pub fn append_record<B: Backend>(acct: &mut AccountState<B>, e: u64, rec: &Record<B>) {
    let prev = acct.chain.get(&e).copied().unwrap_or([0u8; 32]);
    acct.chain.insert(e, chain_step(&prev, rec));
    if !acct.open_epochs.contains(&e) {
        acct.open_epochs.push(e);
        acct.open_epochs.sort_unstable();
    }
}

pub fn drop_epochs_before<B: Backend>(acct: &mut AccountState<B>, e: u64) {
    acct.open_epochs.retain(|&x| x >= e);
    acct.chain.retain(|&x, _| x >= e);
}

pub fn credit_record<B: Backend>(tx: &Tx<B>, j: usize, t: B::Fr) -> Record<B> {
    let r = &tx.recv[j];
    Record {
        t,
        c: tx.c,
        r1: tx.r1,
        r2: tx.r2,
        h0: r.h0,
        h1: r.h1,
        h2: r.h2,
        is_update: true,
    }
}

pub fn debit_record<B: Backend>(tx: &Tx<B>, t: B::Fr) -> Record<B> {
    let z = B::G2::zero();
    Record {
        t,
        c: tx.c,
        r1: tx.r1,
        r2: tx.r2,
        h0: z,
        h1: z,
        h2: z,
        is_update: false,
    }
}

pub fn apply_credit<B: Backend>(
    acct: &mut AccountState<B>,
    tx: &Tx<B>,
    j: usize,
    t: B::Fr,
) -> Record<B> {
    assert_eq!(acct.snap_epoch, tx.epoch, "apply_credit before rollover");
    let r = &tx.recv[j];
    acct.c = acct.c + tx.c;
    acct.d = acct.d + r.d + r.e;
    acct.dpk = acct.dpk + r.dpk;
    acct.up_agg = (acct.up_agg.0 + r.up.0, acct.up_agg.1 + r.up.1);
    acct.credits_this_epoch += 1;
    let rec = credit_record(tx, j, t);
    append_record(acct, tx.epoch, &rec);
    rec
}

pub fn apply_debit<B: Backend>(acct: &mut AccountState<B>, tx: &Tx<B>, t: B::Fr) -> Record<B> {
    assert_eq!(acct.snap_epoch, tx.epoch, "apply_debit before rollover");
    acct.c = acct.c + tx.c;
    acct.d = acct.d + tx.n0 - acct.d_base;
    acct.d_base = tx.n0 - tx.d0;
    drop_epochs_before(acct, tx.epoch);
    acct.debited_epoch = tx.epoch;
    let rec = debit_record(tx, t);
    append_record(acct, tx.epoch, &rec);
    rec
}

pub fn compact<B: Backend>(acct: &mut AccountState<B>, e: u64, d_prime: B::G1) {
    assert_eq!(acct.snap_epoch, e, "compact before rollover");
    acct.d = acct.d + d_prime - acct.d_base;
    acct.d_base = d_prime;
    drop_epochs_before(acct, e);
}

pub fn mint<B: Backend>(acct: &mut AccountState<B>, e: u64, amount: u64) {
    assert_eq!(acct.snap_epoch, e, "mint before rollover");
    let m = B::g1().smul(&B::Fr::from(amount));
    acct.d = acct.d + m;
    acct.d_base = acct.d_base + m;
}
