//! Verification: tampering, basic checks, spend checks and compaction.

mod common;

use btp_algebra::bls::Bls;
use btp_algebra::exp::Exp61;
use btp_algebra::{Backend, GroupElem};
use btp_protocol::encode::tx_tag;
use btp_protocol::read::w_of_epoch;
use btp_protocol::verify::{batched_pairs, deferred_w_pairs, verify_batched, MAX_RING};
use btp_protocol::*;
use common::TestChain;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

fn rng(seed: u64) -> ChaCha20Rng {
    ChaCha20Rng::seed_from_u64(seed)
}

fn funded<B: Backend>(n: usize, bal: u64, r: &mut ChaCha20Rng) -> TestChain<B> {
    let mut c = TestChain::<B>::new(n, r);
    for id in 1..=n as u32 {
        c.mint(id, bal);
    }
    c
}

fn tampering_is_caught<B: Backend>() {
    let mut r = rng(1);
    let chain = funded::<B>(4, 100, &mut r);
    let (tx, _) = chain.build(1, &[2, 3, 4], &[10, 0, 5], &mut r).unwrap();
    let t = tx_tag(&tx);
    let mut cnt = OpCounters::default();
    assert_eq!(verify_batched(&chain.crs, &tx, &t, &mut cnt), Ok(()));
    assert_eq!(
        batched_pairs(&chain.crs, &tx, &t).len(),
        4 + 3 * 3,
        "4 + 3n pairs"
    );

    let (g1, g2) = (B::g1(), B::g2());
    let mut cases: Vec<(&str, Tx<B>)> = Vec::new();
    let mut add = |name, f: &dyn Fn(&mut Tx<B>)| {
        let mut x = tx.clone();
        f(&mut x);
        cases.push((name, x));
    };
    add("C", &|x| x.c = x.c + g1);
    add("R1", &|x| x.r1 = x.r1 + g1);
    add("R2", &|x| x.r2 = x.r2 + g1);
    for j in 0..3 {
        add("dpk", &|x| x.recv[j].dpk = x.recv[j].dpk + g1);
        add("H0", &|x| x.recv[j].h0 = x.recv[j].h0 + g2);
        add("H1", &|x| x.recv[j].h1 = x.recv[j].h1 + g2);
        add("H2", &|x| x.recv[j].h2 = x.recv[j].h2 + g2);
    }
    add("swap", &|x| {
        let h = x.recv[0].h0;
        x.recv[0].h0 = x.recv[1].h0;
        x.recv[1].h0 = h;
    });
    for (name, x) in &cases {
        let res = verify_batched(&chain.crs, x, &tx_tag(x), &mut cnt);
        assert_eq!(
            res,
            Err(ProtocolError::HandleCheck),
            "tampered {name} accepted"
        );
    }
    let other_t = t + B::Fr::from(1u64);
    assert_eq!(
        verify_batched(&chain.crs, &tx, &other_t, &mut cnt),
        Err(ProtocolError::HandleCheck)
    );
    let mut x = tx.clone();
    x.epoch += 1;
    assert_eq!(
        verify_batched(&chain.crs, &x, &t, &mut cnt),
        Err(ProtocolError::HandleCheck)
    );
}

#[test]
fn tampering_is_caught_exp61() {
    tampering_is_caught::<Exp61>();
}

#[test]
fn tampering_is_caught_bls() {
    tampering_is_caught::<Bls>();
}

#[test]
fn batched_verify_random_port() {
    let mut r = rng(2026);
    let chain = funded::<Exp61>(6, 1_000_000, &mut r);
    let (mut honest_ok, mut bad_ok) = (0, 0);
    let mut cnt = OpCounters::default();
    for k in 0..150 {
        let n = [1usize, 2, 5][k % 3];
        let ring: Vec<u32> = (2..2 + n as u32).collect();
        let pls: Vec<u64> = (0..n).map(|_| r.gen_range(0..100)).collect();
        let (tx, _) = chain.build(1, &ring, &pls, &mut r).unwrap();
        honest_ok += verify_batched(&chain.crs, &tx, &tx_tag(&tx), &mut cnt).is_ok() as u32;
        let mut x = tx.clone();
        let j = r.gen_range(0..n);
        let one = <Exp61 as Backend>::g2();
        match r.gen_range(0..3) {
            0 => x.recv[j].h0 = x.recv[j].h0 + one,
            1 => x.recv[j].h1 = x.recv[j].h1 + one,
            _ => x.recv[j].h2 = x.recv[j].h2 + one,
        }
        bad_ok += verify_batched(&chain.crs, &x, &tx_tag(&x), &mut cnt).is_ok() as u32;
    }
    assert_eq!((honest_ok, bad_ok), (150, 0));
}

