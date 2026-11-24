//! Generic-bilinear-group leak checker (port of reference/ggm.py and reference/tdddh.py).

use ark_ff::{Field, One, Zero};
use btp_algebra::f61::F61;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use std::collections::{BTreeMap, HashMap};
use std::ops::{Add, Mul, Neg, Sub};

type F = F61;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Pol(BTreeMap<Vec<u8>, F>);

impl Pol {
    pub fn constant(c: F, nvars: usize) -> Pol {
        Pol::default().with(vec![0; nvars], c)
    }
    pub fn var(i: usize, nvars: usize) -> Pol {
        let mut m = vec![0; nvars];
        m[i] = 1;
        Pol::default().with(m, F::one())
    }
    fn with(mut self, m: Vec<u8>, c: F) -> Pol {
        let e = self.0.entry(m).or_insert_with(F::zero);
        *e += c;
        self.0.retain(|_, v| !v.is_zero());
        self
    }
}

impl Add for &Pol {
    type Output = Pol;
    fn add(self, o: &Pol) -> Pol {
        o.0.iter()
            .fold(self.clone(), |acc, (m, c)| acc.with(m.clone(), *c))
    }
}
impl Neg for &Pol {
    type Output = Pol;
    fn neg(self) -> Pol {
        Pol(self.0.iter().map(|(m, c)| (m.clone(), -*c)).collect())
    }
}
impl Sub for &Pol {
    type Output = Pol;
    fn sub(self, o: &Pol) -> Pol {
        self + &(-o)
    }
}
impl Mul for &Pol {
    type Output = Pol;
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn mul(self, o: &Pol) -> Pol {
        let mut out = Pol::default();
        for (m1, c1) in &self.0 {
            for (m2, c2) in &o.0 {
                let m: Vec<u8> = m1.iter().zip(m2).map(|(a, b)| a + b).collect();
                out = out.with(m, *c1 * c2);
            }
        }
        out
    }
}

pub fn rank(polys: &[Pol]) -> usize {
    let mut cols: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut piv: HashMap<usize, BTreeMap<usize, F>> = HashMap::new();
    let mut r = 0;
    for q in polys {
        let mut v: BTreeMap<usize, F> =
            q.0.iter()
                .map(|(m, c)| {
                    let n = cols.len();
                    (*cols.entry(m.clone()).or_insert(n), *c)
                })
                .collect();
        while let Some((&lead, &f)) = v.iter().next() {
            match piv.get(&lead) {
                Some(row) => {
                    for (k, c) in row {
                        let nv = v.get(k).copied().unwrap_or_else(F::zero) - f * c;
                        if nv.is_zero() {
                            v.remove(k);
                        } else {
                            v.insert(*k, nv);
                        }
                    }
                }
                None => {
                    let inv = f.inverse().expect("non-zero lead");
                    piv.insert(lead, v.iter().map(|(k, c)| (*k, *c * inv)).collect());
                    r += 1;
                    break;
                }
            }
        }
    }
    r
}

