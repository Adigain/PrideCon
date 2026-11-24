//! CRS checks and encodings.

mod common;

use btp_algebra::bls::Bls;
use btp_algebra::exp::Exp61;
use btp_algebra::{Backend, GroupElem};
use btp_protocol::crs::*;
use btp_protocol::encode::*;
use btp_protocol::*;
use common::TestChain;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

fn rng() -> ChaCha20Rng {
    ChaCha20Rng::seed_from_u64(31)
}

#[test]
fn setup_is_deterministic_and_seed_dependent() {
    let (a, ta) = setup_from_seed::<Exp61>(b"seed");
    let (b, _) = setup_from_seed::<Exp61>(b"seed");
    let (c, tc) = setup_from_seed::<Exp61>(b"other");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_ne!(ta.beta, tc.beta);
    assert_eq!(a.beta1.log, ta.beta);
    assert_eq!(a.beta2_1.log, ta.beta * ta.beta);
    assert_eq!(a.etabeta1.log, ta.eta * ta.beta);
    assert_eq!(a.kappabeta1.log, ta.kappa * ta.beta);
    assert_eq!(a.kappa_2.log, ta.kappa);
}

type G1Field<B> = fn(&mut Crs<B>) -> &mut <B as Backend>::G1;
type G2Field<B> = fn(&mut Crs<B>) -> &mut <B as Backend>::G2;

fn crs_checks<B: Backend>() {
    let (crs, td) = setup_from_seed::<B>(b"crs");
    assert!(check_crs(&crs), "honest CRS");
    let g1 = B::g1();
    let g2 = B::g2();
    let tampered_g1: [G1Field<B>; 7] = [
        |c| &mut c.g1,
        |c| &mut c.beta1,
        |c| &mut c.beta2_1,
        |c| &mut c.eta1,
        |c| &mut c.etabeta1,
        |c| &mut c.kappa1,
        |c| &mut c.kappabeta1,
    ];
    for (k, f) in tampered_g1.iter().enumerate() {
        let mut c = crs.clone();
        *f(&mut c) = *f(&mut c) + g1;
        assert!(!check_crs(&c), "tampered G1 element {k} accepted");
    }
    let tampered_g2: [G2Field<B>; 5] = [
        |c| &mut c.g2,
        |c| &mut c.beta_2,
        |c| &mut c.beta2_2,
        |c| &mut c.eta_2,
        |c| &mut c.kappa_2,
    ];
    for (k, f) in tampered_g2.iter().enumerate() {
        let mut c = crs.clone();
        *f(&mut c) = *f(&mut c) + g2;
        assert!(!check_crs(&c), "tampered G2 element {k} accepted");
    }
    let t = B::Fr::from(123456789u64);
    for e in [1u64, 2, 77] {
        let (p1, bp1, p2) = (phi1(&crs, &t, e), bphi1(&crs, &t, e), phi2(&crs, &t, e));
        assert_eq!(B::pair(&p1, &crs.g2), B::pair(&crs.g1, &p2));
        assert_eq!(B::pair(&bp1, &crs.g2), B::pair(&p1, &crs.beta_2));
        let phi = td.beta * t + td.eta + td.kappa * B::Fr::from(e);
        assert_eq!(p1, B::g1().smul(&phi));
    }
}

#[test]
fn crs_exp61() {
    crs_checks::<Exp61>();
}

#[test]
fn crs_bls() {
    crs_checks::<Bls>();
}

#[test]
fn encoding_lengths_bls() {
    let mut r = rng();
    let mut chain = TestChain::<Bls>::new(4, &mut r);
    chain.mint(1, 100);
    let (tx, _) = chain.build(1, &[2, 3, 4], &[5, 0, 0], &mut r).unwrap();
    let n = 3;
    assert_eq!(
        enc_core(&tx).len(),
        8 + 4 + 4 + 4 * n + 2 * 128 + n * (5 * 128 + 32)
    );
    assert_eq!(enc_handles(&tx).len(), 2 * 128 + n * 3 * 256);
    let rec = apply::credit_record(&tx, 0, tx_tag(&tx));
    assert_eq!(enc_record(&rec).len(), 32 + 3 * 128 + 3 * 256 + 1);
}

#[test]
fn tag_and_txhash_bindings() {
    let mut r = rng();
    let mut chain = TestChain::<Exp61>::new(3, &mut r);
    chain.mint(1, 100);
    let (tx, _) = chain.build(1, &[2, 3], &[7, 0], &mut r).unwrap();
    let (t, h) = (tx_tag(&tx), txhash(&tx));
    let one = Exp61::g1();

    let mut x = tx.clone();
    x.recv[1].d = x.recv[1].d + one;
    assert_ne!(tx_tag(&x), t);
    assert_ne!(txhash(&x), h);

    let mut x = tx.clone();
    x.recv[1].side[0] ^= 1;
    assert_ne!(tx_tag(&x), t);

    let mut x = tx.clone();
    x.epoch += 1;
    assert_ne!(tx_tag(&x), t);

    let mut x = tx.clone();
    x.recv[0].h1 = x.recv[0].h1 + Exp61::g2();
    assert_eq!(tx_tag(&x), t);
    assert_ne!(txhash(&x), h);

    let mut x = tx.clone();
    x.n0 = x.n0 + one;
    assert_eq!(tx_tag(&x), t);
    assert_ne!(txhash(&x), h);

    assert_ne!(batch_weight::<Exp61>(&h, 0), batch_weight::<Exp61>(&h, 1));
    assert_ne!(
        batch_weight::<Exp61>(&h, 0),
        batch_weight::<Exp61>(&txhash(&x), 0)
    );
}

#[test]
fn record_chain() {
    let mut r = rng();
    let mut chain = TestChain::<Exp61>::new(3, &mut r);
    chain.mint(1, 100);
    let (a, _) = chain.build(1, &[2, 3], &[1, 1], &mut r).unwrap();
    let (b, _) = chain.build(1, &[2, 3], &[2, 2], &mut r).unwrap();
    let ra = apply::credit_record(&a, 0, tx_tag(&a));
    let rb = apply::credit_record(&b, 0, tx_tag(&b));
    assert_eq!(chain_of::<Exp61>(&[]), [0u8; 32]);
    assert_eq!(
        chain_of(std::slice::from_ref(&ra)),
        chain_step(&[0u8; 32], &ra)
    );
    assert_ne!(
        chain_of(&[ra.clone(), rb.clone()]),
        chain_of(&[rb.clone(), ra.clone()])
    );
    let mut rb2 = rb.clone();
    rb2.is_update = false;
    assert_ne!(chain_of(&[ra.clone(), rb.clone()]), chain_of(&[ra, rb2]));
}
