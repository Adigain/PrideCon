//! Every transaction field matches SPEC 7.1 at exponent level.

mod common;

use ark_ff::{Field, One};
use btp_algebra::bls::Bls;
use btp_algebra::exp::Exp61;
use btp_algebra::{Backend, GroupElem};
use btp_protocol::crs::phi1;
use btp_protocol::encode::tx_tag;
use btp_protocol::transact::{epoch_hints, side_decrypt};
use btp_protocol::*;
use common::TestChain;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

type F = <Exp61 as Backend>::Fr;

fn rng() -> ChaCha20Rng {
    ChaCha20Rng::seed_from_u64(7)
}

#[test]
fn every_field_matches_spec_7_1() {
    let mut r = rng();
    let mut chain = TestChain::<Exp61>::new(4, &mut r);
    chain.mint(1, 1000);
    chain.pay(1, &[2, 3], &[40, 0], &mut r).unwrap();
    chain.next_epoch();

    let e = chain.epoch;
    let payloads = [25u64, 0, 3];
    let ring = [2u32, 3, 4];
    let (tx, sec) = chain.build(1, &ring, &payloads, &mut r).unwrap();
    let td = &chain.td;
    let fr = |v: u64| F::from(v);
    let t = tx_tag(&tx);
    let phi = td.beta * t + td.eta + td.kappa * fr(e);
    let pl0: u64 = payloads.iter().sum();

    let s0 = chain.key(1, e).sk;
    let x0 = apply::rolled(&chain.accounts[0], e).c_base.log;
    let b0 = chain.snapshot_balance(1, e);
    assert_eq!(b0, 960);
    assert_eq!(tx.c.log, sec.r);
    assert_eq!(tx.d0.log, s0 * sec.r - fr(pl0));
    assert_eq!(tx.r1.log, td.beta * sec.r);
    assert_eq!(tx.r2.log, td.beta * phi * sec.r);
    assert_eq!(tx.n0.log, (x0 + sec.r) * s0 + fr(b0 - pl0));

    for (j, &id) in ring.iter().enumerate() {
        let acct = apply::rolled(&chain.accounts[id as usize - 1], e);
        let sj = chain.key(id, e).sk;
        assert_eq!(acct.pk_base.log, sj, "pk_base is g1·s_e after rollover");
        let (rv, d, rho, tau) = (&tx.recv[j], sec.delta[j], sec.rho[j], sec.tau[j]);
        assert_eq!(rv.d.log, sj * sec.r + fr(payloads[j]));
        assert_eq!(rv.up.0.log, rho);
        assert_eq!(rv.up.1.log, sj * rho + d);
        assert_eq!(rv.dpk.log, d);
        assert_eq!(rv.e.log, (acct.c_base.log + sec.r) * d);
        assert_eq!(rv.h0.log, d + td.beta * td.beta * tau);
        assert_eq!(rv.h1.log, phi * tau);
        assert_eq!(rv.h2.log, tau);
        assert_eq!(side_decrypt::<Exp61>(&rv.side, &rv.up.0, &sj), d);
    }
    assert_eq!(phi1(&chain.crs, &t, e).log, phi);
}

#[test]
fn side_value_round_trip_bls() {
    let mut r = rng();
    let mut chain = TestChain::<Bls>::new(3, &mut r);
    chain.mint(1, 10);
    let (tx, sec) = chain.build(1, &[2, 3], &[1, 0], &mut r).unwrap();
    for (j, id) in [2u32, 3].iter().enumerate() {
        let sk = chain.key(*id, 1).sk;
        let rv = &tx.recv[j];
        assert_eq!(side_decrypt::<Bls>(&rv.side, &rv.up.0, &sk), sec.delta[j]);
        assert_eq!(Bls::g1().smul(&sec.delta[j]), rv.dpk);
        let wrong = chain.key(1, 1).sk;
        assert_ne!(
            side_decrypt::<Bls>(&rv.side, &rv.up.0, &wrong),
            sec.delta[j]
        );
    }
}

#[test]
fn overdraft_and_bad_input_are_refused() {
    let mut r = rng();
    let mut chain = TestChain::<Exp61>::new(3, &mut r);
    chain.mint(1, 10);
    assert_eq!(
        chain.build(1, &[2, 3], &[6, 5], &mut r).unwrap_err(),
        ProtocolError::Overdraft
    );
    assert!(
        chain.build(1, &[2, 3], &[10, 0], &mut r).is_ok(),
        "spending everything is fine"
    );
    assert_eq!(
        chain.build(1, &[2, 3], &[1], &mut r).unwrap_err(),
        ProtocolError::BadRing
    );
    assert_eq!(
        chain.build(1, &[], &[], &mut r).unwrap_err(),
        ProtocolError::BadRing
    );
}

#[test]
fn hints_are_inverses() {
    let mut r = rng();
    let mut chain = TestChain::<Exp61>::new(5, &mut r);
    for id in 2..=5 {
        chain.mint(id, 100);
    }
    for id in 2..=5 {
        chain.pay(id, &[1], &[id as u64], &mut r).unwrap();
    }
    let recs = chain.records[0][&1].clone();
    let hints = epoch_hints(&recs);
    assert_eq!(hints.len(), 4);
    for (m, hm) in hints.iter().enumerate() {
        assert_eq!(hm.len(), 3);
        let others: Vec<_> = (0..4).filter(|&i| i != m).collect();
        for (h, &i) in hm.iter().zip(&others) {
            assert_eq!(*h * (recs[m].t - recs[i].t), F::one());
            assert_eq!(*h, (recs[m].t - recs[i].t).inverse().unwrap());
        }
    }
    assert!(epoch_hints(&recs[..1])[0].is_empty());
    let mut debit = recs[1].clone();
    debit.is_update = false;
    let mixed = vec![recs[0].clone(), debit];
    let h = epoch_hints(&mixed);
    assert_eq!(h[0].len(), 1);
    assert!(h[1].is_empty());
}