fn products(g1: &[Pol], g2: &[Pol]) -> Vec<Pol> {
    g1.iter()
        .flat_map(|a| g2.iter().map(move |c| a * c))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RankResult {
    pub real: (usize, usize, usize),
    pub rand: (usize, usize, usize),
}

impl RankResult {
    pub fn leak(&self) -> bool {
        self.real.0 < self.rand.0 || self.real.1 < self.rand.1 || self.real.2 < self.rand.2
    }
}

pub struct RankCase {
    pub game: &'static str,
    pub name: &'static str,
    pub expected_leak: bool,
    pub run: fn(&mut ChaCha20Rng) -> RankResult,
}

const GGM_VARS: [&str; 19] = [
    "x0", "b", "r0", "r1", "d1", "tau1", "sg1", "pl1", "q1", "rs", "ds", "taus", "sgs", "qs", "be",
    "eta", "uu", "vv", "U",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum GgmScheme {
    OriginalSingle,
    MethodA,
    Tagged,
}

struct GgmOpts {
    scheme: GgmScheme,
    b_known: bool,
    x0_zero: bool,
    epoch_layer: bool,
    honest_other: bool,
}

fn ggm_run(o: GgmOpts, rng: &mut ChaCha20Rng) -> RankResult {
    let n = GGM_VARS.len();
    let v = |name: &str| Pol::var(GGM_VARS.iter().position(|x| *x == name).unwrap(), n);
    let k = |c: F| Pol::constant(c, n);
    let one = k(F::one());
    let mut rnd = || F::from(rng.gen_range(2u64..(1u64 << 61) - 1));

    let sp = rnd();
    let s = &(&k(sp) - &v("ds"))
        - &if o.honest_other {
            v("d1")
        } else {
            Pol::default()
        };
    let (p, t0, t1, ts) = (rnd(), rnd(), rnd(), rnd());
    let (e0, e) = (F::from(3u64), F::from(4u64));
    let bb = if o.b_known { k(rnd()) } else { v("b") };
    let x = &(if o.x0_zero { Pol::default() } else { v("x0") }) + &v("r0");
    let (be, eta, uu, vv) = (v("be"), v("eta"), v("uu"), v("vv"));

    let mut out = vec![];
    for pay in [k(p), v("U")] {
        let mut g1 = vec![one.clone(), s.clone(), x.clone(), &(&x * &s) + &bb];
        let mut g2 = vec![one.clone()];
        let mut txs = vec![(
            v("rs"),
            v("ds"),
            v("taus"),
            v("sgs"),
            v("qs"),
            pay.clone(),
            ts,
        )];
        if o.honest_other {
            txs.push((v("r1"), v("d1"), v("tau1"), v("sg1"), v("q1"), v("pl1"), t1));
        }
        match o.scheme {
            GgmScheme::OriginalSingle => {
                let (r, d, _, _, q, pl, _) = &txs[0];
                g1.extend([
                    r.clone(),
                    &(r * &s) + pl,
                    q.clone(),
                    &(q * &s) + d,
                    d.clone(),
                    &(&x + r) * d,
                ]);
            }
            GgmScheme::MethodA => {
                for (r, d, _, _, q, pl, _) in &txs {
                    g1.extend([
                        r.clone(),
                        &(r * &s) + pl,
                        q.clone(),
                        &(q * &s) + d,
                        d.clone(),
                        &x * d,
                    ]);
                    g2.push(d.clone());
                }
            }
            GgmScheme::Tagged => {
                let bb2 = &be * &be;
                let etabe = &eta * &be;
                g1.extend([be.clone(), bb2.clone(), eta.clone(), etabe.clone()]);
                g2.extend([be.clone(), bb2.clone(), eta.clone(), etabe.clone()]);
                if o.epoch_layer {
                    g1.extend([uu.clone(), vv.clone()]);
                    g2.extend([uu.clone(), vv.clone()]);
                }
                let r0 = v("r0");
                let tagged = |t: F, r: &Pol| &(&(&bb2 * &k(t)) + &etabe) * r;
                g1.extend([r0.clone(), &be * &r0, tagged(t0, &r0)]);
                if o.epoch_layer {
                    g1.push(&(&(&uu * &k(e0)) + &vv) * &r0);
                }
                for (r, d, tau, sg, q, pl, t) in &txs {
                    g1.extend([
                        r.clone(),
                        &(r * &s) + pl,
                        q.clone(),
                        &(q * &s) + d,
                        d.clone(),
                        &(&x + r) * d,
                        &be * r,
                        tagged(*t, r),
                    ]);
                    let mut h0 = d + &(&bb2 * tau);
                    if o.epoch_layer {
                        let layer = &(&uu * &k(e)) + &vv;
                        g1.push(&layer * r);
                        h0 = &h0 + &(&layer * sg);
                        g2.push(sg.clone());
                    }
                    g2.extend([h0, &(&(&be * &k(*t)) + &eta) * tau, tau.clone()]);
                }
            }
        }
        out.push((0, 0, rank(&products(&g1, &g2))));
    }
    RankResult {
        real: out[0],
        rand: out[1],
    }
}

fn ggm(scheme: GgmScheme) -> GgmOpts {
    GgmOpts {
        scheme,
        b_known: false,
        x0_zero: false,
        epoch_layer: true,
        honest_other: true,
    }
}

const TD_VARS: [&str; 10] = ["be", "eta", "ka", "x", "th", "r", "d", "tau", "q", "U"];

#[derive(Default)]
struct TdOpts {
    known_total: bool,
    same_epoch: bool,
    raw_delta: bool,
    split_e: bool,
    no_x_handles: bool,
    x_known: bool,
}

fn td_game(o: TdOpts, rng: &mut ChaCha20Rng) -> RankResult {
    let n = TD_VARS.len();
    let v = |name: &str| Pol::var(TD_VARS.iter().position(|x| *x == name).unwrap(), n);
    let k = |c: F| Pol::constant(c, n);
    let one = k(F::one());
    let mut rnd = || F::from(rng.gen_range(2u64..(1u64 << 61) - 1));

    let (be, eta, ka, th, r, d, tau, q) = (
        v("be"),
        v("eta"),
        v("ka"),
        v("th"),
        v("r"),
        v("d"),
        v("tau"),
        v("q"),
    );
    let x = if o.x_known { k(rnd()) } else { v("x") };
    let lam = rnd();
    let s = &k(lam) - &d;
    let (t, t0) = (rnd(), rnd());
    let e = F::from(5u64);
    let e0 = if o.same_epoch { e } else { F::from(4u64) };
    let phi = &(&(&be * &k(t)) + &eta) + &(&ka * &k(e));
    let phi0 = &(&(&be * &k(t0)) + &eta) + &(&ka * &k(e0));
    let (tt, m) = (rnd(), rnd());

    let mut out = vec![];
    for pay in [k(m), v("U")] {
        let od = &(&x * &s)
            + &(if o.known_total {
                &k(tt) - &pay
            } else {
                th.clone()
            });
        let bb2 = &be * &be;
        let mut g1 = vec![
            one.clone(),
            be.clone(),
            bb2.clone(),
            eta.clone(),
            &eta * &be,
            ka.clone(),
            &ka * &be,
            s.clone(),
            x.clone(),
            od,
            r.clone(),
            &(&r * &s) + &pay,
            q.clone(),
            &(&q * &s) + &d,
            d.clone(),
            &be * &r,
            &(&be * &phi) * &r,
        ];
        if o.split_e {
            g1.extend([&x * &d, &r * &d]);
        } else {
            g1.push(&(&x + &r) * &d);
        }
        if !o.no_x_handles {
            g1.extend([&be * &x, &(&be * &phi0) * &x]);
        }
        let mut g2 = vec![
            one.clone(),
            be.clone(),
            bb2.clone(),
            eta.clone(),
            ka.clone(),
        ];
        if o.raw_delta {
            g2.push(d.clone());
        } else {
            g2.extend([&d + &(&bb2 * &tau), &phi * &tau, tau.clone()]);
        }
        out.push((rank(&g1), rank(&g2), rank(&products(&g1, &g2))));
    }
    RankResult {
        real: out[0],
        rand: out[1],
    }
}

pub fn cases() -> Vec<RankCase> {
    use GgmScheme::*;
    vec![
        RankCase {
            game: "ggm",
            name: "original PriDeFUL (single tx)",
            expected_leak: false,
            run: |r| {
                ggm_run(
                    GgmOpts {
                        honest_other: false,
                        ..ggm(OriginalSingle)
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "ggm",
            name: "original, prior balance b known",
            expected_leak: true,
            run: |r| {
                ggm_run(
                    GgmOpts {
                        honest_other: false,
                        b_known: true,
                        ..ggm(OriginalSingle)
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "ggm",
            name: "plan Method A (raw g2^delta)",
            expected_leak: true,
            run: |r| ggm_run(ggm(MethodA), r),
        },
        RankCase {
            game: "ggm",
            name: "tagged handles, no epoch binding, x0=0",
            expected_leak: true,
            run: |r| {
                ggm_run(
                    GgmOpts {
                        epoch_layer: false,
                        x0_zero: true,
                        ..ggm(Tagged)
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "ggm",
            name: "tagged handles, no epoch binding",
            expected_leak: false,
            run: |r| {
                ggm_run(
                    GgmOpts {
                        epoch_layer: false,
                        ..ggm(Tagged)
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "ggm",
            name: "tagged handles + epoch binding",
            expected_leak: false,
            run: |r| ggm_run(ggm(Tagged), r),
        },
        RankCase {
            game: "ggm",
            name: "tagged + epoch binding, x0=0",
            expected_leak: false,
            run: |r| {
                ggm_run(
                    GgmOpts {
                        x0_zero: true,
                        ..ggm(Tagged)
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "ggm",
            name: "tagged + epoch binding, tx* alone",
            expected_leak: false,
            run: |r| {
                ggm_run(
                    GgmOpts {
                        honest_other: false,
                        ..ggm(Tagged)
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "ggm",
            name: "tagged + epoch binding, b known",
            expected_leak: false,
            run: |r| {
                ggm_run(
                    GgmOpts {
                        b_known: true,
                        ..ggm(Tagged)
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "tdddh",
            name: "TD-DDH, hidden uniform balance (variant i)",
            expected_leak: false,
            run: |r| td_game(TdOpts::default(), r),
        },
        RankCase {
            game: "tdddh",
            name: "TD-DDH, known total, equal sums (variant ii)",
            expected_leak: false,
            run: |r| {
                td_game(
                    TdOpts {
                        known_total: true,
                        ..Default::default()
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "tdddh",
            name: "TD-DDH without prior-epoch x-handles",
            expected_leak: false,
            run: |r| {
                td_game(
                    TdOpts {
                        no_x_handles: true,
                        ..Default::default()
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "tdddh",
            name: "control: x-handles in SAME epoch",
            expected_leak: true,
            run: |r| {
                td_game(
                    TdOpts {
                        same_epoch: true,
                        ..Default::default()
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "tdddh",
            name: "control: raw g2^delta",
            expected_leak: true,
            run: |r| {
                td_game(
                    TdOpts {
                        raw_delta: true,
                        ..Default::default()
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "tdddh",
            name: "control: E split into x*d and r*d",
            expected_leak: true,
            run: |r| {
                td_game(
                    TdOpts {
                        split_e: true,
                        ..Default::default()
                    },
                    r,
                )
            },
        },
        RankCase {
            game: "tdddh",
            name: "control: epoch-start randomness x known",
            expected_leak: true,
            run: |r| {
                td_game(
                    TdOpts {
                        x_known: true,
                        no_x_handles: true,
                        ..Default::default()
                    },
                    r,
                )
            },
        },
    ]
}

pub fn run_case(case: &RankCase, seed: u64) -> RankResult {
    (case.run)(&mut ChaCha20Rng::seed_from_u64(seed))
}
