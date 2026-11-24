//! Balance read by baby-step giant-step, and key update.

use ark_ff::UniformRand;
use btp_algebra::bls::Bls;
use btp_algebra::exp::Exp61;
use btp_algebra::{Backend, GroupElem};
use btp_protocol::read::{bsgs, key_update, BsgsTable};
use btp_protocol::transact::{keygen, side_encrypt};
use btp_protocol::*;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

fn bsgs_cases<B: Backend>(max: u64) {
    let table = BsgsTable::<B>::new(max);
    let step = ((max as f64 + 1.0).sqrt().ceil()) as u64;
    let mut r = ChaCha20Rng::seed_from_u64(1);
    let mut vals = vec![0, 1, step - 1, step, step + 1, max - 1, max];
    vals.extend((0..5).map(|_| r.gen_range(0..=max)));
    for b in vals {
        assert_eq!(
            table.solve(&B::gt().smul(&B::Fr::from(b))),
            Some(b),
            "b = {b}"
        );
    }
    assert_eq!(
        table.solve(&B::gt().smul(&B::Fr::from(max + 1))),
        None,
        "above max"
    );
    assert_eq!(
        table.solve(&B::gt().smul(&-B::Fr::from(1u64))),
        None,
        "negative"
    );
}

#[test]
fn bsgs_exp61_full_range() {
    bsgs_cases::<Exp61>((1u64 << 32) - 1);
}

#[test]
fn bsgs_bls() {
    bsgs_cases::<Bls>(1 << 16);
    assert_eq!(
        bsgs::<Bls>(&Bls::gt().smul(&777u64.into()), 1000),
        Some(777)
    );
}

fn key_update_cases<B: Backend>() {
    let mut r = ChaCha20Rng::seed_from_u64(2);
    let k = keygen::<B, _>(&mut r);
    let mut credits = vec![];
    let mut sum = B::Fr::from(0u64);
    for _ in 0..4 {
        let (delta, rho) = (B::Fr::rand(&mut r), B::Fr::rand(&mut r));
        let shared = k.pk.smul(&rho);
        credits.push(Recv::<B> {
            d: B::G1::zero(),
            up: (B::g1().smul(&rho), shared + B::g1().smul(&delta)),
            dpk: B::g1().smul(&delta),
            e: B::G1::zero(),
            side: side_encrypt::<B>(&delta, &shared),
            h0: B::G2::zero(),
            h1: B::G2::zero(),
            h2: B::G2::zero(),
        });
        sum += delta;
    }
    let logged = B::g1().smul(&sum);
    assert_eq!(key_update(&k.sk, &credits, &logged), Ok(k.sk + sum));
    assert_eq!(
        key_update(&k.sk, &credits, &(logged + B::g1())),
        Err(ProtocolError::KeyUpdate)
    );
    let mut forged = credits.clone();
    forged[2].side[31] ^= 1;
    assert_eq!(
        key_update(&k.sk, &forged, &logged),
        Err(ProtocolError::KeyUpdate)
    );
    assert_eq!(
        key_update::<B>(&k.sk, &[], &B::G1::zero()),
        Ok(k.sk),
        "no credits, no change"
    );
}

#[test]
fn key_update_exp61() {
    key_update_cases::<Exp61>();
}

#[test]
fn key_update_bls() {
    key_update_cases::<Bls>();
}
