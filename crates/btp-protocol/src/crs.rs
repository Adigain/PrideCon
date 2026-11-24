//! Setup and the derived CRS elements phi1, bphi1, phi2 (SPEC 5).

use crate::hash::{hash_to_fr, TAG_CRS};
use crate::types::{Crs, Trapdoor};
use btp_algebra::{Backend, GroupElem};

pub fn crs_from_trapdoor<B: Backend>(td: &Trapdoor<B>) -> Crs<B> {
    let (g1, g2) = (B::g1(), B::g2());
    let (b, h, k) = (td.beta, td.eta, td.kappa);
    Crs {
        g1,
        beta1: g1.smul(&b),
        beta2_1: g1.smul(&(b * b)),
        eta1: g1.smul(&h),
        etabeta1: g1.smul(&(h * b)),
        kappa1: g1.smul(&k),
        kappabeta1: g1.smul(&(k * b)),
        g2,
        beta_2: g2.smul(&b),
        beta2_2: g2.smul(&(b * b)),
        eta_2: g2.smul(&h),
        kappa_2: g2.smul(&k),
    }
}

pub fn setup_from_seed<B: Backend>(seed: &[u8]) -> (Crs<B>, Trapdoor<B>) {
    let derive = |i: u8| hash_to_fr::<B::Fr>(TAG_CRS, &[seed, &[i]].concat());
    let td = Trapdoor {
        beta: derive(0),
        eta: derive(1),
        kappa: derive(2),
    };
    (crs_from_trapdoor(&td), td)
}

pub fn check_crs<B: Backend>(crs: &Crs<B>) -> bool {
    let eq = |a: &B::G1, b: &B::G2, c: &B::G1, d: &B::G2| B::pair(a, b) == B::pair(c, d);
    let c = crs;
    c.g1 == B::g1()
        && c.g2 == B::g2()
        && !c.beta1.is_zero()
        && !c.eta1.is_zero()
        && !c.kappa1.is_zero()
        && eq(&c.beta1, &c.g2, &c.g1, &c.beta_2)
        && eq(&c.beta2_1, &c.g2, &c.beta1, &c.beta_2)
        && eq(&c.g1, &c.beta2_2, &c.beta1, &c.beta_2)
        && eq(&c.eta1, &c.g2, &c.g1, &c.eta_2)
        && eq(&c.etabeta1, &c.g2, &c.eta1, &c.beta_2)
        && eq(&c.kappa1, &c.g2, &c.g1, &c.kappa_2)
        && eq(&c.kappabeta1, &c.g2, &c.kappa1, &c.beta_2)
}

fn fr_epoch<B: Backend>(e: u64) -> B::Fr {
    B::Fr::from(e)
}

pub fn phi1<B: Backend>(crs: &Crs<B>, t: &B::Fr, e: u64) -> B::G1 {
    crs.beta1.smul(t) + crs.eta1 + crs.kappa1.smul(&fr_epoch::<B>(e))
}

pub fn bphi1<B: Backend>(crs: &Crs<B>, t: &B::Fr, e: u64) -> B::G1 {
    crs.beta2_1.smul(t) + crs.etabeta1 + crs.kappabeta1.smul(&fr_epoch::<B>(e))
}

pub fn phi2<B: Backend>(crs: &Crs<B>, t: &B::Fr, e: u64) -> B::G2 {
    crs.beta_2.smul(t) + crs.eta_2 + crs.kappa_2.smul(&fr_epoch::<B>(e))
}
