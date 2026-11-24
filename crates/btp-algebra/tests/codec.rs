//! EIP-2537 codec: round-trips, generator bytes and rejection of invalid encodings.

use ark_bls12_381::{g1, g2, Fq, Fq2, Fr};
use ark_ec::short_weierstrass::Affine;
use ark_ff::{BigInteger, PrimeField, UniformRand};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use btp_algebra::bls::{Bls, G1Projective, G2Projective};
use btp_algebra::codec::*;
use btp_algebra::{Backend, GroupElem};

fn rng() -> StdRng {
    StdRng::seed_from_u64(2537)
}

fn coord(hex48: &str) -> Vec<u8> {
    let mut v = vec![0u8; 16];
    v.extend(hex::decode(hex48).unwrap());
    v
}

const G1X: &str = "17f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb";
const G1Y: &str = "08b3f481e3aaa0f1a09e30ed741d8ae4fcf5e095d5d00af600db18cb2c04b3edd03cc744a2888ae40caa232946c5e7e1";
const G2XC0: &str = "024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8";
const G2XC1: &str = "13e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e";
const G2YC0: &str = "0ce5d527727d6e118cc9cdc6da2e351aadfd9baa8cbdd3a76d429a695160d12c923ac9cc3baca289e193548608b82801";
const G2YC1: &str = "0606c4a02ea734cc32acd2b02bc28b99cb3e287e85a763af267492ab572e99ab3f370d275cec1da1aaa9075ff05f79be";

#[test]
fn generators_match_eip2537_constants() {
    let g1 = [coord(G1X), coord(G1Y)].concat();
    assert_eq!(g1_to_eip2537(&Bls::g1()).to_vec(), g1);
    let g2 = [coord(G2XC0), coord(G2XC1), coord(G2YC0), coord(G2YC1)].concat();
    assert_eq!(
        g2_to_eip2537(&Bls::g2()).to_vec(),
        g2,
        "G2 order must be x.c0, x.c1, y.c0, y.c1"
    );
}

#[test]
fn infinity_is_all_zeros() {
    assert_eq!(g1_to_eip2537(&G1Projective::zero()), [0u8; 128]);
    assert_eq!(g2_to_eip2537(&G2Projective::zero()), [0u8; 256]);
    assert!(g1_from_eip2537(&[0u8; 128]).unwrap().is_zero());
    assert!(g2_from_eip2537(&[0u8; 256]).unwrap().is_zero());
}

#[test]
fn random_points_round_trip() {
    let mut r = rng();
    for _ in 0..20 {
        let p = Bls::g1().smul(&Fr::rand(&mut r));
        let q = Bls::g2().smul(&Fr::rand(&mut r));
        assert_eq!(g1_from_eip2537(&g1_to_eip2537(&p)), Some(p));
        assert_eq!(g2_from_eip2537(&g2_to_eip2537(&q)), Some(q));
        let b = g2_to_eip2537(&q);
        assert!((0..4).all(|i| b[64 * i..64 * i + 16].iter().all(|&x| x == 0)));
    }
}

#[test]
fn scalars_round_trip() {
    let mut r = rng();
    for _ in 0..50 {
        let s = Fr::rand(&mut r);
        assert_eq!(fr_from_be_bytes_mod::<Fr>(&fr_to_bytes32(&s)), s);
    }
    let r_minus_1 = -Fr::from(1u64);
    let expect =
        hex::decode("73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000000").unwrap();
    assert_eq!(fr_to_bytes32(&r_minus_1).to_vec(), expect);
}

#[test]
fn rejects_nonzero_padding() {
    let mut b = g1_to_eip2537(&Bls::g1());
    b[0] = 1;
    assert!(g1_from_eip2537(&b).is_none());
    let mut b = g2_to_eip2537(&Bls::g2());
    b[128] = 1;
    assert!(g2_from_eip2537(&b).is_none());
}

#[test]
fn rejects_coordinate_not_below_p() {
    let p_be = Fq::MODULUS.to_bytes_be();
    let mut b = g1_to_eip2537(&Bls::g1());
    b[16..64].copy_from_slice(&p_be);
    assert!(g1_from_eip2537(&b).is_none());
    assert!(fp_from_64(&[vec![0u8; 16], p_be.clone()].concat()).is_none());
    let p_minus_1 = (-Fq::from(1u64)).into_bigint().to_bytes_be();
    assert!(fp_from_64(&[vec![0u8; 16], p_minus_1].concat()).is_some());
}

#[test]
fn rejects_point_off_curve() {
    let mut b = g1_to_eip2537(&Bls::g1());
    b[127] ^= 1;
    assert!(g1_from_eip2537(&b).is_none());
    let mut b = g2_to_eip2537(&Bls::g2());
    b[255] ^= 1;
    assert!(g2_from_eip2537(&b).is_none());
}

#[test]
fn rejects_point_outside_subgroup() {
    let a1 = (1u64..)
        .filter_map(|k| Affine::<g1::Config>::get_point_from_x_unchecked(Fq::from(k), true))
        .find(|a| !a.is_in_correct_subgroup_assuming_on_curve())
        .unwrap();
    let b1: Vec<u8> = [fp_to_64(&a1.x), fp_to_64(&a1.y)].concat();
    assert!(a1.is_on_curve());
    assert!(g1_from_eip2537(&b1.try_into().unwrap()).is_none());

    let a2 = (1u64..)
        .filter_map(|k| {
            Affine::<g2::Config>::get_point_from_x_unchecked(
                Fq2::new(Fq::from(k), Fq::from(1u64)),
                true,
            )
        })
        .find(|a| !a.is_in_correct_subgroup_assuming_on_curve())
        .unwrap();
    let b2: Vec<u8> = [
        fp_to_64(&a2.x.c0),
        fp_to_64(&a2.x.c1),
        fp_to_64(&a2.y.c0),
        fp_to_64(&a2.y.c1),
    ]
    .concat();
    assert!(a2.is_on_curve());
    assert!(g2_from_eip2537(&b2.try_into().unwrap()).is_none());
}