#[test]
fn basic_checks() {
    let mut r = rng(3);
    let mut chain = funded::<Exp61>(12, 100, &mut r);

    let (tx, _) = chain.build(1, &[2, 3], &[1, 0], &mut r).unwrap();
    let mut x = tx.clone();
    x.epoch = 2;
    assert_eq!(chain.submit(&x), Err(ProtocolError::WrongEpoch));
    let mut x = tx.clone();
    x.ring[1] = 2;
    assert_eq!(chain.submit(&x), Err(ProtocolError::BadRing));
    let mut x = tx.clone();
    x.ring[1] = 1;
    assert_eq!(chain.submit(&x), Err(ProtocolError::BadRing));
    let mut x = tx.clone();
    x.ring[1] = 99;
    assert_eq!(chain.submit(&x), Err(ProtocolError::BadRing));
    let mut x = tx.clone();
    x.recv.pop();
    assert_eq!(chain.submit(&x), Err(ProtocolError::BadRing));
    let mut x = tx.clone();
    x.ring = (2..=MAX_RING as u32 + 2).collect();
    assert_eq!(chain.submit(&x), Err(ProtocolError::BadRing));

    chain.submit(&tx).unwrap();
    assert_eq!(chain.submit(&tx), Err(ProtocolError::DuplicateTag));
    let (tx2, _) = chain.build(1, &[4], &[1], &mut r).unwrap();
    assert_eq!(chain.submit(&tx2), Err(ProtocolError::AlreadyDebited));

    for s in 3..3 + chain.k_max {
        chain.pay(s, &[12], &[1], &mut r).unwrap();
    }
    let (tx3, _) = chain.build(11, &[12], &[1], &mut r).unwrap();
    assert_eq!(chain.submit(&tx3), Err(ProtocolError::CreditCap));
    chain.next_epoch();
    chain.pay(11, &[12], &[1], &mut r).unwrap();
}

fn history<B: Backend>(k: u32, r: &mut ChaCha20Rng) -> TestChain<B> {
    let mut chain = funded::<B>(k as usize + 3, 1000, r);
    let n = chain.accounts.len() as u32;
    for s in 0..k {
        chain.pay(2 + s, &[1, n], &[10 + s as u64, 0], r).unwrap();
    }
    chain.pay(1, &[n - 1], &[100], r).unwrap();
    chain.next_epoch();
    chain
}

#[test]
fn spend_with_history_is_accepted_exp61() {
    let mut r = rng(4);
    for k in [0u32, 1, 2, 5] {
        let mut chain = history::<Exp61>(k, &mut r);
        let n = chain.accounts.len() as u32;
        let expect = 1000 + (0..k as u64).map(|s| 10 + s).sum::<u64>() - 100;
        assert_eq!(chain.snapshot_balance(1, 2), expect);
        chain
            .pay(1, &[n], &[expect], &mut r)
            .expect("spend everything");
        assert!(
            chain.accounts[0].open_epochs.iter().all(|&e| e >= 2),
            "epoch 1 normalised"
        );
    }
}

#[test]
fn spend_with_history_is_accepted_bls() {
    let mut r = rng(5);
    let mut chain = history::<Bls>(2, &mut r);
    let n = chain.accounts.len() as u32;
    chain.pay(1, &[n], &[5], &mut r).unwrap();
}

#[test]
fn spend_check_rejects_bad_witness_and_overdraft() {
    let mut r = rng(6);
    let mut chain = history::<Exp61>(3, &mut r);
    let n = chain.accounts.len() as u32;
    let (tx, _) = chain.build(1, &[n], &[1], &mut r).unwrap();
    let ep = &tx.spend.epochs[0];
    assert_eq!(ep.epoch, 1);
    assert_eq!(ep.records.len(), 4, "3 credits + 1 debit record");

    let mut x = tx.clone();
    x.spend.epochs[0].hints[0][1] += <Exp61 as Backend>::Fr::from(1u64);
    assert_eq!(chain.submit(&x), Err(ProtocolError::BadHint));
    let mut x = tx.clone();
    x.spend.epochs[0].hints[0].pop();
    assert_eq!(chain.submit(&x), Err(ProtocolError::BadHint));

    let mut x = tx.clone();
    x.spend.epochs[0].records.pop();
    assert_eq!(chain.submit(&x), Err(ProtocolError::RecordChain));
    let mut x = tx.clone();
    x.spend.epochs[0].records[1].c = x.spend.epochs[0].records[1].c + Exp61::g1();
    assert_eq!(chain.submit(&x), Err(ProtocolError::RecordChain));
    let mut x = tx.clone();
    x.spend.epochs.clear();
    assert_eq!(chain.submit(&x), Err(ProtocolError::RecordChain));
    let mut x = tx.clone();
    x.spend.epochs[0].records.swap(0, 1);
    assert_eq!(chain.submit(&x), Err(ProtocolError::RecordChain));

    let real = chain.snapshot_balance(1, 2);
    let (x, _) = chain
        .build_with_balance(1, &[n], &[1], real + 1, &mut r)
        .unwrap();
    assert_eq!(chain.submit(&x), Err(ProtocolError::SpendCheck));

    chain.submit(&tx).unwrap();
}

