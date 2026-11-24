//! Conflict model (SPEC 17): when payments land and become forward-secure, per scheme.

use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rand_distr::{Distribution, Poisson, Zipf};
use std::collections::VecDeque;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub enum Scheme {
    Ours,
    PriDeFulOption2,
    PriDeFulOption1,
    PrideCt,
    ConstructionII,
}

impl Scheme {
    pub const ALL: [Scheme; 5] = [
        Scheme::Ours,
        Scheme::PriDeFulOption2,
        Scheme::PriDeFulOption1,
        Scheme::PrideCt,
        Scheme::ConstructionII,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Scheme::Ours => "ours",
            Scheme::PriDeFulOption2 => "PriDeFUL option 2",
            Scheme::PriDeFulOption1 => "PriDeFUL option 1",
            Scheme::PrideCt => "PriDe CT",
            Scheme::ConstructionII => "Construction II",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Config {
    pub accounts: usize,
    pub ring: usize,
    pub rate: f64,
    pub zipf_s: f64,
    pub epoch_len: u64,
    pub k_max: u32,
    pub epochs: u64,
    pub drain_epochs: u64,
    pub capacity: Option<usize>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            accounts: 10_000,
            ring: 8,
            rate: 50.0,
            zipf_s: 0.0,
            epoch_len: 16,
            k_max: 8,
            epochs: 200,
            drain_epochs: 50,
            capacity: None,
        }
    }
}

pub struct Arrivals {
    pub ring: usize,
    pub block: Vec<u64>,
    pub sender: Vec<u32>,
    pub rings: Vec<u32>,
}

impl Arrivals {
    pub fn len(&self) -> usize {
        self.block.len()
    }
    pub fn is_empty(&self) -> bool {
        self.block.is_empty()
    }
    pub fn ring_of(&self, i: usize) -> &[u32] {
        &self.rings[i * self.ring..(i + 1) * self.ring]
    }
}

pub fn arrivals(cfg: &Config, seed: u64) -> Arrivals {
    assert!(
        cfg.accounts > cfg.ring,
        "need more accounts than ring members"
    );
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    let poisson = Poisson::new(cfg.rate).expect("rate > 0");
    let zipf =
        (cfg.zipf_s > 0.0).then(|| Zipf::new(cfg.accounts as u64, cfg.zipf_s).expect("zipf"));
    let n = cfg.accounts as u32;
    let mut a = Arrivals {
        ring: cfg.ring,
        block: vec![],
        sender: vec![],
        rings: vec![],
    };
    for b in 0..cfg.epochs * cfg.epoch_len {
        let k: f64 = poisson.sample(&mut rng);
        for _ in 0..k as u64 {
            let s = rng.gen_range(0..n);
            let real = loop {
                let r = match &zipf {
                    Some(z) => z.sample(&mut rng) as u32 - 1,
                    None => rng.gen_range(0..n),
                };
                if r != s {
                    break r;
                }
            };
            let start = a.rings.len();
            a.rings.push(real);
            while a.rings.len() - start < cfg.ring {
                let d = rng.gen_range(0..n);
                if d != s && !a.rings[start..].contains(&d) {
                    a.rings.push(d);
                }
            }
            a.block.push(b);
            a.sender.push(s);
        }
    }
    a
}

#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub landed: Option<u64>,
    pub fs: Option<u64>,
    pub attempts: u32,
    pub hindered: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Land,
    DeferDebited,
    DeferCap,
}

#[derive(Clone, Debug, Default)]
pub struct KHist {
    pub counts: Vec<u64>,
    pub nonzero: u64,
}

impl KHist {
    fn add(&mut self, k: u32) {
        let k = k as usize;
        if self.counts.len() <= k {
            self.counts.resize(k + 1, 0);
        }
        self.counts[k] += 1;
        self.nonzero += 1;
    }
}

