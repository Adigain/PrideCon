//! The same algebraic identities hold on every backend (Exp61, ExpBls, Bls).

use ark_ff::{Field, UniformRand};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use btp_algebra::bls::Bls;
use btp_algebra::exp::{Exp61, ExpBls};
use btp_algebra::{Backend, GroupElem};

fn rng() -> StdRng {
    StdRng::seed_from_u64(2537)
}

fn group_laws<F: Field + ark_ff::PrimeField, G: GroupElem<F>>(a: F, b: F) {
    let g = G::generator();
    assert_eq!(g.smul(&a) + g.smul(&b), g.smul(&(a + b)), "distributivity");
    assert_eq!(g.smul(&a) - g.smul(&b), g.smul(&(a - b)), "subtraction");
    assert_eq!(-g.smul(&a), g.smul(&(-a)), "negation");
    assert_eq!(g.smul(&a).smul(&b), g.smul(&(a * b)), "repeated smul");
    assert!(g.smul(&F::zero()).is_zero(), "times 0");
    assert_eq!(g.smul(&F::one()), g, "times 1");
    assert!((g.smul(&a) - g.smul(&a)).is_zero(), "x - x");
    assert_eq!(g.smul(&a) + G::zero(), g.smul(&a), "zero is neutral");
    assert!(!g.is_zero(), "generator is not zero");
}

fn group_laws_all<B: Backend>(rounds: usize) {
    let mut r = rng();
    for _ in 0..rounds {
        let (a, b) = (B::Fr::rand(&mut r), B::Fr::rand(&mut r));
        group_laws::<B::Fr, B::G1>(a, b);
        group_laws::<B::Fr, B::G2>(a, b);
        group_laws::<B::Fr, B::Gt>(a, b);
    }
}

fn bilinearity<B: Backend>(rounds: usize) {
    let mut r = rng();
    assert_eq!(B::pair(&B::g1(), &B::g2()), B::gt(), "gt is e(g1, g2)");
    assert!(!B::gt().is_zero(), "pairing is non-degenerate");
    assert!(B::pair(&B::G1::zero(), &B::g2()).is_zero());
    assert!(B::pair(&B::g1(), &B::G2::zero()).is_zero());
    for _ in 0..rounds {
        let (a, b, c) = (
            B::Fr::rand(&mut r),
            B::Fr::rand(&mut r),
            B::Fr::rand(&mut r),
        );
        let (p, q) = (B::g1().smul(&a), B::g2().smul(&b));
        assert_eq!(
            B::pair(&p, &q),
            B::gt().smul(&(a * b)),
            "e([a]1,[b]2) = [ab]T"
        );
        let p2 = B::g1().smul(&c);
        assert_eq!(
            B::pair(&(p + p2), &q),
            B::pair(&p, &q) + B::pair(&p2, &q),
            "linear in G1"
        );
        let q2 = B::g2().smul(&c);
        assert_eq!(
            B::pair(&p, &(q + q2)),
            B::pair(&p, &q) + B::pair(&p, &q2),
            "linear in G2"
        );
        assert_eq!(
            B::pair(&p.smul(&c), &q),
            B::pair(&p, &q.smul(&c)),
            "move scalar across"
        );
    }
}

fn multi_ops<B: Backend>(sizes: &[usize]) {
    let mut r = rng();
    for &n in sizes {
        let pairs: Vec<(B::G1, B::G2)> = (0..n)
            .map(|_| {
                (
                    B::g1().smul(&B::Fr::rand(&mut r)),
                    B::g2().smul(&B::Fr::rand(&mut r)),
                )
            })
            .collect();
        let naive = pairs
            .iter()
            .fold(B::Gt::zero(), |acc, (a, b)| acc + B::pair(a, b));
        assert_eq!(B::multi_pair(&pairs), naive, "multi_pair, n = {n}");

        let points: Vec<B::G1> = pairs.iter().map(|(a, _)| *a).collect();
        let scalars: Vec<B::Fr> = (0..n).map(|_| B::Fr::rand(&mut r)).collect();
        let naive = points
            .iter()
            .zip(&scalars)
            .fold(B::G1::zero(), |acc, (p, s)| acc + p.smul(s));
        assert_eq!(B::msm_g1(&points, &scalars), naive, "msm_g1, n = {n}");
    }
    let a = B::Fr::rand(&mut r);
    let cancel = [(B::g1().smul(&a), B::g2()), (-B::g1(), B::g2().smul(&a))];
    assert!(
        B::multi_pair(&cancel).is_zero(),
        "e([a]1,g2) * e(-g1,[a]2) = 1"
    );
}

fn encodings<B: Backend>(g1_len: usize, g2_len: usize) {
    let mut r = rng();
    let (a, b) = (B::Fr::rand(&mut r), B::Fr::rand(&mut r));
    let p = B::g1().smul(&a) + B::g1().smul(&b);
    let q = B::g2().smul(&a) + B::g2().smul(&b);
    assert_eq!(B::enc_g1(&p), B::enc_g1(&B::g1().smul(&(a + b))));
    assert_eq!(B::enc_g2(&q), B::enc_g2(&B::g2().smul(&(a + b))));
    assert_ne!(B::enc_g1(&p), B::enc_g1(&B::g1().smul(&a)));
    assert_eq!(B::enc_g1(&p).len(), g1_len);
    assert_eq!(B::enc_g2(&q).len(), g2_len);
}

macro_rules! backend_suite {
    ($name:ident, $B:ty, rounds = $rounds:expr, sizes = $sizes:expr, enc = ($l1:expr, $l2:expr)) => {
        mod $name {
            use super::*;
            #[test]
            fn group_laws() {
                group_laws_all::<$B>($rounds);
            }
            #[test]
            fn bilinearity() {
                super::bilinearity::<$B>($rounds);
            }
            #[test]
            fn multi_pair_and_msm() {
                multi_ops::<$B>(&$sizes);
            }
            #[test]
            fn hash_encodings() {
                encodings::<$B>($l1, $l2);
            }
        }
    };
}

backend_suite!(
    exp61,
    Exp61,
    rounds = 500,
    sizes = [1, 2, 7, 33],
    enc = (32, 32)
);
backend_suite!(
    exp_bls,
    ExpBls,
    rounds = 500,
    sizes = [1, 2, 7, 33],
    enc = (32, 32)
);
backend_suite!(
    bls,
    Bls,
    rounds = 4,
    sizes = [1, 2, 7, 33],
    enc = (128, 256)
);