fn grouped_equals_naive<B: Backend>(ks: &[u32]) {
    let mut r = rng(8);
    for &k in ks {
        let chain = history::<B>(k, &mut r);
        let acct = apply::rolled(&chain.accounts[0], 2);
        let records = chain.records[0][&1].clone();
        let witness = transact::build_witness(&acct, 2, &chain.records[0]);
        let mut cnt = OpCounters::default();
        let pairs = deferred_w_pairs(&acct, 2, &witness, &mut cnt).unwrap();
        assert_eq!(pairs.len(), 3 * k as usize * (records.len() >= 2) as usize);
        assert_eq!(B::multi_pair(&pairs), w_of_epoch(&records), "k = {k}");
    }
}

#[test]
fn grouped_equals_naive_exp61() {
    grouped_equals_naive::<Exp61>(&[0, 1, 2, 3, 6]);
}

#[test]
fn grouped_equals_naive_bls() {
    grouped_equals_naive::<Bls>(&[2]);
}

#[test]
fn cross_term_is_r_times_delta() {
    let mut r = rng(9);
    let mut chain = funded::<Exp61>(4, 100, &mut r);
    let (a, sa) = chain.build(2, &[1], &[1], &mut r).unwrap();
    let (b, sb) = chain.build(3, &[1], &[1], &mut r).unwrap();
    let ta = chain.submit(&a).unwrap();
    let tb = chain.submit(&b).unwrap();
    let ra = apply::credit_record(&a, 0, ta);
    let rb = apply::credit_record(&b, 0, tb);
    use btp_protocol::read::cross_term;
    assert_eq!(cross_term(&ra, &rb).log, sa.r * sb.delta[0]);
    assert_eq!(cross_term(&rb, &ra).log, sb.r * sa.delta[0]);
    assert!(cross_term(&ra, &apply::debit_record(&b, tb)).is_zero());
}

#[test]
fn compaction_is_checked() {
    let mut r = rng(10);
    let mut chain = history::<Exp61>(3, &mut r);
    let comp = chain.build_compaction(1);
    let mut bad = comp.clone();
    bad.d_prime = bad.d_prime + Exp61::g1();
    assert_eq!(
        chain.submit_compaction(&bad),
        Err(ProtocolError::SpendCheck)
    );
    let mut bad = comp.clone();
    bad.witness.epochs[0].records.pop();
    assert_eq!(
        chain.submit_compaction(&bad),
        Err(ProtocolError::RecordChain)
    );
    let mut bad = comp.clone();
    bad.epoch = 3;
    assert_eq!(
        chain.submit_compaction(&bad),
        Err(ProtocolError::WrongEpoch)
    );
    chain.submit_compaction(&comp).unwrap();
    assert!(chain.accounts[0].open_epochs.is_empty());
    let n = chain.accounts.len() as u32;
    let bal = chain.snapshot_balance(1, 2);
    chain.pay(1, &[n], &[bal], &mut r).unwrap();
}

#[test]
fn debit_and_compaction_commute() {
    let mut finals = Vec::new();
    for debit_first in [true, false] {
        let mut r = rng(11);
        let mut chain = history::<Exp61>(3, &mut r);
        let n = chain.accounts.len() as u32;
        let comp = chain.build_compaction(1);
        let (tx, _) = chain.build(1, &[n], &[7], &mut r).unwrap();
        if debit_first {
            chain.submit(&tx).unwrap();
            chain
                .submit_compaction(&comp)
                .expect("stale epoch entries are ignored");
        } else {
            chain.submit_compaction(&comp).unwrap();
            chain.submit(&tx).expect("stale epoch entries are ignored");
        }
        finals.push(chain.accounts[0].clone());
    }
    assert_eq!(finals[0], finals[1]);
}