pub fn simulate(
    scheme: Scheme,
    cfg: &Config,
    arr: &Arrivals,
    seed: u64,
    mut log: Option<&mut Vec<(u64, usize, Decision)>>,
    mut khist: Option<&mut KHist>,
) -> Vec<Outcome> {
    let n = cfg.accounts;
    let l = cfg.epoch_len;
    let update_phase = (l / 4).max(1);
    let mut rng = ChaCha20Rng::seed_from_u64(seed ^ 0x5e_ed0f_5c4e_3e00 ^ scheme as u64);
    let mut out = vec![Outcome::default(); arr.len()];
    let mut not_before: Vec<u64> = arr.block.iter().map(|b| b + 1).collect();

    let mut queue: Vec<VecDeque<usize>> = vec![VecDeque::new(); n];
    let mut active: Vec<u32> = vec![];
    let mut is_active = vec![false; n];
    let mut debited = vec![0u64; n];
    let mut credit_epoch = vec![0u64; n];
    let mut credits = vec![0u32; n];
    let mut touched = vec![0u64; n];

    let mut next_arrival = 0;
    let last_block = (cfg.epochs + cfg.drain_epochs) * l;
    let mut pending = arr.len();
    let mut b = 0u64;
    while pending > 0 && b <= last_block {
        while next_arrival < arr.len() && arr.block[next_arrival] < b {
            let s = arr.sender[next_arrival] as usize;
            queue[s].push_back(next_arrival);
            if !is_active[s] {
                is_active[s] = true;
                active.push(s as u32);
            }
            next_arrival += 1;
        }
        let e = b / l;
        let mut cands: Vec<usize> = active
            .iter()
            .map(|&s| queue[s as usize][0])
            .filter(|&p| not_before[p] <= b)
            .collect();
        cands.shuffle(&mut rng);
        if let Some(cap) = cfg.capacity {
            for &p in cands.iter().skip(cap) {
                out[p].hindered = true;
            }
            cands.truncate(cap);
        }
        for p in cands {
            let s = arr.sender[p] as usize;
            let ring = arr.ring_of(p);
            let landed = match scheme {
                Scheme::Ours => {
                    let decision = if debited[s] == e + 1 {
                        Decision::DeferDebited
                    } else if ring.iter().any(|&m| {
                        credit_epoch[m as usize] == e + 1 && credits[m as usize] >= cfg.k_max
                    }) {
                        Decision::DeferCap
                    } else {
                        Decision::Land
                    };
                    if let Some(lg) = log.as_deref_mut() {
                        lg.push((b, p, decision));
                    }
                    out[p].attempts = 1;
                    if decision == Decision::Land {
                        debited[s] = e + 1;
                        for &m in ring {
                            let m = m as usize;
                            if credit_epoch[m] != e + 1 {
                                if credits[m] > 0 {
                                    if let Some(h) = khist.as_deref_mut() {
                                        h.add(credits[m]);
                                    }
                                }
                                credit_epoch[m] = e + 1;
                                credits[m] = 0;
                            }
                            credits[m] += 1;
                        }
                        out[p].fs = Some((e + 1) * l);
                        true
                    } else {
                        out[p].hindered = true;
                        not_before[p] = (e + 1) * l;
                        false
                    }
                }
                Scheme::PriDeFulOption2 => {
                    out[p].attempts += 1;
                    let conflict =
                        touched[s] == b + 1 || ring.iter().any(|&m| touched[m as usize] == b + 1);
                    if conflict {
                        out[p].hindered = true;
                        false
                    } else {
                        touched[s] = b + 1;
                        for &m in ring {
                            touched[m as usize] = b + 1;
                        }
                        out[p].fs = Some(b);
                        true
                    }
                }
                Scheme::PriDeFulOption1 => {
                    out[p].attempts = 1;
                    if touched[ring[0] as usize] != b + 1 {
                        out[p].fs = Some(b);
                    }
                    for &m in ring {
                        touched[m as usize] = b + 1;
                    }
                    true
                }
                Scheme::PrideCt => {
                    out[p].attempts = 1;
                    true
                }
                Scheme::ConstructionII => {
                    out[p].attempts = 1;
                    if b % l < l - update_phase {
                        out[p].fs = Some((e + 1) * l);
                        true
                    } else {
                        out[p].hindered = true;
                        not_before[p] = (e + 1) * l;
                        false
                    }
                }
            };
            if landed {
                out[p].landed = Some(b);
                pending -= 1;
                queue[s].pop_front();
                if let Some(&next) = queue[s].front() {
                    not_before[next] = not_before[next].max(b + 1);
                }
            }
        }
        active.retain(|&s| {
            let keep = !queue[s as usize].is_empty();
            is_active[s as usize] = keep;
            keep
        });
        b += 1;
    }
    if let Some(h) = khist {
        for (m, &c) in credits.iter().enumerate() {
            if credit_epoch[m] > 0 && c > 0 {
                h.add(c);
            }
        }
    }
    out
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Stats {
    pub payments: usize,
    pub landed: usize,
    pub unfinished: usize,
    pub first_try_rate: f64,
    pub attempts_mean: f64,
    pub latency_mean: f64,
    pub latency_p50: u64,
    pub latency_p95: u64,
    pub fs_fraction: f64,
    pub fs_time_mean: f64,
    pub fs_time_p95: u64,
    pub messages_per_payment: f64,
}

fn pct(sorted: &[u64], q: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

pub fn stats(scheme: Scheme, arr: &Arrivals, out: &[Outcome]) -> Stats {
    let n = out.len().max(1) as f64;
    let mut lat: Vec<u64> = vec![];
    let mut fst: Vec<u64> = vec![];
    let (mut first, mut attempts) = (0usize, 0u64);
    for (i, o) in out.iter().enumerate() {
        attempts += o.attempts as u64;
        if let Some(b) = o.landed {
            lat.push(b - arr.block[i]);
            first += !o.hindered as usize;
        }
        if let Some(f) = o.fs {
            fst.push(f - arr.block[i]);
        }
    }
    lat.sort_unstable();
    fst.sort_unstable();
    let mean = |v: &[u64]| {
        if v.is_empty() {
            0.0
        } else {
            v.iter().sum::<u64>() as f64 / v.len() as f64
        }
    };
    let per_msg = if scheme == Scheme::ConstructionII {
        2.0
    } else {
        1.0
    };
    Stats {
        payments: out.len(),
        landed: lat.len(),
        unfinished: out.len() - lat.len(),
        first_try_rate: first as f64 / n,
        attempts_mean: attempts as f64 / n,
        latency_mean: mean(&lat),
        latency_p50: pct(&lat, 0.5),
        latency_p95: pct(&lat, 0.95),
        fs_fraction: fst.len() as f64 / n,
        fs_time_mean: mean(&fst),
        fs_time_p95: pct(&fst, 0.95),
        messages_per_payment: per_msg * attempts as f64 / n,
    }
}

pub fn e5_round(
    rng: &mut ChaCha20Rng,
    accounts: usize,
    senders: usize,
    n: usize,
    k_max: u32,
) -> (usize, usize) {
    let ids: Vec<u32> = (0..accounts as u32).collect();
    let chosen: Vec<u32> = ids.choose_multiple(rng, senders).copied().collect();
    let mut touched = vec![false; accounts];
    let mut credits = vec![0u32; accounts];
    let (mut opt2, mut ours) = (0, 0);
    for s in chosen {
        let others: Vec<u32> = ids.iter().copied().filter(|&x| x != s).collect();
        let ring: Vec<u32> = others.choose_multiple(rng, n).copied().collect();
        if !touched[s as usize] && ring.iter().all(|&m| !touched[m as usize]) {
            opt2 += 1;
            touched[s as usize] = true;
            ring.iter().for_each(|&m| touched[m as usize] = true);
        }
        if ring.iter().all(|&m| credits[m as usize] < k_max) {
            ours += 1;
            ring.iter().for_each(|&m| credits[m as usize] += 1);
        }
    }
    (opt2, ours)
}
